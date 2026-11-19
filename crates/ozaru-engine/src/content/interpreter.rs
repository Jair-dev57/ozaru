use std::collections::HashMap;
use std::sync::Arc;

use super::graphics_state::{ClipNode, GraphicsStack, GraphicsState, Matrix, PaintState, SoftMask};
use super::operators::{ContentParser, Operation, Operator};
use super::text_state::{TextObject, TextRenderingMode};
use crate::base::{DiagnosticKind, Diagnostics, ErrorKind, Limits};
use crate::document::{Page, Rectangle, Resources, decode_text_string};
use crate::fonts::{Code, Font, VerticalMetrics, read_render_program};
use crate::images::{ColorSpace, read_color_space};
use crate::render::{BlendMode, FillRule, LineCap, LineJoin, Path};
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString};
use crate::xref::Resolver;

/// A glyph painted by a text-showing operator, where and how it was painted.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// The character code, as the font read it from the string.
    pub code: Code,
    /// The text of the code, as the font maps it. `None` if the font doesn't
    /// say.
    pub text: Option<String>,
    pub font: Arc<Font>,
    /// The font size set with `Tf`.
    pub font_size: f64,
    /// The text rendering matrix: from the glyph's text space, scaled by the
    /// font size, to the default user space of the page (ISO 32000-2, section
    /// 9.4.4). Its origin, `(e, f)`, is where the glyph is placed: the
    /// origin of its glyph space, also in vertical writing.
    pub matrix: Matrix,
    /// The horizontal width of the code, in unscaled text space units,
    /// without character or word spacing. In horizontal writing, it is how
    /// far the glyph moves the text position.
    pub width: f64,
    /// For a font that writes vertically, how the glyph moves the text
    /// position down and where its vertical origin is.
    pub vertical: Option<VerticalMetrics>,
    pub rendering_mode: TextRenderingMode,
    /// The innermost marked-content sequence the glyph is in, an index into
    /// [`PageContent::marked_content`].
    pub marked_content: Option<usize>,
    /// The colors, line and clip it was painted with.
    pub paint: PaintState,
}

impl Glyph {
    /// Where the glyph is placed, in the default user space of the page.
    pub fn origin(&self) -> (f64, f64) {
        (self.matrix.e, self.matrix.f)
    }

    /// Where the glyph ends, in the default user space of the page: its
    /// origin moved by its width, or in vertical writing its vertical origin
    /// moved by its vertical width.
    pub fn end(&self) -> (f64, f64) {
        match &self.vertical {
            Some(v) => self.matrix.apply(v.origin_x, v.origin_y + v.width),
            None => self.matrix.apply(self.width, 0.0),
        }
    }

    /// The corners of the glyph's box, in the default user space of the page:
    /// as wide as the glyph, and as tall as its font's extent, from the descent
    /// below the baseline to the ascent above it. In vertical writing, the box
    /// runs from the vertical origin down by the vertical width. The box of a
    /// rotated or skewed glyph is rotated or skewed too.
    pub fn corners(&self) -> [(f64, f64); 4] {
        let (left, right, bottom, top) = match &self.vertical {
            Some(v) => (0.0, self.width, v.origin_y + v.width, v.origin_y),
            None => {
                let extent = self.font.extent();
                (0.0, self.width, extent.descent, extent.ascent)
            }
        };
        [
            self.matrix.apply(left, bottom),
            self.matrix.apply(right, bottom),
            self.matrix.apply(right, top),
            self.matrix.apply(left, top),
        ]
    }

    /// The smallest rectangle around [`corners`](Self::corners).
    pub fn bounding_box(&self) -> Rectangle {
        Rectangle::around(&self.corners())
    }
}

/// A marked-content sequence, between `BMC` or `BDC` and `EMC` (ISO 32000-2,
/// section 14.6).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkedContent {
    /// The tag, like `/Span`, `/P` or `/Artifact`.
    pub tag: Name,
    /// The property list of `BDC`, given in the content or named in the
    /// resources' `/Properties`.
    pub properties: Option<Dictionary>,
    /// `/MCID` in the properties: the sequence's identifier in the structure
    /// tree.
    pub mcid: Option<i64>,
    /// `/ActualText` in the properties: the text the glyphs of the sequence
    /// stand for, like `fi` for a ligature or a word for a drawn logo
    /// (section 14.9.4).
    pub actual_text: Option<String>,
    /// The sequence this one is in, an index into the same list.
    pub parent: Option<usize>,
}

/// An image painted on the page, with `Do` or as an inline image (ISO
/// 32000-2, section 8.9).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedImage {
    pub source: ImageSource,
    /// The current transformation matrix when it was painted. An image fills
    /// the unit square of its own space, so this maps that square to the
    /// default user space of the page (section 8.9.4).
    pub matrix: Matrix,
    /// The innermost marked-content sequence the image is in, an index into
    /// [`PageContent::marked_content`].
    pub marked_content: Option<usize>,
    /// The opacity and clip it was painted with.
    pub paint: PaintState,
}

/// Where the data of an image is.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    /// An image XObject: its object, a stream.
    XObject { id: ObjectId, object: Arc<Object> },
    /// An inline image: its dictionary, with the abbreviations it was written
    /// with, and its data, still encoded.
    Inline {
        dictionary: Dictionary,
        data: Vec<u8>,
    },
}

impl PlacedImage {
    /// The corners of the image on the page, in the default user space: the
    /// unit square, through its matrix, from its bottom left corner around.
    pub fn corners(&self) -> [(f64, f64); 4] {
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| self.matrix.apply(x, y))
    }

    /// The smallest rectangle around [`corners`](Self::corners).
    pub fn bounding_box(&self) -> Rectangle {
        Rectangle::around(&self.corners())
    }
}

/// A path that was filled, stroked or both (ISO 32000-2, section 8.5.3).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintedPath {
    pub path: Path,
    /// From the path's user space to the default user space of the page.
    pub matrix: Matrix,
    /// The fill rule, when it was filled.
    pub fill: Option<FillRule>,
    /// Whether it was stroked, with the line and colors of `paint`.
    pub stroke: bool,
    pub paint: PaintState,
}

/// One thing painted: an index into the `paths`, `glyphs` or `images` of the
/// [`PageContent`], or the start or end of a transparency group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintItem {
    Path(usize),
    Glyph(usize),
    Image(usize),
    /// Everything up to the matching `EndGroup` is painted on its own and put
    /// on the page as a whole: an index into the `groups`.
    BeginGroup(usize),
    EndGroup,
}

/// A transparency group painted with an opacity, a blend mode or a soft mask:
/// the state of what paints it, whose clip, opacity, mode and mask apply to
/// the group as a whole (ISO 32000-2, section 11.4).
#[derive(Debug, Clone, PartialEq)]
pub struct GroupPaint {
    pub paint: PaintState,
}

/// What the content of a page left on it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageContent {
    /// The glyphs, in the order they were painted.
    pub glyphs: Vec<Glyph>,
    /// The images, in the order they were painted.
    pub images: Vec<PlacedImage>,
    /// The marked-content sequences, in the order they start.
    pub marked_content: Vec<MarkedContent>,
    /// The paths that were filled or stroked, when the interpreter paints
    /// ([`Interpreter::painting`]): empty otherwise.
    pub paths: Vec<PaintedPath>,
    /// Everything painted, in the order it was, when the interpreter paints.
    pub order: Vec<PaintItem>,
    /// The groups `BeginGroup` items refer to.
    pub groups: Vec<GroupPaint>,
    /// Problems found and worked around. Those inside a content stream are
    /// reported at the start of its data in the file.
    pub diagnostics: Diagnostics,
}

impl PageContent {
    /// The marked-content sequences `glyph` is in, from the innermost out.
    pub fn marked_content_of<'c>(
        &'c self,
        glyph: &Glyph,
    ) -> impl Iterator<Item = &'c MarkedContent> + 'c {
        let mut next = glyph.marked_content;
        std::iter::from_fn(move || {
            let sequence = self.marked_content.get(next?)?;
            next = sequence.parent;
            Some(sequence)
        })
    }
}

/// Runs content streams (ISO 32000-2, section 8 and 9): it follows the
/// graphics and text state through their operators, and collects the glyphs
/// they paint.
///
/// It keeps the fonts it reads, so running the pages of a document with the
/// same interpreter reads each font once.
#[derive(Debug)]
pub struct Interpreter {
    limits: Limits,
    fonts: HashMap<ObjectId, Option<Arc<Font>>>,
    painting: bool,
}

impl Interpreter {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            fonts: HashMap::new(),
            painting: false,
        }
    }

    /// An interpreter that also keeps the paths that are filled and stroked,
    /// and the order everything is painted in, which drawing a page needs.
    pub fn painting(limits: Limits) -> Self {
        Self {
            painting: true,
            ..Self::new(limits)
        }
    }

    /// Runs the content of `page`, its `/Contents` stream or the streams of
    /// its `/Contents` array, joined, with the page's resources.
    ///
    /// It never fails: what can't be read is reported in the diagnostics of
    /// the result, and a page whose content can't be decoded has nothing on
    /// it.
    pub fn run_page(&mut self, resolver: &mut Resolver<'_>, page: &Page) -> PageContent {
        let mut diagnostics = Diagnostics::new(self.limits.max_diagnostics);
        let (data, pieces) = page_contents(resolver, page, &mut diagnostics);
        let mut run = Run::new(self, resolver, diagnostics);
        run.execute(&data, &pieces, page.resources());
        run.finish()
    }

    /// Runs `content`, a content stream already decoded, with `resources`.
    /// Its diagnostics are reported at `offset`.
    pub fn run_content(
        &mut self,
        resolver: &mut Resolver<'_>,
        content: &[u8],
        resources: &Resources,
        offset: usize,
    ) -> PageContent {
        let diagnostics = Diagnostics::new(self.limits.max_diagnostics);
        let mut run = Run::new(self, resolver, diagnostics);
        run.execute(content, &[(0, offset)], resources);
        run.finish()
    }

    /// Runs the form XObject `id` as if it were painted with the current
    /// matrix `matrix`: its own `/Matrix` and resources apply. For drawing
    /// the form of a soft mask, which paints on its own.
    pub fn run_form(
        &mut self,
        resolver: &mut Resolver<'_>,
        id: ObjectId,
        matrix: &Matrix,
    ) -> PageContent {
        let diagnostics = Diagnostics::new(self.limits.max_diagnostics);
        let mut run = Run::new(self, resolver, diagnostics);
        run.paint_form_root(id, matrix);
        run.finish()
    }

    // The font that `id` refers to, read once.
    fn font(
        &mut self,
        resolver: &mut Resolver<'_>,
        id: ObjectId,
        diagnostics: &mut Diagnostics,
    ) -> Option<Arc<Font>> {
        if let Some(font) = self.fonts.get(&id) {
            return font.clone();
        }
        let offset = resolver.object_offset(id.number);
        let font = resolver.resolve(id).ok().and_then(|object| {
            let dictionary = object.as_dictionary()?;
            let mut font = Font::read(resolver, dictionary, self.limits, offset, diagnostics)?;
            if self.painting {
                font.set_render_program(read_render_program(resolver, dictionary));
                if font.is_type3() {
                    font.set_type3_dictionary(Some(Arc::new(dictionary.clone())));
                }
            }
            Some(Arc::new(font))
        });
        self.fonts.insert(id, font.clone());
        font
    }
}

// The data of the page's content streams, joined by a line feed, and where
// each one starts in it and in the file.
fn page_contents(
    resolver: &mut Resolver<'_>,
    page: &Page,
    diagnostics: &mut Diagnostics,
) -> (Vec<u8>, Vec<(usize, usize)>) {
    let mut data = Vec::new();
    let mut pieces = Vec::new();
    let page_offset = page.id().map_or(0, |id| resolver.object_offset(id.number));
    let streams: Vec<Object> = match page.dictionary().get("Contents") {
        None | Some(Object::Null) => return (data, pieces),
        Some(Object::Reference(id)) => match resolver.resolve(*id).as_deref() {
            Ok(Object::Array(items)) => items.clone(),
            Ok(Object::Stream(_)) => vec![Object::Reference(*id)],
            Ok(Object::Null) => return (data, pieces),
            _ => {
                diagnostics.push(DiagnosticKind::InvalidContents, page_offset);
                return (data, pieces);
            }
        },
        Some(Object::Array(items)) => items.clone(),
        Some(_) => {
            diagnostics.push(DiagnosticKind::InvalidContents, page_offset);
            return (data, pieces);
        }
    };
    for stream in streams {
        let Object::Reference(id) = stream else {
            if !stream.is_null() {
                diagnostics.push(DiagnosticKind::InvalidContents, page_offset);
            }
            continue;
        };
        if matches!(resolver.resolve(id).as_deref(), Ok(Object::Null)) {
            continue;
        }
        let offset = resolver.object_offset(id.number);
        match resolver.stream_data(id) {
            Ok(decoded) => {
                diagnostics.append(&decoded.diagnostics);
                if !data.is_empty() {
                    // Streams can split an operation, but not a token.
                    data.push(b'\n');
                }
                pieces.push((data.len(), offset));
                data.extend_from_slice(&decoded.data);
            }
            Err(error) if error.kind() == ErrorKind::PasswordRequired => {
                diagnostics.push(DiagnosticKind::LockedContent, offset);
                return (Vec::new(), Vec::new());
            }
            Err(_) => diagnostics.push(DiagnosticKind::InvalidContents, offset),
        }
    }
    (data, pieces)
}

// One run of a page's content.
struct Run<'i, 'r, 'f> {
    interpreter: &'i mut Interpreter,
    resolver: &'r mut Resolver<'f>,
    limits: Limits,
    stack: GraphicsStack,
    text: TextObject,
    in_text_object: bool,
    reported_outside_text: bool,
    // How many `BX` sections are open: unknown operators in them are expected.
    compatibility: usize,
    operations: usize,
    stopped: bool,
    // The forms and patterns being painted, the outermost first.
    forms: Vec<ObjectId>,
    // The default space of each content stream being run, the page's first:
    // where the matrix of a pattern it uses starts from (section 8.7.3.1).
    bases: Vec<Matrix>,
    // The tiling patterns already run on this page.
    patterns: Vec<ObjectId>,
    // How many patterns are being run: their glyphs aren't text of the page.
    in_pattern: usize,
    // The marked-content sequences open, the outermost first.
    open_marked: Vec<usize>,
    marked_content: Vec<MarkedContent>,
    glyphs: Vec<Glyph>,
    images: Vec<PlacedImage>,
    painting: bool,
    // How many Type 3 glyphs are being run inside each other, and whether the
    // one being run has no colors of its own (`d1`).
    type3_depth: usize,
    uncolored: bool,
    // The path being built, in user space, its current point and where its
    // subpath started, and the rule of the clip it will make.
    path: Path,
    current: Option<(f64, f64)>,
    subpath: (f64, f64),
    pending_clip: Option<FillRule>,
    paths: Vec<PaintedPath>,
    order: Vec<PaintItem>,
    groups: Vec<GroupPaint>,
    diagnostics: Diagnostics,
}

// Saved graphics states, at most. Real files nest a few dozen.
const MAX_SAVED_STATES: usize = 1024;

// Forms painted inside each other, at most. Real files nest a few.
const MAX_NESTED_FORMS: usize = 32;

impl<'i, 'r, 'f> Run<'i, 'r, 'f> {
    fn new(
        interpreter: &'i mut Interpreter,
        resolver: &'r mut Resolver<'f>,
        diagnostics: Diagnostics,
    ) -> Self {
        let limits = interpreter.limits;
        let painting = interpreter.painting;
        Self {
            interpreter,
            resolver,
            limits,
            stack: GraphicsStack::new(GraphicsState::default(), MAX_SAVED_STATES),
            text: TextObject::default(),
            in_text_object: false,
            reported_outside_text: false,
            compatibility: 0,
            operations: 0,
            stopped: false,
            forms: Vec::new(),
            bases: vec![Matrix::IDENTITY],
            patterns: Vec::new(),
            in_pattern: 0,
            open_marked: Vec::new(),
            marked_content: Vec::new(),
            glyphs: Vec::new(),
            images: Vec::new(),
            painting,
            type3_depth: 0,
            uncolored: false,
            path: Path::new(),
            current: None,
            subpath: (0.0, 0.0),
            pending_clip: None,
            paths: Vec::new(),
            order: Vec::new(),
            groups: Vec::new(),
            diagnostics,
        }
    }

    fn finish(self) -> PageContent {
        PageContent {
            glyphs: self.glyphs,
            images: self.images,
            marked_content: self.marked_content,
            paths: self.paths,
            order: self.order,
            groups: self.groups,
            diagnostics: self.diagnostics,
        }
    }

    // Runs the form `id` from the start, with `matrix` as the current matrix
    // before the form's own.
    fn paint_form_root(&mut self, id: ObjectId, matrix: &Matrix) {
        let form_offset = self.resolver.object_offset(id.number);
        let Ok(object) = self.resolver.resolve(id) else {
            return;
        };
        let Some(stream) = object.as_stream() else {
            return;
        };
        let own_matrix = stream
            .dictionary()
            .get("Matrix")
            .map(|entry| self.resolved(entry))
            .and_then(|entry| entry.as_array().and_then(Matrix::from_objects))
            .unwrap_or(Matrix::IDENTITY);
        let resources = stream
            .dictionary()
            .get("Resources")
            .and_then(|_| Resources::of(self.resolver, &object))
            .unwrap_or_default();
        let Ok(decoded) = self.resolver.stream_data(id) else {
            return;
        };
        let ctm = own_matrix.then(matrix);
        if !ctm.is_finite() {
            return;
        }
        self.stack.current_mut().ctm = ctm;
        self.forms.push(id);
        self.bases.push(ctm);
        self.execute(&decoded.data, &[(0, form_offset)], &resources);
    }

    // Runs `data`, whose pieces start at the given offsets in it and in the
    // file.
    fn execute(&mut self, data: &[u8], pieces: &[(usize, usize)], resources: &Resources) {
        let marked = self.open_marked.len();
        let mut parser = ContentParser::new(data, self.limits);
        while !self.stopped {
            let Some(operation) = parser.next_operation() else {
                break;
            };
            let offset = file_offset(pieces, operation.offset);
            self.operation(&operation, offset, resources);
        }
        for diagnostic in parser.diagnostics().iter() {
            let offset = file_offset(pieces, diagnostic.offset());
            self.diagnostics.push(diagnostic.kind(), offset);
        }
        // Sequences can't go past the end of their content stream.
        if self.open_marked.len() > marked && !self.stopped {
            let offset = file_offset(pieces, data.len());
            self.diagnostics
                .push(DiagnosticKind::UnbalancedMarkedContent, offset);
        }
        self.open_marked.truncate(marked);
    }

    // Counts one more operation or glyph. `false`, once, when the limit is
    // reached: the run stops there.
    fn count(&mut self, offset: usize) -> bool {
        if self.stopped {
            return false;
        }
        self.operations += 1;
        if self.operations > self.limits.max_content_operations {
            self.diagnostics
                .push(DiagnosticKind::ContentLimitReached, offset);
            self.stopped = true;
            return false;
        }
        true
    }

    fn operation(&mut self, operation: &Operation<'_>, offset: usize, resources: &Resources) {
        if !self.count(offset) {
            return;
        }
        let operands = operation.operands;
        match operation.operator {
            Operator::Save => {
                if !self.stack.save() {
                    self.diagnostics
                        .push(DiagnosticKind::TooManySavedStates, offset);
                }
            }
            Operator::Restore => {
                if !self.stack.restore() {
                    self.diagnostics
                        .push(DiagnosticKind::UnbalancedRestore, offset);
                }
            }
            Operator::Transform => match last(operands, 6).and_then(Matrix::from_objects) {
                Some(matrix) => {
                    if !self.stack.transform(&matrix) {
                        self.invalid(offset);
                    }
                }
                None => self.invalid(offset),
            },
            Operator::SetExtGState => self.ext_g_state(operands, offset, resources),
            Operator::BeginText => {
                if self.in_text_object {
                    self.diagnostics
                        .push(DiagnosticKind::UnbalancedTextObject, offset);
                }
                self.in_text_object = true;
                self.text = TextObject::default();
            }
            Operator::EndText => {
                if !self.in_text_object {
                    self.diagnostics
                        .push(DiagnosticKind::UnbalancedTextObject, offset);
                }
                self.in_text_object = false;
            }
            Operator::SetCharacterSpacing => {
                if let Some([value]) = self.numbers(operands, offset) {
                    self.stack.current_mut().text.character_spacing = value;
                }
            }
            Operator::SetWordSpacing => {
                if let Some([value]) = self.numbers(operands, offset) {
                    self.stack.current_mut().text.word_spacing = value;
                }
            }
            Operator::SetHorizontalScaling => {
                if let Some([value]) = self.numbers(operands, offset) {
                    self.stack.current_mut().text.horizontal_scaling = value / 100.0;
                }
            }
            Operator::SetLeading => {
                if let Some([value]) = self.numbers(operands, offset) {
                    self.stack.current_mut().text.leading = value;
                }
            }
            Operator::SetTextRise => {
                if let Some([value]) = self.numbers(operands, offset) {
                    self.stack.current_mut().text.rise = value;
                }
            }
            Operator::SetTextRenderingMode => {
                let mode = last(operands, 1)
                    .and_then(|operands| operands[0].as_integer())
                    .and_then(TextRenderingMode::from_number);
                match mode {
                    Some(mode) => self.stack.current_mut().text.rendering_mode = mode,
                    None => self.invalid(offset),
                }
            }
            Operator::SetFont => self.set_font(operands, offset, resources),
            Operator::MoveText => {
                if let Some([x, y]) = self.numbers(operands, offset) {
                    self.text_position(offset);
                    self.text.move_text(x, y);
                }
            }
            Operator::MoveTextSetLeading => {
                if let Some([x, y]) = self.numbers(operands, offset) {
                    self.text_position(offset);
                    self.stack.current_mut().text.leading = -y;
                    self.text.move_text(x, y);
                }
            }
            Operator::SetTextMatrix => match last(operands, 6).and_then(Matrix::from_objects) {
                Some(matrix) => {
                    self.text_position(offset);
                    self.text.set_matrix(matrix);
                }
                None => self.invalid(offset),
            },
            Operator::NextLine => {
                self.text_position(offset);
                let leading = self.stack.current().text.leading;
                self.text.next_line(leading);
            }
            Operator::ShowText => match last(operands, 1).and_then(|o| o[0].as_string()) {
                Some(string) => self.show(string, offset, resources),
                None => self.invalid(offset),
            },
            Operator::ShowTextAdjusted => match last(operands, 1).and_then(|o| o[0].as_array()) {
                Some(items) => self.show_adjusted(items, offset, resources),
                None => self.invalid(offset),
            },
            Operator::NextLineShowText => match last(operands, 1).and_then(|o| o[0].as_string()) {
                Some(string) => {
                    self.text_position(offset);
                    let leading = self.stack.current().text.leading;
                    self.text.next_line(leading);
                    self.show(string, offset, resources);
                }
                None => self.invalid(offset),
            },
            Operator::NextLineShowTextSpaced => {
                let found = last(operands, 3)
                    .and_then(|o| Some((o[0].as_number()?, o[1].as_number()?, o[2].as_string()?)));
                match found {
                    Some((word, character, string))
                        if word.is_finite() && character.is_finite() =>
                    {
                        let state = &mut self.stack.current_mut().text;
                        state.word_spacing = word;
                        state.character_spacing = character;
                        self.text_position(offset);
                        let leading = self.stack.current().text.leading;
                        self.text.next_line(leading);
                        self.show(string, offset, resources);
                    }
                    _ => self.invalid(offset),
                }
            }
            Operator::PaintXObject => self.paint_xobject(operands, offset, resources),
            Operator::SetFillColorSpace => self.set_color_space(operands, offset, resources, true),
            Operator::SetStrokeColorSpace => {
                self.set_color_space(operands, offset, resources, false)
            }
            Operator::SetFillColorExtended => {
                let name = operands.last().and_then(Object::as_name).cloned();
                if name.is_none() {
                    self.set_color(operands, true);
                }
                self.stack.current_mut().fill_pattern = name;
            }
            Operator::SetStrokeColorExtended => {
                if operands.last().and_then(Object::as_name).is_none() {
                    self.set_color(operands, false);
                }
            }
            Operator::SetFillColor => {
                self.stack.current_mut().fill_pattern = None;
                self.set_color(operands, true);
            }
            Operator::SetStrokeColor => self.set_color(operands, false),
            Operator::SetFillGray | Operator::SetStrokeGray => {
                let fill = operation.operator == Operator::SetFillGray;
                self.set_device_color(operands, 1, fill);
            }
            Operator::SetFillRgb | Operator::SetStrokeRgb => {
                let fill = operation.operator == Operator::SetFillRgb;
                self.set_device_color(operands, 3, fill);
            }
            Operator::SetFillCmyk | Operator::SetStrokeCmyk => {
                let fill = operation.operator == Operator::SetFillCmyk;
                self.set_device_color(operands, 4, fill);
            }
            Operator::SetLineWidth if self.painting => {
                if let Some([width]) = self.numbers(operands, offset) {
                    Arc::make_mut(&mut self.stack.current_mut().paint.line).width = width.max(0.0);
                }
            }
            Operator::SetLineCap if self.painting => {
                match last(operands, 1).and_then(|o| o[0].as_integer()) {
                    Some(cap @ 0..=2) => {
                        Arc::make_mut(&mut self.stack.current_mut().paint.line).cap =
                            [LineCap::Butt, LineCap::Round, LineCap::Square][cap as usize];
                    }
                    _ => self.invalid(offset),
                }
            }
            Operator::SetLineJoin if self.painting => {
                match last(operands, 1).and_then(|o| o[0].as_integer()) {
                    Some(join @ 0..=2) => {
                        Arc::make_mut(&mut self.stack.current_mut().paint.line).join =
                            [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel][join as usize];
                    }
                    _ => self.invalid(offset),
                }
            }
            Operator::SetMiterLimit if self.painting => {
                if let Some([limit]) = self.numbers(operands, offset) {
                    Arc::make_mut(&mut self.stack.current_mut().paint.line).miter_limit =
                        limit.max(1.0);
                }
            }
            Operator::SetDashPattern if self.painting => {
                let found =
                    last(operands, 2).and_then(|o| Some((o[0].as_array()?, o[1].as_number()?)));
                match found {
                    Some((items, phase)) if phase.is_finite() => {
                        let dash: Vec<f64> = items.iter().filter_map(Object::as_number).collect();
                        let line = Arc::make_mut(&mut self.stack.current_mut().paint.line);
                        line.dash = dash;
                        line.dash_phase = phase;
                    }
                    _ => self.invalid(offset),
                }
            }
            Operator::MoveTo if self.painting => {
                if let Some([x, y]) = self.numbers(operands, offset) {
                    self.path.move_to(x, y);
                    self.current = Some((x, y));
                    self.subpath = (x, y);
                }
            }
            Operator::LineTo if self.painting => {
                if let Some([x, y]) = self.numbers(operands, offset) {
                    if self.current.is_none() {
                        self.path.move_to(x, y);
                        self.subpath = (x, y);
                    } else {
                        self.path.line_to(x, y);
                    }
                    self.current = Some((x, y));
                }
            }
            Operator::CurveTo if self.painting => {
                if let Some([x1, y1, x2, y2, x3, y3]) = self.numbers(operands, offset) {
                    self.curve(x1, y1, x2, y2, x3, y3);
                }
            }
            Operator::CurveToFromCurrent if self.painting => {
                if let Some([x2, y2, x3, y3]) = self.numbers(operands, offset) {
                    let (x1, y1) = self.current.unwrap_or((x2, y2));
                    self.curve(x1, y1, x2, y2, x3, y3);
                }
            }
            Operator::CurveToEnd if self.painting => {
                if let Some([x1, y1, x3, y3]) = self.numbers(operands, offset) {
                    self.curve(x1, y1, x3, y3, x3, y3);
                }
            }
            Operator::ClosePath => self.close_path(),
            Operator::Rectangle if self.painting => {
                if let Some([x, y, width, height]) = self.numbers(operands, offset) {
                    self.path.rectangle(x, y, width, height);
                    self.current = Some((x, y));
                    self.subpath = (x, y);
                }
            }
            Operator::SetGlyphWidthAndBox if self.type3_depth > 0 => self.uncolored = true,
            Operator::Clip => self.pending_clip = Some(FillRule::NonZero),
            Operator::ClipEvenOdd => self.pending_clip = Some(FillRule::EvenOdd),
            Operator::Stroke
            | Operator::CloseAndStroke
            | Operator::Fill
            | Operator::FillObsolete
            | Operator::FillEvenOdd
            | Operator::FillAndStroke
            | Operator::FillEvenOddAndStroke
            | Operator::CloseFillAndStroke
            | Operator::CloseFillEvenOddAndStroke
            | Operator::EndPath => self.paint_path(operation.operator, offset, resources),
            Operator::InlineImage => {
                if let Some(image) = &operation.inline_image {
                    let dictionary = self.inline_color_space(image.dictionary, resources);
                    self.place_image(ImageSource::Inline {
                        dictionary,
                        data: image.data.to_vec(),
                    });
                }
            }
            Operator::BeginMarkedContent => match last(operands, 1).and_then(|o| o[0].as_name()) {
                Some(tag) => self.begin_marked_content(tag.clone(), None),
                None => self.invalid(offset),
            },
            Operator::BeginMarkedContentWithProperties => {
                self.begin_marked_content_with_properties(operands, offset, resources)
            }
            Operator::EndMarkedContent => {
                if self.open_marked.pop().is_none() {
                    self.diagnostics
                        .push(DiagnosticKind::UnbalancedMarkedContent, offset);
                }
            }
            Operator::BeginCompatibility => self.compatibility += 1,
            Operator::EndCompatibility => self.compatibility = self.compatibility.saturating_sub(1),
            Operator::Unknown(_) if self.compatibility == 0 => {
                self.diagnostics
                    .push(DiagnosticKind::UnknownOperator, offset);
            }
            // Paths, colours, shadings, marked points and Type 3 glyph
            // metrics don't change where text goes.
            _ => {}
        }
    }

    // `c`, `v` and `y`: a curve from the current point.
    fn curve(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        if !self.painting {
            return;
        }
        if self.current.is_none() {
            self.path.move_to(x1, y1);
            self.subpath = (x1, y1);
        }
        self.path.curve_to(x1, y1, x2, y2, x3, y3);
        self.current = Some((x3, y3));
    }

    fn close_path(&mut self) {
        if self.painting && self.current.is_some() {
            self.path.close();
            self.current = Some(self.subpath);
        }
    }

    // A path-painting operator: the path is filled, stroked or neither, and
    // then it clips if `W` or `W*` came before, with the clip that holds
    // after it (section 8.5.4).
    fn paint_path(&mut self, operator: Operator, offset: usize, resources: &Resources) {
        use Operator::{
            CloseAndStroke, CloseFillAndStroke, CloseFillEvenOddAndStroke, Fill, FillAndStroke,
            FillEvenOdd, FillEvenOddAndStroke, FillObsolete, Stroke,
        };
        if matches!(
            operator,
            CloseAndStroke | CloseFillAndStroke | CloseFillEvenOddAndStroke
        ) {
            self.close_path();
        }
        let fill = match operator {
            Fill | FillObsolete | FillAndStroke | CloseFillAndStroke => Some(FillRule::NonZero),
            FillEvenOdd | FillEvenOddAndStroke | CloseFillEvenOddAndStroke => {
                Some(FillRule::EvenOdd)
            }
            _ => None,
        };
        let stroke = matches!(
            operator,
            Stroke
                | CloseAndStroke
                | FillAndStroke
                | FillEvenOddAndStroke
                | CloseFillAndStroke
                | CloseFillEvenOddAndStroke
        );
        // A pattern fill runs the pattern once for its images.
        if fill.is_some()
            && let Some(name) = self.stack.current().fill_pattern.clone()
        {
            self.paint_pattern(&name, offset, resources);
        }
        let patterned = self.stack.current().fill_pattern.is_some();
        let fill = fill.filter(|_| !patterned);
        if self.painting
            && self.in_pattern == 0
            && !self.path.is_empty()
            && (fill.is_some() || stroke)
        {
            let state = self.stack.current();
            self.order.push(PaintItem::Path(self.paths.len()));
            self.paths.push(PaintedPath {
                path: self.path.clone(),
                matrix: state.ctm,
                fill,
                stroke,
                paint: state.paint.clone(),
            });
        }
        if let Some(rule) = self.pending_clip.take()
            && !self.path.is_empty()
        {
            let state = self.stack.current_mut();
            let node = ClipNode {
                parent: state.paint.clip.clone(),
                path: self.path.clone(),
                matrix: state.ctm,
                rule,
            };
            state.paint.clip = Some(Arc::new(node));
        }
        self.path = Path::new();
        self.current = None;
    }

    // `cs` and `CS`: selects a color space, and its initial color.
    fn set_color_space(
        &mut self,
        operands: &[Object],
        offset: usize,
        resources: &Resources,
        fill: bool,
    ) {
        let Some(name) = last(operands, 1).and_then(|o| o[0].as_name().cloned()) else {
            self.invalid(offset);
            return;
        };
        // Patterns are painted from `scn`, with their name.
        if name.as_bytes() == b"Pattern" {
            return;
        }
        if fill {
            self.stack.current_mut().fill_pattern = None;
        }
        if !self.painting {
            return;
        }
        let entry = match name.as_bytes() {
            b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" => Some(Object::Name(name.clone())),
            _ => self.resource(resources, "ColorSpace", name.as_bytes()),
        };
        let space = entry.and_then(|entry| read_color_space(self.resolver, &entry));
        let Some(space) = space else {
            self.diagnostics
                .push(DiagnosticKind::MissingResource, offset);
            return;
        };
        let color = space.rgb(&space.initial_color());
        let paint = &mut self.stack.current_mut().paint;
        if fill {
            paint.fill_color = color;
            paint.fill_space = Some(Arc::new(space));
        } else {
            paint.stroke_color = color;
            paint.stroke_space = Some(Arc::new(space));
        }
    }

    // `sc` and `scn` with numbers: a color in the current color space.
    fn set_color(&mut self, operands: &[Object], fill: bool) {
        if !self.painting || self.uncolored {
            return;
        }
        let values: Vec<f64> = operands.iter().filter_map(Object::as_number).collect();
        let paint = &mut self.stack.current_mut().paint;
        let space = if fill {
            &paint.fill_space
        } else {
            &paint.stroke_space
        };
        let color = match space {
            Some(space) => {
                let count = space.components();
                let start = values.len().saturating_sub(count);
                space.rgb(&values[start..])
            }
            // Without a space, the device space the count says.
            None => device_color(&values),
        };
        if fill {
            paint.fill_color = color;
        } else {
            paint.stroke_color = color;
        }
    }

    // `g`, `rg` and `k` (and their stroke forms): a device color.
    fn set_device_color(&mut self, operands: &[Object], count: usize, fill: bool) {
        if fill {
            self.stack.current_mut().fill_pattern = None;
        }
        if !self.painting || self.uncolored {
            return;
        }
        let Some(values) = last(operands, count) else {
            self.diagnostics.push(DiagnosticKind::InvalidOperands, 0);
            return;
        };
        let values: Vec<f64> = values.iter().filter_map(Object::as_number).collect();
        if values.len() != count {
            self.diagnostics.push(DiagnosticKind::InvalidOperands, 0);
            return;
        }
        let space = match count {
            1 => ColorSpace::Gray,
            3 => ColorSpace::Rgb,
            _ => ColorSpace::Cmyk,
        };
        let color = space.rgb(&values);
        let paint = &mut self.stack.current_mut().paint;
        if fill {
            paint.fill_color = color;
            paint.fill_space = Some(Arc::new(space));
        } else {
            paint.stroke_color = color;
            paint.stroke_space = Some(Arc::new(space));
        }
    }

    // Fills an area with the pattern `name` of the resources' `/Pattern`. A
    // tiling pattern is run once on a page, for its images, with its
    // `/Matrix` from the default space of the content that uses it, as
    // writers such as ONLYOFFICE paint images (section 8.7.3). The cells are
    // not repeated and the area doesn't clip them: one cell is what most
    // images painted this way need. Shading patterns paint no images.
    fn paint_pattern(&mut self, name: &Name, offset: usize, resources: &Resources) {
        let Some(Object::Reference(id)) = self.resource(resources, "Pattern", name.as_bytes())
        else {
            return;
        };
        if self.patterns.contains(&id) || self.forms.contains(&id) {
            return;
        }
        if self.forms.len() >= MAX_NESTED_FORMS {
            self.diagnostics
                .push(DiagnosticKind::XObjectTooDeep, offset);
            return;
        }
        let Ok(object) = self.resolver.resolve(id) else {
            return;
        };
        let Some(stream) = object.as_stream() else {
            return;
        };
        let dictionary = stream.dictionary();
        let tiling = dictionary
            .get("PatternType")
            .map(|entry| self.resolved(entry))
            .and_then(|entry| entry.as_integer())
            == Some(1);
        if !tiling {
            return;
        }
        self.patterns.push(id);
        let matrix = dictionary
            .get("Matrix")
            .map(|entry| self.resolved(entry))
            .and_then(|entry| entry.as_array().and_then(Matrix::from_objects))
            .unwrap_or(Matrix::IDENTITY);
        let own_resources = dictionary
            .get("Resources")
            .and_then(|_| Resources::of(self.resolver, &object));
        let Ok(decoded) = self.resolver.stream_data(id) else {
            return;
        };
        let base = *self.bases.last().expect("the page's base is always there");
        let pattern_offset = self.resolver.object_offset(id.number);
        let depth = self.stack.depth();
        if !self.stack.save() {
            self.diagnostics
                .push(DiagnosticKind::TooManySavedStates, offset);
        }
        let text = self.text;
        let in_text_object = self.in_text_object;
        let ctm = matrix.then(&base);
        if ctm.is_finite() {
            let state = self.stack.current_mut();
            state.ctm = ctm;
            state.fill_pattern = None;
            self.forms.push(id);
            self.bases.push(ctm);
            self.in_pattern += 1;
            let pattern_resources = own_resources.unwrap_or_default();
            self.execute(&decoded.data, &[(0, pattern_offset)], &pattern_resources);
            self.in_pattern -= 1;
            self.bases.pop();
            self.forms.pop();
        }
        while self.stack.depth() > depth {
            self.stack.restore();
        }
        self.text = text;
        self.in_text_object = in_text_object;
    }

    // The dictionary of an inline image, with a color space named in the
    // resources' `/ColorSpace` put in its place (section 8.9.7).
    fn inline_color_space(&mut self, dictionary: &Dictionary, resources: &Resources) -> Dictionary {
        let mut dictionary = dictionary.clone();
        let key = if dictionary.contains_key("CS") {
            "CS"
        } else {
            "ColorSpace"
        };
        if let Some(Object::Name(name)) = dictionary.get(key).cloned()
            && !matches!(
                name.as_bytes(),
                b"G" | b"RGB"
                    | b"CMYK"
                    | b"I"
                    | b"DeviceGray"
                    | b"DeviceRGB"
                    | b"DeviceCMYK"
                    | b"Indexed"
            )
            && let Some(entry) = self.resource(resources, "ColorSpace", name.as_bytes())
        {
            let entry = (*self.resolved(&entry)).clone();
            dictionary.insert(key.into(), entry);
        }
        dictionary
    }

    fn place_image(&mut self, source: ImageSource) {
        // The images of a tiling pattern are painted once, where the pattern
        // is run (see `paint_pattern`).
        if self.painting {
            self.order.push(PaintItem::Image(self.images.len()));
        }
        self.images.push(PlacedImage {
            source,
            matrix: self.stack.current().ctm,
            marked_content: self.open_marked.last().copied(),
            paint: self.stack.current().paint.clone(),
        });
    }

    fn invalid(&mut self, offset: usize) {
        self.diagnostics
            .push(DiagnosticKind::InvalidOperands, offset);
    }

    // The last `N` operands, as finite numbers. `None`, reported, if there
    // aren't `N` or they aren't numbers.
    fn numbers<const N: usize>(&mut self, operands: &[Object], offset: usize) -> Option<[f64; N]> {
        let numbers = last(operands, N).and_then(|operands| {
            let mut numbers = [0.0; N];
            for (number, operand) in numbers.iter_mut().zip(operands) {
                *number = operand.as_number().filter(|value| value.is_finite())?;
            }
            Some(numbers)
        });
        if numbers.is_none() {
            self.invalid(offset);
        }
        numbers
    }

    // Text positioning and showing belong in a text object. Outside one, they
    // are run anyway, as readers do, and reported once.
    fn text_position(&mut self, offset: usize) {
        if !self.in_text_object && !self.reported_outside_text {
            self.reported_outside_text = true;
            self.diagnostics
                .push(DiagnosticKind::TextOutsideTextObject, offset);
        }
    }

    // `Tf`: the font named in the resources' `/Font`, and its size.
    fn set_font(&mut self, operands: &[Object], offset: usize, resources: &Resources) {
        let Some((name, size)) =
            last(operands, 2).and_then(|o| Some((o[0].as_name()?.clone(), o[1].as_number()?)))
        else {
            self.invalid(offset);
            return;
        };
        if !size.is_finite() {
            self.invalid(offset);
            return;
        }
        let entry = self.resource(resources, "Font", name.as_bytes());
        let font = entry.and_then(|entry| self.font_from(&entry));
        if font.is_none() {
            self.diagnostics.push(DiagnosticKind::MissingFont, offset);
        }
        let state = &mut self.stack.current_mut().text;
        state.font = font;
        state.font_size = size;
    }

    // A font from its entry in a resource dictionary or an `ExtGState`.
    fn font_from(&mut self, entry: &Object) -> Option<Arc<Font>> {
        match entry {
            Object::Reference(id) => {
                self.interpreter
                    .font(self.resolver, *id, &mut self.diagnostics)
            }
            // Fonts should be indirect objects, but a direct one is read too,
            // each time.
            Object::Dictionary(dictionary) => {
                let mut font = Font::read(
                    self.resolver,
                    dictionary,
                    self.limits,
                    0,
                    &mut self.diagnostics,
                )?;
                if self.painting {
                    font.set_render_program(read_render_program(self.resolver, dictionary));
                    if font.is_type3() {
                        font.set_type3_dictionary(Some(Arc::new(dictionary.clone())));
                    }
                }
                Some(Arc::new(font))
            }
            _ => None,
        }
    }

    // `gs`: of the parameters of an `ExtGState`, only `/Font` matters here.
    fn ext_g_state(&mut self, operands: &[Object], offset: usize, resources: &Resources) {
        let Some(name) = last(operands, 1).and_then(|o| o[0].as_name().cloned()) else {
            self.invalid(offset);
            return;
        };
        let Some(entry) = self.resource(resources, "ExtGState", name.as_bytes()) else {
            self.diagnostics
                .push(DiagnosticKind::MissingResource, offset);
            return;
        };
        let Some(parameters) = self.dictionary(&entry) else {
            self.diagnostics
                .push(DiagnosticKind::MissingResource, offset);
            return;
        };
        if self.painting {
            self.paint_parameters(&parameters);
        }
        let Some(font) = parameters.get("Font") else {
            return;
        };
        let font = self.resolved(font);
        let pair = font
            .as_array()
            .filter(|items| items.len() == 2)
            .and_then(|items| Some((items[0].clone(), self.resolved(&items[1]).as_number()?)));
        let Some((font, size)) = pair.filter(|(_, size)| size.is_finite()) else {
            self.invalid(offset);
            return;
        };
        let font = self.font_from(&font);
        if font.is_none() {
            self.diagnostics.push(DiagnosticKind::MissingFont, offset);
        }
        let state = &mut self.stack.current_mut().text;
        state.font = font;
        state.font_size = size;
    }

    // The line and opacity parameters of an `ExtGState` (table 57).
    fn paint_parameters(&mut self, parameters: &Dictionary) {
        let number = |this: &mut Self, key: &str| {
            let entry = parameters.get(key)?;
            this.resolved(entry).as_number().filter(|v| v.is_finite())
        };
        let alpha = (number(self, "ca"), number(self, "CA"));
        let width = number(self, "LW");
        let cap = number(self, "LC");
        let join = number(self, "LJ");
        let miter = number(self, "ML");
        let dash = parameters
            .get("D")
            .map(|entry| self.resolved(entry))
            .and_then(|entry| {
                let items = entry.as_array()?;
                let pattern = self.resolved(items.first()?);
                let phase = self.resolved(items.get(1)?).as_number()?;
                let lengths: Vec<f64> = pattern
                    .as_array()?
                    .iter()
                    .filter_map(|item| self.resolved(item).as_number())
                    .collect();
                Some((lengths, phase))
            });
        let blend = parameters
            .get("BM")
            .map(|e| self.resolved(e))
            .and_then(|entry| {
                // A name, or an array of them of which the first one known counts.
                let names: Vec<Name> = match &*entry {
                    Object::Name(name) => vec![name.clone()],
                    Object::Array(items) => {
                        items.iter().filter_map(|i| i.as_name().cloned()).collect()
                    }
                    _ => Vec::new(),
                };
                names
                    .iter()
                    .find_map(|name| BlendMode::from_name(name.as_bytes()))
            });
        let soft_mask = parameters
            .get("SMask")
            .map(|e| self.resolved(e))
            .map(|entry| {
                let dictionary = entry.as_dictionary()?;
                let Object::Reference(form) = dictionary.get("G")? else {
                    return None;
                };
                let luminosity = dictionary
                    .get("S")
                    .and_then(Object::as_name)
                    .is_some_and(|name| name.as_bytes() == b"Luminosity");
                let backdrop = dictionary
                    .get("BC")
                    .map(|e| self.resolved(e))
                    .and_then(|bc| {
                        let values: Vec<f64> = bc
                            .as_array()?
                            .iter()
                            .filter_map(Object::as_number)
                            .collect();
                        Some(device_color(&values))
                    })
                    .unwrap_or([0, 0, 0]);
                Some(Arc::new(SoftMask {
                    form: *form,
                    matrix: self.stack.current().ctm,
                    luminosity,
                    backdrop,
                }))
            });
        let paint = &mut self.stack.current_mut().paint;
        if let Some(mode) = blend {
            paint.blend = mode;
        }
        // `/SMask /None` removes the mask; a dictionary sets one.
        if let Some(mask) = soft_mask {
            paint.soft_mask = mask;
        }
        if let Some(a) = alpha.0 {
            paint.fill_alpha = a.clamp(0.0, 1.0);
        }
        if let Some(a) = alpha.1 {
            paint.stroke_alpha = a.clamp(0.0, 1.0);
        }
        if width.is_some() || cap.is_some() || join.is_some() || miter.is_some() || dash.is_some() {
            let line = Arc::make_mut(&mut paint.line);
            if let Some(w) = width {
                line.width = w.max(0.0);
            }
            if let Some(c) = cap.filter(|c| (0.0..=2.0).contains(c)) {
                line.cap = [LineCap::Butt, LineCap::Round, LineCap::Square][c as usize];
            }
            if let Some(j) = join.filter(|j| (0.0..=2.0).contains(j)) {
                line.join = [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel][j as usize];
            }
            if let Some(m) = miter {
                line.miter_limit = m.max(1.0);
            }
            if let Some((lengths, phase)) = dash {
                line.dash = lengths;
                line.dash_phase = phase;
            }
        }
    }

    // `Do`: paints the XObject named in the resources' `/XObject`. A form is
    // run inside a saved graphics state, with its `/Matrix` and its own
    // resources, or those of what paints it when it has none (section 8.10).
    // An image is placed where the current transformation matrix puts it.
    fn paint_xobject(&mut self, operands: &[Object], offset: usize, resources: &Resources) {
        let Some(name) = last(operands, 1).and_then(|o| o[0].as_name().cloned()) else {
            self.invalid(offset);
            return;
        };
        let entry = self.resource(resources, "XObject", name.as_bytes());
        let Some(Object::Reference(id)) = entry else {
            self.diagnostics
                .push(DiagnosticKind::MissingResource, offset);
            return;
        };
        let Ok(object) = self.resolver.resolve(id) else {
            self.diagnostics
                .push(DiagnosticKind::MissingResource, offset);
            return;
        };
        let Some(stream) = object.as_stream() else {
            self.diagnostics
                .push(DiagnosticKind::MissingResource, offset);
            return;
        };
        let dictionary = stream.dictionary();
        let subtype = dictionary.get("Subtype").map(|entry| self.resolved(entry));
        match subtype
            .as_deref()
            .and_then(Object::as_name)
            .map(|n| n.as_bytes())
        {
            Some(b"Form") => {}
            Some(b"Image") => {
                self.place_image(ImageSource::XObject {
                    id,
                    object: object.clone(),
                });
                return;
            }
            Some(b"PS") => return,
            _ => {
                self.diagnostics
                    .push(DiagnosticKind::MissingResource, offset);
                return;
            }
        }
        if self.forms.contains(&id) {
            self.diagnostics.push(DiagnosticKind::XObjectCycle, offset);
            return;
        }
        if self.forms.len() >= MAX_NESTED_FORMS {
            self.diagnostics
                .push(DiagnosticKind::XObjectTooDeep, offset);
            return;
        }
        let form_offset = self.resolver.object_offset(id.number);
        let matrix = match dictionary.get("Matrix") {
            None => Matrix::IDENTITY,
            Some(entry) => {
                let entry = self.resolved(entry);
                match entry.as_array().and_then(Matrix::from_objects) {
                    Some(matrix) => matrix,
                    None => {
                        self.diagnostics
                            .push(DiagnosticKind::InvalidForm, form_offset);
                        Matrix::IDENTITY
                    }
                }
            }
        };
        let own_resources = match dictionary.get("Resources") {
            None => None,
            Some(_) => {
                let read = Resources::of(self.resolver, &object);
                if read.is_none() {
                    self.diagnostics
                        .push(DiagnosticKind::InvalidForm, form_offset);
                }
                read
            }
        };
        let data = match self.resolver.stream_data(id) {
            Ok(decoded) => {
                self.diagnostics.append(&decoded.diagnostics);
                decoded.data
            }
            Err(error) => {
                let kind = if error.kind() == ErrorKind::PasswordRequired {
                    DiagnosticKind::LockedContent
                } else {
                    DiagnosticKind::InvalidForm
                };
                self.diagnostics.push(kind, form_offset);
                return;
            }
        };

        // A transparency group painted with opacity, a blend mode or a soft
        // mask is painted on its own and put on the page as a whole.
        let is_group = dictionary
            .get("Group")
            .map(|entry| self.resolved(entry))
            .and_then(|group| {
                let subtype = group.as_dictionary()?.get("S")?.as_name()?.clone();
                Some(subtype.as_bytes() == b"Transparency")
            })
            .unwrap_or(false);
        let outer = self.stack.current().paint.clone();
        let grouped = self.painting
            && is_group
            && (outer.fill_alpha < 1.0
                || outer.blend != BlendMode::Normal
                || outer.soft_mask.is_some());
        if grouped {
            self.order.push(PaintItem::BeginGroup(self.groups.len()));
            self.groups.push(GroupPaint { paint: outer });
        }
        // States the form leaves saved are dropped with it.
        let depth = self.stack.depth();
        if !self.stack.save() {
            self.diagnostics
                .push(DiagnosticKind::TooManySavedStates, offset);
        }
        let text = self.text;
        let in_text_object = self.in_text_object;
        self.stack.transform(&matrix);
        if grouped {
            let paint = &mut self.stack.current_mut().paint;
            paint.fill_alpha = 1.0;
            paint.stroke_alpha = 1.0;
            paint.blend = BlendMode::Normal;
            paint.soft_mask = None;
            paint.clip = None;
        }
        self.forms.push(id);
        self.bases.push(self.stack.current().ctm);
        let form_resources = own_resources.as_ref().unwrap_or(resources);
        self.execute(&data, &[(0, form_offset)], form_resources);
        self.bases.pop();
        self.forms.pop();
        while self.stack.depth() > depth {
            self.stack.restore();
        }
        self.text = text;
        self.in_text_object = in_text_object;
        if grouped {
            self.order.push(PaintItem::EndGroup);
        }
    }

    fn begin_marked_content(&mut self, tag: Name, properties: Option<Dictionary>) {
        let mcid = properties
            .as_ref()
            .and_then(|p| p.get("MCID"))
            .map(|entry| self.resolved(entry))
            .and_then(|entry| entry.as_integer());
        let actual_text = properties
            .as_ref()
            .and_then(|p| p.get("ActualText"))
            .map(|entry| self.resolved(entry))
            .and_then(|entry| entry.as_string().map(|s| decode_text_string(s.as_bytes())));
        self.marked_content.push(MarkedContent {
            tag,
            properties,
            mcid,
            actual_text,
            parent: self.open_marked.last().copied(),
        });
        self.open_marked.push(self.marked_content.len() - 1);
    }

    // `BDC`: a tag and a property list, given in the content or named in the
    // resources' `/Properties`. A sequence whose property list can't be found
    // is still a sequence, without properties.
    fn begin_marked_content_with_properties(
        &mut self,
        operands: &[Object],
        offset: usize,
        resources: &Resources,
    ) {
        let Some((tag, properties)) =
            last(operands, 2).and_then(|o| Some((o[0].as_name()?.clone(), o[1].clone())))
        else {
            self.invalid(offset);
            // Still a sequence, so its `EMC` has something to close.
            let tag = operands
                .iter()
                .find_map(Object::as_name)
                .cloned()
                .unwrap_or_else(|| Name::new(""));
            self.begin_marked_content(tag, None);
            return;
        };
        let properties = match properties {
            Object::Dictionary(dictionary) => Some(dictionary),
            Object::Name(name) => {
                let entry = self.resource(resources, "Properties", name.as_bytes());
                let found = entry.and_then(|entry| self.dictionary(&entry));
                if found.is_none() {
                    self.diagnostics
                        .push(DiagnosticKind::MissingResource, offset);
                }
                found
            }
            _ => {
                self.invalid(offset);
                None
            }
        };
        self.begin_marked_content(tag, properties);
    }

    fn resolved(&mut self, object: &Object) -> Arc<Object> {
        match object {
            Object::Reference(id) => self
                .resolver
                .resolve(*id)
                .unwrap_or_else(|_| Arc::new(Object::Null)),
            other => Arc::new(other.clone()),
        }
    }

    fn dictionary(&mut self, object: &Object) -> Option<Dictionary> {
        self.resolved(object).as_dictionary().cloned()
    }

    // The entry `name` of the resource category `category`, as it is: often a
    // reference.
    fn resource(&mut self, resources: &Resources, category: &str, name: &[u8]) -> Option<Object> {
        let category = resources.category(category)?;
        let category = self.resolved(category);
        category.as_dictionary()?.get(name).cloned()
    }

    // `Tj`: shows the glyphs of `string` and moves the text position past them
    // (section 9.4.4).
    fn show(&mut self, string: &PdfString, offset: usize, resources: &Resources) {
        self.text_position(offset);
        let state = &self.stack.current().text;
        let Some(font) = state.font.clone() else {
            self.diagnostics.push(DiagnosticKind::MissingFont, offset);
            return;
        };
        let size = state.font_size;
        let scaling = state.horizontal_scaling;
        let character_spacing = state.character_spacing;
        let word_spacing = state.word_spacing;
        let rendering_mode = state.rendering_mode;
        let mut rest = string.as_bytes();
        while let Some((code, length)) = font.next_code(rest) {
            rest = &rest[length..];
            if !self.count(offset) {
                return;
            }
            let state = &self.stack.current().text;
            let mut matrix = self.text.rendering_matrix(state, &self.stack.current().ctm);
            let width = font.width(code);
            let vertical = font.vertical_metrics(code);
            // In vertical writing the text position is the glyph's vertical
            // origin, so the glyph starts at the position less `v`.
            if let Some(v) = &vertical {
                matrix = Matrix::translation(-v.origin_x, -v.origin_y).then(&matrix);
            }
            // The glyphs of a pattern aren't text of the page.
            if self.in_pattern == 0 {
                if self.painting {
                    self.order.push(PaintItem::Glyph(self.glyphs.len()));
                }
                let paint = self.stack.current().paint.clone();
                self.glyphs.push(Glyph {
                    code,
                    text: font.text(code).map(|text| text.into_owned()),
                    font: font.clone(),
                    font_size: size,
                    matrix,
                    width,
                    vertical,
                    rendering_mode,
                    marked_content: self.open_marked.last().copied(),
                    paint,
                });
            }
            if self.painting && font.is_type3() {
                self.run_type3_glyph(&font, code, &matrix, offset, resources);
            }
            // Word spacing applies to the byte 32 of a single-byte code.
            let word = if code.length == 1 && code.value == 32 {
                word_spacing
            } else {
                0.0
            };
            match vertical {
                Some(v) => self
                    .text
                    .advance(0.0, v.width * size + character_spacing + word),
                None => {
                    let advance = (width * size + character_spacing + word) * scaling;
                    self.text.advance(advance, 0.0);
                }
            }
        }
    }

    // The glyph of a Type 3 font: its content stream in `/CharProcs`, run with
    // the font matrix and the glyph's text matrix as the current matrix
    // (section 9.6.4). A glyph that starts with `d1` has no colors of its own
    // and is painted with the fill color of the text.
    fn run_type3_glyph(
        &mut self,
        font: &Font,
        code: Code,
        matrix: &Matrix,
        offset: usize,
        resources: &Resources,
    ) {
        let (Some(dictionary), Font::Simple(simple)) = (font.type3_dictionary(), font) else {
            return;
        };
        if self.type3_depth >= 4
            || !matches!(
                self.stack.current().text.rendering_mode,
                TextRenderingMode::Fill
                    | TextRenderingMode::FillStroke
                    | TextRenderingMode::FillClip
                    | TextRenderingMode::FillStrokeClip
                    | TextRenderingMode::Stroke
                    | TextRenderingMode::StrokeClip
            )
        {
            return;
        }
        let Some(name) = u8::try_from(code.value)
            .ok()
            .and_then(|byte| simple.glyph_name(byte))
            .map(<[u8]>::to_vec)
        else {
            return;
        };
        let procs = dictionary
            .get("CharProcs")
            .map(|entry| self.resolved(entry))
            .and_then(|entry| entry.as_dictionary().cloned());
        let Some(Object::Reference(id)) = procs.and_then(|p| p.get(name.as_slice()).cloned())
        else {
            return;
        };
        let Ok(decoded) = self.resolver.stream_data(id) else {
            return;
        };
        let own_resources = dictionary.get("Resources").and_then(|_| {
            let holder = Arc::new(Object::Dictionary((**dictionary).clone()));
            Resources::of(self.resolver, &holder)
        });
        let font_matrix = simple.font_matrix();
        let glyph_matrix = Matrix::new(
            font_matrix[0],
            font_matrix[1],
            font_matrix[2],
            font_matrix[3],
            font_matrix[4],
            font_matrix[5],
        )
        .then(matrix);
        if !glyph_matrix.is_finite() {
            return;
        }
        let depth = self.stack.depth();
        let text = self.text;
        let in_text_object = self.in_text_object;
        let (path, current, subpath) = (
            std::mem::take(&mut self.path),
            self.current.take(),
            self.subpath,
        );
        if !self.stack.save() {
            self.diagnostics
                .push(DiagnosticKind::TooManySavedStates, offset);
        }
        let state = self.stack.current_mut();
        state.ctm = glyph_matrix;
        // The line of a glyph is its own, but its colors are the text's.
        state.paint.stroke_color = state.paint.fill_color;
        state.paint.line = Arc::new(Default::default());
        let uncolored = self.uncolored;
        self.uncolored = false;
        self.type3_depth += 1;
        let glyph_resources = own_resources.as_ref().unwrap_or(resources);
        self.execute(&decoded.data, &[(0, offset)], glyph_resources);
        self.type3_depth -= 1;
        self.uncolored = uncolored;
        while self.stack.depth() > depth {
            self.stack.restore();
        }
        self.text = text;
        self.in_text_object = in_text_object;
        self.path = path;
        self.current = current;
        self.subpath = subpath;
    }

    // `TJ`: strings and adjustments, in thousandths of the font size, that
    // move the next glyph back.
    fn show_adjusted(&mut self, items: &[Object], offset: usize, resources: &Resources) {
        for item in items {
            match item {
                Object::String(string) => self.show(string, offset, resources),
                Object::Integer(_) | Object::Real(_) => {
                    let Some(adjustment) = item.as_number().filter(|value| value.is_finite())
                    else {
                        continue;
                    };
                    self.text_position(offset);
                    let state = &self.stack.current().text;
                    let shift = -adjustment / 1000.0 * state.font_size;
                    if state.font.as_ref().is_some_and(|font| font.is_vertical()) {
                        self.text.advance(0.0, shift);
                    } else {
                        self.text.advance(shift * state.horizontal_scaling, 0.0);
                    }
                }
                _ => self.invalid(offset),
            }
            if self.stopped {
                return;
            }
        }
    }
}

// The color of operands in no color space: gray, RGB or CMYK by their count.
fn device_color(values: &[f64]) -> [u8; 3] {
    match values.len() {
        0 => [0; 3],
        1 | 2 => ColorSpace::Gray.rgb(values),
        3 => ColorSpace::Rgb.rgb(values),
        n => ColorSpace::Cmyk.rgb(&values[n - 4..]),
    }
}

// The last `count` operands. Extra operands before them, left by damaged
// content, are ignored, as readers do.
fn last(operands: &[Object], count: usize) -> Option<&[Object]> {
    operands
        .len()
        .checked_sub(count)
        .map(|start| &operands[start..])
}

// The offset in the file of the piece of content that holds `offset`.
fn file_offset(pieces: &[(usize, usize)], offset: usize) -> usize {
    let index = pieces.partition_point(|(start, _)| *start <= offset);
    index
        .checked_sub(1)
        .map_or(pieces.first().map_or(0, |(_, file)| *file), |i| pieces[i].1)
}
