//! Redaction: taking what is under an area out of the content of a page, not
//! covering it (ISO 32000-2 has no operator for it: it is done by rewriting
//! the content streams).
//!
//! The content of the page is run once to know where every glyph, image and
//! path is painted, and then written again operation by operation, leaving
//! out what is under the areas: glyphs are taken out of the strings that show
//! them (the others keep their places), paths and images are dropped or, for
//! images, have the pixels under the area blanked, and the forms that hold
//! any of that are copied and rewritten the same way.

use std::collections::HashSet;

use crate::base::{Error, ErrorKind, Limits, Result};
use crate::content::{
    ContentParser, FormUse, Glyph, Interpreter, Operation, OperationTrace, Operator, PageContent,
    PlacedImage,
};
use crate::document::{Rectangle, Resources};
use crate::filters::{ImageFilter, decode_jpeg, deflate_zlib};
use crate::images::{PngColor, pixels, read_image};
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
use crate::writer::serialize::{write_dictionary, write_object};
use crate::xref::Resolver;

/// An area to redact, in the default user space of the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RedactArea {
    pub rect: Rectangle,
    /// The color of the box drawn over the area, red, green and blue from 0 to
    /// 255; `None` draws nothing (the content is removed and the area is left
    /// as it is).
    pub fill: Option<[u8; 3]>,
}

impl RedactArea {
    /// An area filled with black.
    pub fn black(rect: Rectangle) -> Self {
        Self {
            rect,
            fill: Some([0, 0, 0]),
        }
    }
}

/// What is done with the images an area touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageMode {
    /// The pixels under the area are blanked and the image is written again
    /// without compression artifacts of its own (a lossless copy); an image
    /// that can't be read is removed.
    #[default]
    Pixels,
    /// The whole image is removed.
    Remove,
    /// The image stays as it is (the box drawn over it hides it, but its
    /// data is still in the file).
    Keep,
}

/// What is done with the lines and shapes an area touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsMode {
    /// Those that are all inside the area are removed.
    #[default]
    Covered,
    /// Those that touch the area are removed.
    Touched,
    /// They stay.
    Keep,
}

/// How a page is redacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedactOptions {
    pub images: ImageMode,
    pub graphics: GraphicsMode,
    /// Whether the annotations that touch an area are removed (form fields
    /// never are).
    pub annotations: bool,
}

impl Default for RedactOptions {
    fn default() -> Self {
        Self {
            images: ImageMode::Pixels,
            graphics: GraphicsMode::Covered,
            annotations: true,
        }
    }
}

/// What a redaction took out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RedactReport {
    /// Glyphs taken out of strings of text.
    pub glyphs: usize,
    /// Images removed whole.
    pub images_removed: usize,
    /// Images that had pixels blanked.
    pub images_blanked: usize,
    /// Paths (lines and shapes) removed.
    pub paths: usize,
    /// Form XObjects copied to take something out of them.
    pub forms: usize,
    /// Annotations removed.
    pub annotations: usize,
    /// Form fields under an area, which are left as they are.
    pub widgets_left: usize,
}

impl RedactReport {
    pub(super) fn add(&mut self, other: &Self) {
        self.glyphs += other.glyphs;
        self.images_removed += other.images_removed;
        self.images_blanked += other.images_blanked;
        self.paths += other.paths;
        self.forms += other.forms;
        self.annotations += other.annotations;
        self.widgets_left += other.widgets_left;
    }

    /// Whether anything was taken out.
    pub fn is_empty(&self) -> bool {
        self.glyphs
            + self.images_removed
            + self.images_blanked
            + self.paths
            + self.forms
            + self.annotations
            == 0
    }
}

/// A content stream written again.
pub(super) struct Rewritten {
    pub data: Vec<u8>,
    /// Whether anything of it changed.
    pub changed: bool,
    /// The forms it paints in their rewritten copies, with the names it gives
    /// them.
    pub forms: Vec<(String, ObjectId, Rewritten)>,
    pub images: Vec<(String, NewImage)>,
}

/// An image made to replace one that had pixels blanked.
pub(super) struct NewImage {
    pub dictionary: Dictionary,
    pub data: Vec<u8>,
    pub mask: Option<(Dictionary, Vec<u8>)>,
}

// What the run of a stream showed, for deciding each of its operations.
#[derive(Clone, Copy)]
struct Seen<'x> {
    resources: &'x Resources,
    content: &'x PageContent,
    // For each glyph, whether it goes.
    removed: &'x [bool],
    // The marked-content sequences with a glyph that goes.
    tainted: &'x HashSet<usize>,
}

// Nested forms rewritten inside each other, at most.
const MAX_FORM_DEPTH: usize = 8;

pub(super) struct Rewriter<'a, 'd> {
    pub resolver: &'a mut Resolver<'d>,
    pub interpreter: Interpreter,
    pub areas: &'a [RedactArea],
    pub options: RedactOptions,
    pub limits: Limits,
    pub report: RedactReport,
    names: usize,
    // The forms being rewritten, the outermost first.
    forms: Vec<ObjectId>,
}

fn overlaps(a: &Rectangle, b: &Rectangle) -> bool {
    a.left < b.right && b.left < a.right && a.bottom < b.top && b.bottom < a.top
}

fn inside(inner: &Rectangle, outer: &Rectangle) -> bool {
    inner.left >= outer.left
        && inner.right <= outer.right
        && inner.bottom >= outer.bottom
        && inner.top <= outer.top
}

fn incomplete() -> Error {
    Error::new(ErrorKind::RedactionIncomplete, 0)
}

impl<'a, 'd> Rewriter<'a, 'd> {
    pub fn new(
        resolver: &'a mut Resolver<'d>,
        areas: &'a [RedactArea],
        options: RedactOptions,
        limits: Limits,
        first_name: usize,
    ) -> Self {
        Self {
            resolver,
            interpreter: Interpreter::painting(limits),
            areas,
            options,
            limits,
            report: RedactReport::default(),
            names: first_name,
            forms: Vec::new(),
        }
    }

    fn fresh_name(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix}{}", self.names)
    }

    // Whether a glyph is taken out: its box has its center in an area, or
    // a fourth of it there.
    fn glyph_hit(&self, glyph: &Glyph) -> bool {
        let boxed = glyph.bounding_box();
        let (cx, cy) = (
            (boxed.left + boxed.right) / 2.0,
            (boxed.bottom + boxed.top) / 2.0,
        );
        let area_of = boxed.width() * boxed.height();
        self.areas.iter().any(|area| {
            let r = &area.rect;
            if cx >= r.left && cx <= r.right && cy >= r.bottom && cy <= r.top {
                return true;
            }
            match boxed.intersection(r) {
                Some(common) if area_of > 0.0 => common.width() * common.height() >= 0.25 * area_of,
                _ => false,
            }
        })
    }

    fn touches(&self, rect: &Rectangle) -> bool {
        self.areas.iter().any(|area| overlaps(rect, &area.rect))
    }

    fn covered(&self, rect: &Rectangle) -> bool {
        self.areas.iter().any(|area| inside(rect, &area.rect))
    }

    /// Writes `data` again, whose run gave `content` and `trace`.
    ///
    /// `resources` are those the stream uses, which the forms it paints
    /// without resources of their own use too.
    pub fn rewrite(
        &mut self,
        data: &[u8],
        resources: &Resources,
        content: &PageContent,
        trace: &[OperationTrace],
    ) -> Result<Rewritten> {
        let removed: Vec<bool> = content.glyphs.iter().map(|g| self.glyph_hit(g)).collect();
        // The marked-content sequences with a glyph that goes, which can't keep
        // an /ActualText that spells it.
        let mut tainted: HashSet<usize> = HashSet::new();
        for (glyph, &gone) in content.glyphs.iter().zip(&removed) {
            if !gone {
                continue;
            }
            let mut next = glyph.marked_content;
            while let Some(index) = next {
                if !tainted.insert(index) {
                    break;
                }
                next = content.marked_content.get(index).and_then(|m| m.parent);
            }
        }

        let mut out = Vec::with_capacity(data.len());
        let mut result = Rewritten {
            data: Vec::new(),
            changed: false,
            forms: Vec::new(),
            images: Vec::new(),
        };
        let mut parser = ContentParser::new(data, self.limits);
        let mut index = 0;
        while let Some(operation) = parser.next_operation() {
            let Some(entry) = trace.get(index) else {
                // The run stopped before this operation: it wasn't checked.
                return Err(incomplete());
            };
            index += 1;
            let seen = Seen {
                resources,
                content,
                removed: &removed,
                tainted: &tainted,
            };
            let replaced = self.operation(&operation, entry, &seen, &mut result)?;
            match replaced {
                Some(bytes) => {
                    result.changed = true;
                    out.extend_from_slice(&bytes);
                }
                None => write_operation(&mut out, &operation),
            }
        }
        if index != trace.len() {
            return Err(incomplete());
        }
        result.data = out;
        Ok(result)
    }

    // What to write instead of `operation`: `None` to keep it, empty to drop it.
    fn operation(
        &mut self,
        operation: &Operation<'_>,
        entry: &OperationTrace,
        seen: &Seen<'_>,
        result: &mut Rewritten,
    ) -> Result<Option<Vec<u8>>> {
        let Seen {
            resources,
            content,
            removed,
            tainted,
        } = *seen;
        use Operator::{
            BeginMarkedContentWithProperties, CloseAndStroke, CloseFillAndStroke,
            CloseFillEvenOddAndStroke, Fill, FillAndStroke, FillEvenOdd, FillEvenOddAndStroke,
            FillObsolete, InlineImage, NextLineShowText, NextLineShowTextSpaced, PaintXObject,
            ShowText, ShowTextAdjusted, Stroke,
        };
        match operation.operator {
            ShowText | ShowTextAdjusted | NextLineShowText | NextLineShowTextSpaced => {
                let range = entry.glyphs.clone();
                if range.is_empty() || !removed[range.clone()].iter().any(|&gone| gone) {
                    return Ok(None);
                }
                let gone = removed[range.clone()].iter().filter(|&&g| g).count();
                self.report.glyphs += gone;
                Ok(Some(
                    rewrite_show(
                        operation,
                        &content.glyphs[range.clone()],
                        &removed[range],
                        entry.spacing,
                    )
                    .unwrap_or_default(),
                ))
            }
            Stroke
            | CloseAndStroke
            | Fill
            | FillObsolete
            | FillEvenOdd
            | FillAndStroke
            | FillEvenOddAndStroke
            | CloseFillAndStroke
            | CloseFillEvenOddAndStroke => {
                if self.options.graphics == GraphicsMode::Keep {
                    return Ok(None);
                }
                let Some(painted) = entry.paths.clone().next().map(|i| &content.paths[i]) else {
                    return Ok(None);
                };
                let reach = if painted.stroke {
                    painted.paint.line.width * painted.matrix.a.hypot(painted.matrix.b) / 2.0
                } else {
                    0.0
                };
                let mut rect = path_box(&painted.path.transformed(&painted.matrix));
                rect.left -= reach;
                rect.bottom -= reach;
                rect.right += reach;
                rect.top += reach;
                let hit = match self.options.graphics {
                    GraphicsMode::Covered => self.covered(&rect),
                    _ => self.touches(&rect),
                };
                if hit {
                    self.report.paths += 1;
                    // The path is ended without painting it, so a clip that
                    // went with it still applies.
                    return Ok(Some(b"n\n".to_vec()));
                }
                Ok(None)
            }
            InlineImage => {
                let Some(placed) = entry.images.clone().next().map(|i| &content.images[i]) else {
                    return Ok(None);
                };
                self.image(placed, result)
            }
            PaintXObject => {
                if let Some(form) = &entry.form {
                    return self.form(operation, form, resources, result);
                }
                let Some(placed) = entry.images.clone().next().map(|i| &content.images[i]) else {
                    return Ok(None);
                };
                self.image(placed, result)
            }
            BeginMarkedContentWithProperties => {
                let Some(index) = entry.marked_content.clone().next() else {
                    return Ok(None);
                };
                if !tainted.contains(&index) {
                    return Ok(None);
                }
                let Some(properties) = &content.marked_content[index].properties else {
                    return Ok(None);
                };
                let leaks = ["ActualText", "Alt", "E", "TU"];
                if !leaks.iter().any(|key| properties.contains_key(key)) {
                    return Ok(None);
                }
                let mut cleaned = properties.clone();
                for key in leaks {
                    cleaned.remove(key);
                }
                let mut bytes = Vec::new();
                write_object(
                    &mut bytes,
                    &Object::Name(content.marked_content[index].tag.clone()),
                );
                bytes.push(b' ');
                write_dictionary(&mut bytes, &cleaned);
                bytes.extend_from_slice(b" BDC\n");
                Ok(Some(bytes))
            }
            _ => Ok(None),
        }
    }

    // An image painted by `Do` or as an inline image.
    fn image(&mut self, placed: &PlacedImage, result: &mut Rewritten) -> Result<Option<Vec<u8>>> {
        if self.options.images == ImageMode::Keep || !self.touches(&placed.bounding_box()) {
            return Ok(None);
        }
        if self.options.images == ImageMode::Pixels
            && let Some(new) = self.blank_pixels(placed)
        {
            let key = self.fresh_name("OzRdImg");
            let mut bytes = Vec::new();
            write_object(&mut bytes, &Object::Name(Name::from(key.as_str())));
            bytes.extend_from_slice(b" Do\n");
            result.images.push((key, new));
            self.report.images_blanked += 1;
            return Ok(Some(bytes));
        }
        self.report.images_removed += 1;
        Ok(Some(Vec::new()))
    }

    // `placed` as a new image with the pixels under the areas black, if its
    // pixels can be read.
    fn blank_pixels(&mut self, placed: &PlacedImage) -> Option<NewImage> {
        let image = read_image(self.resolver, &placed.source, self.limits).ok()?;
        if image.description.image_mask {
            return None;
        }
        let (width, height) = (
            image.description.width as usize,
            image.description.height as usize,
        );
        let (color, mut samples) = match &image.encoding {
            None => pixels(&image.description, image.colors.as_ref(), &image.data)?,
            Some(encoding) if encoding.filter == ImageFilter::Dct => {
                let jpeg = decode_jpeg(&image.data, None, 0, self.limits).ok()?;
                if (jpeg.width, jpeg.height) != (width, height) {
                    return None;
                }
                match jpeg.components {
                    1 => (PngColor::Gray, jpeg.data),
                    3 => (PngColor::Rgb, jpeg.data),
                    _ => return None,
                }
            }
            Some(_) => return None,
        };
        let step = if color == PngColor::Gray { 1 } else { 3 };
        if samples.len() < width * height * step {
            return None;
        }
        let inverse = placed.matrix.inverse()?;
        let mut alpha = image.opacity.clone();
        let mut changed = false;
        for area in self.areas {
            // The pixels that can be under the area: its corners in the
            // space of the image, which is the unit square.
            let r = &area.rect;
            let corners = [
                (r.left, r.bottom),
                (r.right, r.bottom),
                (r.right, r.top),
                (r.left, r.top),
            ]
            .map(|(x, y)| inverse.apply(x, y));
            let (u0, u1) = min_max(corners.iter().map(|c| c.0));
            let (v0, v1) = min_max(corners.iter().map(|c| c.1));
            let first_x = ((u0 * width as f64).floor() - 1.0).max(0.0) as usize;
            let last_x = (((u1 * width as f64).ceil() + 1.0).max(0.0) as usize).min(width);
            // Row 0 is at the top: v = 1.
            let first_y = (((1.0 - v1) * height as f64).floor() - 1.0).max(0.0) as usize;
            let last_y =
                ((((1.0 - v0) * height as f64).ceil() + 1.0).max(0.0) as usize).min(height);
            for y in first_y..last_y {
                for x in first_x..last_x {
                    let pixel = Rectangle::around(&[
                        placed
                            .matrix
                            .apply(x as f64 / width as f64, 1.0 - y as f64 / height as f64),
                        placed.matrix.apply(
                            (x + 1) as f64 / width as f64,
                            1.0 - y as f64 / height as f64,
                        ),
                        placed.matrix.apply(
                            (x + 1) as f64 / width as f64,
                            1.0 - (y + 1) as f64 / height as f64,
                        ),
                        placed.matrix.apply(
                            x as f64 / width as f64,
                            1.0 - (y + 1) as f64 / height as f64,
                        ),
                    ]);
                    if !overlaps(&pixel, r) {
                        continue;
                    }
                    let at = (y * width + x) * step;
                    let fill = area.fill.unwrap_or([0, 0, 0]);
                    if step == 1 {
                        // The luminance of the color.
                        samples[at] = ((u32::from(fill[0]) * 299
                            + u32::from(fill[1]) * 587
                            + u32::from(fill[2]) * 114)
                            / 1000) as u8;
                    } else {
                        samples[at..at + 3].copy_from_slice(&fill);
                    }
                    if let Some(alpha) = alpha.as_mut()
                        && let Some(a) = alpha.get_mut(y * width + x)
                    {
                        // Nothing of the picture shows through.
                        *a = 255;
                    }
                    changed = true;
                }
            }
        }
        if !changed {
            return None;
        }
        let name = |text: &str| Object::Name(Name::from(text));
        let base = |color_space: &str| {
            let mut d = Dictionary::new();
            d.insert(Name::from("Type"), name("XObject"));
            d.insert(Name::from("Subtype"), name("Image"));
            d.insert(Name::from("Width"), Object::Integer(width as i64));
            d.insert(Name::from("Height"), Object::Integer(height as i64));
            d.insert(Name::from("ColorSpace"), name(color_space));
            d.insert(Name::from("BitsPerComponent"), Object::Integer(8));
            d.insert(Name::from("Filter"), name("FlateDecode"));
            d
        };
        let dictionary = base(if step == 1 { "DeviceGray" } else { "DeviceRGB" });
        let mask = alpha.map(|alpha| (base("DeviceGray"), deflate_zlib(&alpha, 6)));
        samples.truncate(width * height * step);
        Some(NewImage {
            dictionary,
            data: deflate_zlib(&samples, 6),
            mask,
        })
    }

    // A form painted by `operation`: written again if anything under the areas
    // is in it.
    fn form(
        &mut self,
        operation: &Operation<'_>,
        form: &FormUse,
        resources: &Resources,
        result: &mut Rewritten,
    ) -> Result<Option<Vec<u8>>> {
        if self.forms.contains(&form.id) || self.forms.len() >= MAX_FORM_DEPTH {
            return Ok(None);
        }
        // Where the form paints: its box, moved to the page.
        let object = self.resolver.resolve(form.id).map_err(|_| incomplete())?;
        let bounds = object
            .as_stream()
            .and_then(|stream| stream.dictionary().get("BBox").cloned())
            .and_then(|entry| match entry {
                Object::Reference(id) => self.resolver.resolve(id).ok().map(|o| (*o).clone()),
                other => Some(other),
            })
            .and_then(|entry| {
                let numbers: Vec<f64> = entry
                    .as_array()?
                    .iter()
                    .filter_map(Object::as_number)
                    .collect();
                (numbers.len() == 4).then_some([numbers[0], numbers[1], numbers[2], numbers[3]])
            });
        if let Some([x0, y0, x1, y1]) = bounds {
            let matrix = form.matrix.then(&form.ctm);
            let rect = Rectangle::around(&[
                matrix.apply(x0, y0),
                matrix.apply(x1, y0),
                matrix.apply(x1, y1),
                matrix.apply(x0, y1),
            ]);
            if !self.touches(&rect) {
                return Ok(None);
            }
        }
        let decoded = self
            .resolver
            .stream_data(form.id)
            .map_err(|_| incomplete())?;
        let (content, trace) =
            self.interpreter
                .trace_form(self.resolver, form.id, &form.ctm, resources);
        // The resources the form's own content uses.
        let own = match object
            .as_stream()
            .and_then(|s| s.dictionary().get("Resources"))
        {
            Some(_) => Resources::of(self.resolver, &object),
            None => None,
        };
        let inner_resources = own.unwrap_or_else(|| resources.clone());
        self.forms.push(form.id);
        let inner = self.rewrite(&decoded.data, &inner_resources, &content, &trace);
        self.forms.pop();
        let inner = inner?;
        if !inner.changed {
            return Ok(None);
        }
        self.report.forms += 1;
        let key = self.fresh_name("OzRdForm");
        let mut bytes = Vec::new();
        write_object(&mut bytes, &Object::Name(Name::from(key.as_str())));
        bytes.extend_from_slice(b" Do\n");
        let _ = operation;
        result.forms.push((key, form.id, inner));
        Ok(Some(bytes))
    }
}

fn min_max(values: impl Iterator<Item = f64>) -> (f64, f64) {
    values.fold((f64::MAX, f64::MIN), |(low, high), v| {
        (low.min(v), high.max(v))
    })
}

// The box around the points of a path, control points included.
fn path_box(path: &crate::render::Path) -> Rectangle {
    use crate::render::Segment;
    let mut points = Vec::new();
    for segment in &path.segments {
        match *segment {
            Segment::Move(x, y) | Segment::Line(x, y) => points.push((x, y)),
            Segment::Curve(x1, y1, x2, y2, x3, y3) => {
                points.extend([(x1, y1), (x2, y2), (x3, y3)]);
            }
            Segment::Close => {}
        }
    }
    Rectangle::around(&points)
}

// Writes an operation as it was read.
fn write_operation(out: &mut Vec<u8>, operation: &Operation<'_>) {
    if let (Operator::InlineImage, Some(image)) = (operation.operator, &operation.inline_image) {
        out.extend_from_slice(b"BI\n");
        for (key, value) in image.dictionary.iter() {
            write_object(out, &Object::Name(key.clone()));
            out.push(b' ');
            write_object(out, value);
            out.push(b'\n');
        }
        out.extend_from_slice(b"ID\n");
        out.extend_from_slice(image.data);
        out.extend_from_slice(b"\nEI\n");
        return;
    }
    for operand in operation.operands {
        write_object(out, operand);
        out.push(b' ');
    }
    out.extend_from_slice(operation.operator.keyword());
    out.push(b'\n');
}

// A string as the hexadecimal text of its bytes.
fn hex_string(bytes: &[u8]) -> Object {
    Object::String(PdfString::new(bytes.to_vec(), StringFormat::Hexadecimal))
}

// A text-showing operation without the glyphs that go: a `TJ` whose numbers
// move the text position by what each of them took, so the rest stays where
// it was. `None` if the glyphs don't match the string, when the whole
// operation goes.
fn rewrite_show(
    operation: &Operation<'_>,
    glyphs: &[Glyph],
    removed: &[bool],
    spacing: [f64; 3],
) -> Option<Vec<u8>> {
    let items: Vec<&Object> = match operation.operator {
        Operator::ShowTextAdjusted => operation.operands.last()?.as_array()?.iter().collect(),
        _ => vec![operation.operands.last()?],
    };
    let [character_spacing, word_spacing, _] = spacing;
    let mut array: Vec<Object> = Vec::new();
    let mut run: Vec<u8> = Vec::new();
    // How far the text position must still be moved, in the thousandths of
    // text size a `TJ` number counts in.
    let mut pending = 0.0_f64;
    let mut at_glyph = 0;
    let flush_run = |array: &mut Vec<Object>, run: &mut Vec<u8>| {
        if !run.is_empty() {
            array.push(hex_string(run));
            run.clear();
        }
    };
    let flush_move = |array: &mut Vec<Object>, pending: &mut f64| {
        if *pending != 0.0 {
            array.push(Object::Real(*pending));
            *pending = 0.0;
        }
    };
    for item in items {
        match item {
            Object::String(string) => {
                let bytes = string.as_bytes();
                let mut at = 0;
                while at < bytes.len() {
                    let glyph = glyphs.get(at_glyph)?;
                    let length = usize::from(glyph.code.length);
                    let code = bytes.get(at..at + length)?;
                    if removed[at_glyph] {
                        flush_run(&mut array, &mut run);
                        let word = if glyph.code.length == 1 && glyph.code.value == 32 {
                            word_spacing
                        } else {
                            0.0
                        };
                        let w1 = match &glyph.vertical {
                            Some(v) => v.width,
                            None => glyph.width,
                        };
                        if glyph.font_size != 0.0 {
                            pending -= 1000.0 * (w1 + (character_spacing + word) / glyph.font_size);
                        }
                    } else {
                        flush_move(&mut array, &mut pending);
                        run.extend_from_slice(code);
                    }
                    at += length;
                    at_glyph += 1;
                }
            }
            number => {
                flush_run(&mut array, &mut run);
                flush_move(&mut array, &mut pending);
                array.push(number.clone());
            }
        }
    }
    if at_glyph != glyphs.len() {
        return None;
    }
    flush_run(&mut array, &mut run);
    flush_move(&mut array, &mut pending);

    let mut out = Vec::new();
    match operation.operator {
        Operator::NextLineShowText => out.extend_from_slice(b"T*\n"),
        Operator::NextLineShowTextSpaced => {
            // `aw ac string "`: the spacings, then the next line.
            let spacings = &operation.operands[operation.operands.len().checked_sub(3)?..];
            write_object(&mut out, &spacings[0]);
            out.extend_from_slice(b" Tw ");
            write_object(&mut out, &spacings[1]);
            out.extend_from_slice(b" Tc T*\n");
        }
        _ => {}
    }
    write_object(&mut out, &Object::Array(array));
    out.extend_from_slice(b" TJ\n");
    Some(out)
}
