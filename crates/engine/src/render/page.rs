use std::collections::HashMap;
use std::sync::Arc;

use super::blend::BlendMode;
use super::canvas::{Clip, Pixmap};
use super::glyphs::GlyphCache;
use super::image::{draw_bitmap, jpeg_scale, prepare};
use super::path::{Path, flatten};
use super::raster::{FillRule, IRect, Mask, fill_contours, rect_mask};
use super::shading::{Shading, paint_shading, read_shading};
use super::stroke::{StrokeStyle, stroke_contours};
use crate::base::{Error, ErrorKind, Limit, Limits, Result};
use crate::content::{
    ClipNode, Interpreter, Matrix, PageContent, PaintItem, PaintState, PatternRef, SoftMask,
    TextRenderingMode,
};
use crate::document::{Page, read_annotations};
use crate::images::{ImageData, read_image};
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::Resolver;

/// How a page is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    /// Pixels for each point of the page: 1 is 72 dots per inch.
    pub scale: f64,
    /// The color of the paper, as red, green and blue.
    pub background: [u8; 3],
    /// Whether annotations, and the widgets of form fields, are drawn with
    /// their appearance.
    pub annotations: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            background: [255, 255, 255],
            annotations: true,
        }
    }
}

/// The matrix from the default user space of `page` to pixels at `scale`: the
/// crop box starts at the top left corner, and the page is turned by its
/// `/Rotate`. Also the size of the pixmap, in pixels.
pub fn page_matrix(page: &Page, scale: f64) -> (Matrix, usize, usize) {
    let crop = page.crop_box();
    let (width, height) = (crop.width() * scale, crop.height() * scale);
    let s = scale;
    let (matrix, w, h) = match page.rotation() % 360 {
        90 => (
            Matrix::new(0.0, s, s, 0.0, -crop.bottom * s, -crop.left * s),
            height,
            width,
        ),
        180 => (
            Matrix::new(-s, 0.0, 0.0, s, crop.right * s, -crop.bottom * s),
            width,
            height,
        ),
        270 => (
            Matrix::new(0.0, -s, -s, 0.0, crop.top * s, crop.right * s),
            height,
            width,
        ),
        _ => (
            Matrix::new(s, 0.0, 0.0, -s, -crop.left * s, crop.top * s),
            width,
            height,
        ),
    };
    (
        matrix,
        w.ceil().max(1.0) as usize,
        h.ceil().max(1.0) as usize,
    )
}

/// Draws `page`. It is an error if the page would take more than
/// `limits.max_decoded_length` bytes as pixels. What can't be drawn, such as
/// JPEG 2000 images, is left out.
pub fn render_page(
    resolver: &mut Resolver<'_>,
    page: &Page,
    options: &RenderOptions,
    limits: Limits,
) -> Result<Pixmap> {
    if !options.scale.is_finite() || options.scale <= 0.0 {
        return Err(Error::new(ErrorKind::InvalidScale, 0));
    }
    let (page_to_device, width, height) = page_matrix(page, options.scale);
    let mut pixmap = Pixmap::new(width, height, limits.max_decoded_length)
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded(Limit::DecodedLength), 0))?;
    pixmap.clear(options.background);
    if page.is_damaged() {
        return Ok(pixmap);
    }
    let mut interpreter = Interpreter::painting(limits);
    let content = interpreter.run_page(resolver, page);
    let mut painter = Painter {
        page_to_device,
        base: Clip::new(pixmap.rect()),
        clips: HashMap::new(),
        images: HashMap::new(),
        glyphs: GlyphCache::new(),
        masks: HashMap::new(),
        masks_of_glyphs: HashMap::new(),
        shadings: HashMap::new(),
        cells: HashMap::new(),
        size: (width, height),
        depth: 0,
        limits,
    };
    painter.paint(&mut pixmap, resolver, &content);
    if options.annotations {
        paint_annotations(&mut painter, &mut pixmap, resolver, &mut interpreter, page);
    }
    Ok(pixmap)
}

// The most annotations drawn on a page.
const MAX_DRAWN_ANNOTATIONS: usize = 5_000;

// Draws the appearance of each annotation that is shown, as a form fitted to
// its rectangle (ISO 32000-2, section 12.5.5, algorithm 8.1).
fn paint_annotations(
    painter: &mut Painter,
    pixmap: &mut Pixmap,
    resolver: &mut Resolver<'_>,
    interpreter: &mut Interpreter,
    page: &Page,
) {
    let annotations = read_annotations(resolver, page, &[], &Dictionary::new());
    for annotation in annotations.iter().take(MAX_DRAWN_ANNOTATIONS) {
        if annotation.is_hidden() || annotation.subtype == "Popup" {
            continue;
        }
        let Some(form) = annotation.appearance else {
            continue;
        };
        let Some(matrix) = appearance_matrix(resolver, form, &annotation.rect) else {
            continue;
        };
        let content = interpreter.run_form(resolver, form, &matrix);
        // The clips and masks are kept by the address of what makes them, which
        // the next annotation's content may reuse.
        painter.clips.clear();
        painter.masks.clear();
        painter.paint(pixmap, resolver, &content);
        painter.clips.clear();
        painter.masks.clear();
    }
}

// The matrix that fits the form's box, after its own matrix, into `rect`.
fn appearance_matrix(
    resolver: &mut Resolver<'_>,
    form: ObjectId,
    rect: &crate::document::Rectangle,
) -> Option<Matrix> {
    let object = resolver.resolve(form).ok()?;
    let dictionary = object.as_stream()?.dictionary();
    let numbers = |resolver: &mut Resolver<'_>, key: &str| -> Option<Vec<f64>> {
        let entry = match dictionary.get(key)? {
            Object::Reference(id) => resolver.resolve(*id).ok()?,
            other => Arc::new(other.clone()),
        };
        entry.as_array()?.iter().map(Object::as_number).collect()
    };
    let bbox = numbers(resolver, "BBox")?;
    let &[x1, y1, x2, y2] = bbox.as_slice() else {
        return None;
    };
    let own = numbers(resolver, "Matrix")
        .and_then(|numbers| match numbers.as_slice() {
            &[a, b, c, d, e, f] => Some(Matrix::new(a, b, c, d, e, f)),
            _ => None,
        })
        .unwrap_or(Matrix::IDENTITY);
    let corners = [(x1, y1), (x2, y1), (x2, y2), (x1, y2)].map(|(x, y)| own.apply(x, y));
    let bounds = crate::document::Rectangle::around(&corners);
    let scale = |wanted: f64, found: f64| if found > 1e-9 { wanted / found } else { 1.0 };
    let (sx, sy) = (
        scale(rect.width(), bounds.width()),
        scale(rect.height(), bounds.height()),
    );
    let matrix = Matrix::new(
        sx,
        0.0,
        0.0,
        sy,
        rect.left - bounds.left * sx,
        rect.bottom - bounds.bottom * sy,
    );
    matrix.is_finite().then_some(matrix)
}

struct Painter {
    page_to_device: Matrix,
    base: Clip,
    // The clips already made, by the address of the node that makes them.
    clips: HashMap<usize, Clip>,
    // The images already prepared, once for each object.
    images: HashMap<(ObjectId, usize), Option<Arc<super::image::Bitmap>>>,
    glyphs: GlyphCache,
    // The soft masks already drawn, by the address of the mask.
    masks: HashMap<usize, Option<Mask>>,
    // The masks of glyphs already drawn, by font, code, size and the eighth of
    // a pixel they are at.
    masks_of_glyphs: HashMap<GlyphKey, Option<Arc<Mask>>>,
    // The shadings and the cells of tiling patterns already read, by their
    // object, and for uncolored patterns the color.
    shadings: HashMap<ObjectId, Option<Arc<Shading>>>,
    cells: HashMap<(ObjectId, [u8; 3]), Arc<PageContent>>,
    // The size of the pixmap, which masks and groups share.
    size: (usize, usize),
    // How many soft masks and patterns are being drawn inside each other.
    depth: usize,
    limits: Limits,
}

type GlyphKey = (usize, u32, [i64; 4], i32, i32);

// How deep soft masks and patterns can be drawn inside each other.
const MAX_NESTING: usize = 3;

// The most cells of a tiling pattern painted for one area.
const MAX_TILES: i64 = 20_000;

// How a tiling pattern repeats: its cell, the steps, and the matrix from its
// space to the device.
struct Tiles {
    bbox: [f64; 4],
    steps: (f64, f64),
    to_device: Matrix,
    uncolored: bool,
}

// What a shape is painted with.
#[derive(Clone, Copy)]
struct Ink {
    color: [u8; 3],
    alpha: u8,
    mode: BlendMode,
}

impl Painter {
    fn paint(&mut self, pixmap: &mut Pixmap, resolver: &mut Resolver<'_>, content: &PageContent) {
        self.paint_range(pixmap, resolver, content, 0, content.order.len());
    }

    // Paints the items from `start` to `end` of the order.
    fn paint_range(
        &mut self,
        pixmap: &mut Pixmap,
        resolver: &mut Resolver<'_>,
        content: &PageContent,
        start: usize,
        end: usize,
    ) {
        let mut next = start;
        while next < end {
            let item = content.order[next];
            next += 1;
            match item {
                PaintItem::EndGroup => {}
                PaintItem::Shading(index) => {
                    let painted = &content.shadings[index];
                    let clip = self.clip(resolver, &painted.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let Some(shading) = self.shading_of(resolver, &painted.shading) else {
                        continue;
                    };
                    let matrix = painted.matrix.then(&self.page_to_device);
                    let Some(inverse) = inverted(&matrix) else {
                        continue;
                    };
                    paint_shading(
                        pixmap,
                        &clip,
                        None,
                        &shading,
                        &inverse,
                        to_byte(painted.paint.fill_alpha),
                        painted.paint.blend,
                        false,
                    );
                }
                PaintItem::BeginGroup(index) => {
                    // The group ends at the `EndGroup` that matches.
                    let (mut depth, mut close) = (1, next);
                    while close < end {
                        match content.order[close] {
                            PaintItem::BeginGroup(_) => depth += 1,
                            PaintItem::EndGroup => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        close += 1;
                    }
                    self.paint_group(pixmap, resolver, content, index, next, close);
                    next = close + 1;
                }
                PaintItem::Path(index) => {
                    let painted = &content.paths[index];
                    let clip = self.clip(resolver, &painted.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let matrix = painted.matrix.then(&self.page_to_device);
                    if let Some(rule) = painted.fill {
                        let ink = Ink {
                            color: painted.paint.fill_color,
                            alpha: to_byte(painted.paint.fill_alpha),
                            mode: painted.paint.blend,
                        };
                        if let Some(mask) = fill_mask(&painted.path, &matrix, rule, &clip, pixmap) {
                            let pattern = painted.paint.fill_pattern.clone();
                            self.paint_mask(pixmap, resolver, &mask, &clip, ink, pattern.as_ref());
                        }
                    }
                    if painted.stroke {
                        let ink = Ink {
                            color: painted.paint.stroke_color,
                            alpha: to_byte(painted.paint.stroke_alpha),
                            mode: painted.paint.blend,
                        };
                        let line = &painted.paint.line;
                        if let Some(mask) = stroke_mask(&painted.path, &matrix, line, &clip, pixmap)
                        {
                            let pattern = painted.paint.stroke_pattern.clone();
                            self.paint_mask(pixmap, resolver, &mask, &clip, ink, pattern.as_ref());
                        }
                    }
                }
                PaintItem::Image(index) => {
                    let placed = &content.images[index];
                    let clip = self.clip(resolver, &placed.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let id = match &placed.source {
                        crate::content::ImageSource::XObject { id, .. } => Some(*id),
                        crate::content::ImageSource::Inline { .. } => None,
                    };
                    // How big the image is drawn, to decode a JPEG that is drawn
                    // small at a fraction of its size.
                    let to_device = placed.matrix.then(&self.page_to_device);
                    let target = (
                        to_device.a.hypot(to_device.b),
                        to_device.c.hypot(to_device.d),
                    );
                    let bitmap = {
                        let cached = id.and_then(|id| {
                            // Any size that was made and is big enough serves.
                            [1usize, 2, 4, 8]
                                .into_iter()
                                .find_map(|n| self.images.get(&(id, n)).cloned().map(|b| (n, b)))
                        });
                        let wanted = |n: usize, bitmap: &Option<Arc<super::image::Bitmap>>| {
                            bitmap.as_ref().is_none_or(|b| {
                                n == 8
                                    || (b.width as f64 >= target.0 && b.height as f64 >= target.1)
                            })
                        };
                        match cached {
                            Some((n, bitmap)) if wanted(n, &bitmap) => bitmap,
                            _ => {
                                let image: Option<ImageData> =
                                    read_image(resolver, &placed.source, self.limits).ok();
                                let scale = image.as_ref().map_or(8, |i| {
                                    jpeg_scale(
                                        i.description.width as usize,
                                        i.description.height as usize,
                                        target,
                                    )
                                });
                                let bitmap = image
                                    .as_ref()
                                    .and_then(|image| prepare(image, self.limits, scale))
                                    .map(Arc::new);
                                if let Some(id) = id {
                                    // Only a decode that wasn't reduced is kept as the
                                    // whole image; the others are kept by their scale.
                                    let kept = if image.as_ref().is_some_and(|i| {
                                        i.encoding.is_some() && i.opacity.is_none()
                                    }) {
                                        scale
                                    } else {
                                        8
                                    };
                                    self.images.insert((id, kept), bitmap.clone());
                                }
                                bitmap
                            }
                        }
                    };
                    let Some(bitmap) = bitmap else { continue };
                    let matrix = placed.matrix.then(&self.page_to_device);
                    let stencil = bitmap.stencil.then_some(placed.paint.fill_color);
                    draw_bitmap(
                        pixmap,
                        &clip,
                        &bitmap,
                        &matrix,
                        to_byte(placed.paint.fill_alpha),
                        stencil,
                        placed.paint.blend,
                    );
                }
                PaintItem::Glyph(index) => {
                    let glyph = &content.glyphs[index];
                    let mode = glyph.rendering_mode;
                    let fill = matches!(
                        mode,
                        TextRenderingMode::Fill
                            | TextRenderingMode::FillStroke
                            | TextRenderingMode::FillClip
                            | TextRenderingMode::FillStrokeClip
                    );
                    let stroke = matches!(
                        mode,
                        TextRenderingMode::Stroke
                            | TextRenderingMode::FillStroke
                            | TextRenderingMode::StrokeClip
                            | TextRenderingMode::FillStrokeClip
                    );
                    if !fill && !stroke {
                        continue;
                    }
                    let clip = self.clip(resolver, &glyph.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let Some(shape) = self.glyphs.shape(&glyph.font, glyph.code) else {
                        continue;
                    };
                    let matrix = shape.matrix.then(&glyph.matrix).then(&self.page_to_device);
                    if fill {
                        let ink = Ink {
                            color: glyph.paint.fill_color,
                            alpha: to_byte(glyph.paint.fill_alpha),
                            mode: glyph.paint.blend,
                        };
                        let rule = FillRule::NonZero;
                        if glyph.paint.fill_pattern.is_none() {
                            // The same glyph at the same size is drawn from one
                            // mask, moved to the whole pixel it is at.
                            let key = (
                                Arc::as_ptr(&glyph.font) as usize,
                                glyph.code.value,
                                [matrix.a, matrix.b, matrix.c, matrix.d]
                                    .map(|v| (v * 1024.0).round() as i64),
                                ((matrix.e - matrix.e.floor()) * 8.0).round() as i32,
                                ((matrix.f - matrix.f.floor()) * 8.0).round() as i32,
                            );
                            let cached = match self.masks_of_glyphs.get(&key) {
                                Some(mask) => mask.clone(),
                                None => {
                                    let at = Matrix::new(
                                        matrix.a,
                                        matrix.b,
                                        matrix.c,
                                        matrix.d,
                                        f64::from(key.3) / 8.0,
                                        f64::from(key.4) / 8.0,
                                    );
                                    let limit = IRect::new(-1024, -1024, 2048, 2048);
                                    let mask = flatten(&shape.path, &at, 0.1)
                                        .and_then(|contours| fill_contours(&contours, rule, &limit))
                                        .map(Arc::new);
                                    self.masks_of_glyphs.insert(key, mask.clone());
                                    mask
                                }
                            };
                            if let Some(mask) = cached {
                                let offset = (matrix.e.floor() as i32, matrix.f.floor() as i32);
                                pixmap.fill_moved(
                                    &mask, offset, &clip, ink.color, ink.alpha, ink.mode,
                                );
                            }
                        } else if let Some(mask) =
                            fill_mask(&shape.path, &matrix, rule, &clip, pixmap)
                        {
                            let pattern = glyph.paint.fill_pattern.clone();
                            self.paint_mask(pixmap, resolver, &mask, &clip, ink, pattern.as_ref());
                        }
                    }
                    if stroke {
                        // The line width is in the space of the page, as the
                        // glyph's own matrix has the font size in it.
                        let in_page = shape.path.transformed(&shape.matrix.then(&glyph.matrix));
                        let ink = Ink {
                            color: glyph.paint.stroke_color,
                            alpha: to_byte(glyph.paint.stroke_alpha),
                            mode: glyph.paint.blend,
                        };
                        let line = &glyph.paint.line;
                        let to_device = self.page_to_device;
                        if let Some(mask) = stroke_mask(&in_page, &to_device, line, &clip, pixmap) {
                            let pattern = glyph.paint.stroke_pattern.clone();
                            self.paint_mask(pixmap, resolver, &mask, &clip, ink, pattern.as_ref());
                        }
                    }
                }
            }
        }
    }

    // The clip of a painting state in device pixels, made once for each node.
    // Paints what `mask` covers with the ink, or with the pattern if there is
    // one.
    fn paint_mask(
        &mut self,
        pixmap: &mut Pixmap,
        resolver: &mut Resolver<'_>,
        mask: &Mask,
        clip: &Clip,
        ink: Ink,
        pattern: Option<&Arc<PatternRef>>,
    ) {
        let Some(pattern) = pattern else {
            pixmap.fill_with(mask, clip, ink.color, ink.alpha, ink.mode);
            return;
        };
        let object = match &pattern.object {
            Object::Reference(id) => match resolver.resolve(*id) {
                Ok(object) => object,
                Err(_) => return,
            },
            other => Arc::new(other.clone()),
        };
        let dictionary = match &*object {
            Object::Stream(stream) => stream.dictionary().clone(),
            Object::Dictionary(dictionary) => dictionary.clone(),
            _ => return,
        };
        let numbers = |key: &str, resolver: &mut Resolver<'_>| -> Option<Vec<f64>> {
            let entry = dictionary.get(key)?;
            let resolved = match entry {
                Object::Reference(id) => resolver.resolve(*id).ok()?.as_ref().clone(),
                other => other.clone(),
            };
            Some(
                resolved
                    .as_array()?
                    .iter()
                    .filter_map(Object::as_number)
                    .collect(),
            )
        };
        let own = numbers("Matrix", resolver)
            .filter(|m| m.len() == 6)
            .map_or(Matrix::IDENTITY, |m| {
                Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5])
            });
        let to_device = own.then(&pattern.base).then(&self.page_to_device);
        match dictionary.get("PatternType").and_then(Object::as_integer) {
            Some(2) => {
                let Some(entry) = dictionary.get("Shading") else {
                    return;
                };
                let Some(shading) = self.shading_of(resolver, entry) else {
                    return;
                };
                let Some(inverse) = inverted(&to_device) else {
                    return;
                };
                paint_shading(
                    pixmap,
                    clip,
                    Some(mask),
                    &shading,
                    &inverse,
                    ink.alpha,
                    ink.mode,
                    true,
                );
            }
            Some(1) if object.as_stream().is_some() && pattern.object.as_reference().is_some() => {
                let bbox = numbers("BBox", resolver).filter(|b| b.len() == 4);
                let uncolored = dictionary.get("PaintType").and_then(Object::as_integer) == Some(2);
                let step = |key: &str| dictionary.get(key).and_then(Object::as_number);
                let (Some(bbox), Some(xs), Some(ys)) = (bbox, step("XStep"), step("YStep")) else {
                    return;
                };
                self.paint_tiles(
                    pixmap,
                    resolver,
                    mask,
                    clip,
                    pattern,
                    &Tiles {
                        bbox: [
                            bbox[0].min(bbox[2]),
                            bbox[1].min(bbox[3]),
                            bbox[0].max(bbox[2]),
                            bbox[1].max(bbox[3]),
                        ],
                        steps: (xs.abs(), ys.abs()),
                        to_device,
                        uncolored,
                    },
                    ink,
                );
            }
            _ => {}
        }
    }

    // The shading of an entry: a reference to its object, or its dictionary.
    fn shading_of(&mut self, resolver: &mut Resolver<'_>, entry: &Object) -> Option<Arc<Shading>> {
        if let Object::Reference(id) = entry
            && let Some(cached) = self.shadings.get(id)
        {
            return cached.clone();
        }
        let shading = read_shading(resolver, entry, self.limits).map(Arc::new);
        if let Object::Reference(id) = entry {
            self.shadings.insert(*id, shading.clone());
        }
        shading
    }

    // Paints the cells of a tiling pattern over what `mask` covers: the cell
    // is run once and painted at every step that reaches the area.
    #[allow(clippy::too_many_arguments)]
    fn paint_tiles(
        &mut self,
        pixmap: &mut Pixmap,
        resolver: &mut Resolver<'_>,
        mask: &Mask,
        clip: &Clip,
        pattern: &PatternRef,
        tiles: &Tiles,
        ink: Ink,
    ) {
        let (xs, ys) = tiles.steps;
        let Some(inverse) = inverted(&tiles.to_device) else {
            return;
        };
        // A pattern whose cell uses a pattern, which may be itself.
        if self.depth >= MAX_NESTING {
            return;
        }
        if xs <= 0.0 || ys <= 0.0 {
            return;
        }
        // The area in pattern space, and the steps that reach it.
        let area = mask.rect.intersect(&clip.rect);
        if area.is_empty() {
            return;
        }
        let corners = [
            (area.x0, area.y0),
            (area.x1, area.y0),
            (area.x1, area.y1),
            (area.x0, area.y1),
        ]
        .map(|(x, y)| inverse.apply(f64::from(x), f64::from(y)));
        let low = corners
            .iter()
            .fold((f64::MAX, f64::MAX), |a, c| (a.0.min(c.0), a.1.min(c.1)));
        let high = corners
            .iter()
            .fold((f64::MIN, f64::MIN), |a, c| (a.0.max(c.0), a.1.max(c.1)));
        let b = tiles.bbox;
        let (i0, i1) = (
            ((low.0 - b[2]) / xs).floor() as i64,
            ((high.0 - b[0]) / xs).ceil() as i64,
        );
        let (j0, j1) = (
            ((low.1 - b[3]) / ys).floor() as i64,
            ((high.1 - b[1]) / ys).ceil() as i64,
        );
        let count = (i1 - i0 + 1).max(0).saturating_mul((j1 - j0 + 1).max(0));
        if count == 0 || count > MAX_TILES {
            return;
        }
        let Some(id) = pattern.object.as_reference() else {
            return;
        };
        let color = if tiles.uncolored {
            pattern.color
        } else {
            [0; 3]
        };
        let content = match self.cells.get(&(id, color)) {
            Some(content) => content.clone(),
            None => {
                let mut interpreter = Interpreter::painting(self.limits);
                let content = Arc::new(interpreter.run_pattern(
                    resolver,
                    id,
                    tiles.uncolored.then_some(pattern.color),
                ));
                self.cells.insert((id, color), content.clone());
                content
            }
        };
        let Some(mut own) = Pixmap::new(
            pixmap.width(),
            pixmap.height(),
            self.limits.max_decoded_length,
        ) else {
            return;
        };
        let (saved_matrix, saved_base) = (self.page_to_device, self.base.clone());
        self.depth += 1;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let tile = Matrix::translation(i as f64 * xs, j as f64 * ys).then(&tiles.to_device);
                // The cell is cut to its box.
                let mut cell = Path::new();
                cell.rectangle(b[0], b[1], b[2] - b[0], b[3] - b[1]);
                let Some(contours) = flatten(&cell, &tile, 0.1) else {
                    continue;
                };
                let Some(cut) = fill_contours(&contours, FillRule::NonZero, &area) else {
                    continue;
                };
                self.page_to_device = tile;
                self.base = Clip::new(own.rect()).intersect(Some(&cut));
                let (clips, masks) = (
                    std::mem::take(&mut self.clips),
                    std::mem::take(&mut self.masks),
                );
                self.paint_range(&mut own, resolver, &content, 0, content.order.len());
                self.clips = clips;
                self.masks = masks;
            }
        }
        self.depth -= 1;
        self.page_to_device = saved_matrix;
        self.base = saved_base;
        let area_clip = clip.intersect(Some(mask));
        pixmap.composite(&own, &area_clip, ink.alpha, ink.mode);
    }

    // The clip of a painting state in device pixels, with its soft mask.
    fn clip(&mut self, resolver: &mut Resolver<'_>, paint: &PaintState) -> Clip {
        let clip = match &paint.clip {
            None => self.base.clone(),
            Some(node) => self.node_clip(node),
        };
        match &paint.soft_mask {
            Some(soft) if !clip.rect.is_empty() => {
                let mask = self.soft_mask(resolver, soft);
                // A mask that can't be drawn doesn't hide anything.
                match mask {
                    Some(mask) => clip.intersect(Some(&mask)),
                    None => clip,
                }
            }
            _ => clip,
        }
    }

    // The opacity a soft mask gives each pixel, drawn once for each mask.
    fn soft_mask(&mut self, resolver: &mut Resolver<'_>, soft: &Arc<SoftMask>) -> Option<Mask> {
        let key = Arc::as_ptr(soft) as usize;
        if let Some(mask) = self.masks.get(&key) {
            return mask.clone();
        }
        // Guards against a mask that uses itself.
        self.masks.insert(key, None);
        if self.depth >= MAX_NESTING {
            return None;
        }
        let (width, height) = self.size;
        let mut pixmap = Pixmap::new(width, height, self.limits.max_decoded_length)?;
        if soft.luminosity {
            pixmap.clear(soft.backdrop);
        }
        let mut interpreter = Interpreter::painting(self.limits);
        // The form is painted with the matrix it had when the mask was set.
        let content = interpreter.run_form(resolver, soft.form, &soft.matrix);
        let mut inner = Painter {
            page_to_device: self.page_to_device,
            base: self.base.clone(),
            clips: HashMap::new(),
            images: HashMap::new(),
            glyphs: GlyphCache::new(),
            masks: HashMap::new(),
            masks_of_glyphs: HashMap::new(),
            shadings: HashMap::new(),
            cells: HashMap::new(),
            size: self.size,
            depth: self.depth + 1,
            limits: self.limits,
        };
        inner.paint(&mut pixmap, resolver, &content);
        let rect = pixmap.rect();
        let data: Vec<u8> = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                if soft.luminosity {
                    // Opaque over the backdrop: the luminosity of the color.
                    let l = 0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2]);
                    (l + 0.5).min(255.0) as u8
                } else {
                    p[3]
                }
            })
            .collect();
        let mask = Mask { rect, data };
        self.masks.insert(key, Some(mask.clone()));
        Some(mask)
    }

    // Paints the group of the items `start` to `end` on its own pixmap and puts
    // it over `pixmap` with the opacity, blend mode and clip it was painted
    // with.
    fn paint_group(
        &mut self,
        pixmap: &mut Pixmap,
        resolver: &mut Resolver<'_>,
        content: &PageContent,
        index: usize,
        start: usize,
        end: usize,
    ) {
        let group = &content.groups[index];
        let clip = self.clip(resolver, &group.paint);
        if clip.rect.is_empty() {
            return;
        }
        let Some(mut own) = Pixmap::new(
            pixmap.width(),
            pixmap.height(),
            self.limits.max_decoded_length,
        ) else {
            return;
        };
        self.paint_range(&mut own, resolver, content, start, end);
        pixmap.composite(
            &own,
            &clip,
            to_byte(group.paint.fill_alpha),
            group.paint.blend,
        );
    }

    fn node_clip(&mut self, node: &Arc<ClipNode>) -> Clip {
        let key = Arc::as_ptr(node) as usize;
        if let Some(clip) = self.clips.get(&key) {
            return clip.clone();
        }
        let parent = match &node.parent {
            None => self.base.clone(),
            Some(parent) => self.node_clip(parent),
        };
        let matrix = node.matrix.then(&self.page_to_device);
        // A clip made by text is the union of its glyphs.
        if !node.glyphs.is_empty() {
            let mut paths = Path::new();
            for glyph in &node.glyphs {
                if let Some(shape) = self.glyphs.shape(&glyph.font, glyph.code) {
                    let to_page = shape.matrix.then(&glyph.matrix);
                    paths
                        .segments
                        .extend(shape.path.transformed(&to_page).segments);
                }
            }
            let mask = flatten(&paths, &self.page_to_device, 0.1)
                .and_then(|contours| fill_contours(&contours, FillRule::NonZero, &parent.rect));
            let clip = match mask {
                Some(mask) => parent.intersect_owned(mask),
                None => parent.intersect(None),
            };
            self.clips.insert(key, clip.clone());
            return clip;
        }
        // A rectangle with sides along the axes, the usual clip, is
        // computed without sweeping its edges.
        let mask = match node.path.axis_aligned_rectangle(&matrix) {
            Some((x0, y0, x1, y1)) => rect_mask(x0, y0, x1, y1, &parent.rect),
            None => flatten(&node.path, &matrix, 0.1)
                .and_then(|contours| fill_contours(&contours, node.rule, &parent.rect)),
        };
        let clip = match mask {
            Some(mask) => parent.intersect_owned(mask),
            None => parent.intersect(None),
        };
        self.clips.insert(key, clip.clone());
        clip
    }
}

fn to_byte(alpha: f64) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn inverted(m: &Matrix) -> Option<Matrix> {
    let det = m.a * m.d - m.b * m.c;
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    Some(Matrix::new(
        m.d / det,
        -m.b / det,
        -m.c / det,
        m.a / det,
        (m.c * m.f - m.d * m.e) / det,
        (m.b * m.e - m.a * m.f) / det,
    ))
}

// The coverage of the pixels a path fills inside `clip`.
fn fill_mask(
    path: &Path,
    matrix: &Matrix,
    rule: FillRule,
    clip: &Clip,
    pixmap: &Pixmap,
) -> Option<Mask> {
    let limit = clip.rect.intersect(&pixmap.rect());
    if let Some((x0, y0, x1, y1)) = path.axis_aligned_rectangle(matrix) {
        return rect_mask(x0, y0, x1, y1, &limit);
    }
    let contours = flatten(path, matrix, 0.1)?;
    fill_contours(&contours, rule, &limit)
}

// The coverage of the pixels a path strokes inside `clip`.
fn stroke_mask(
    path: &Path,
    matrix: &Matrix,
    style: &StrokeStyle,
    clip: &Clip,
    pixmap: &Pixmap,
) -> Option<Mask> {
    // A line thinner than a pixel is a pixel wide, as the thinnest line a
    // device draws (section 10.6.2).
    let scale = matrix.a.hypot(matrix.b).max(matrix.c.hypot(matrix.d));
    if scale <= 0.0 || !scale.is_finite() {
        return None;
    }
    let mut style = style.clone();
    if style.width * scale < 1.0 {
        style.width = 1.0 / scale;
    }
    let contours = flatten(path, &Matrix::IDENTITY, 0.1 / scale)?;
    let outline = stroke_contours(&contours, &style, matrix, 0.1)?;
    fill_contours(
        &outline,
        FillRule::NonZero,
        &clip.rect.intersect(&pixmap.rect()),
    )
}
