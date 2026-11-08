use std::collections::HashMap;
use std::sync::Arc;

use super::graphics_state::{GraphicsStack, GraphicsState, Matrix};
use super::operators::{ContentParser, Operation, Operator};
use super::text_state::{TextObject, TextRenderingMode};
use crate::base::{DiagnosticKind, Diagnostics, ErrorKind, Limits};
use crate::document::{Page, Resources};
use crate::fonts::{Code, Font, VerticalMetrics};
use crate::syntax::{Dictionary, Object, ObjectId, PdfString};
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
}

/// What the content of a page left on it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageContent {
    /// The glyphs, in the order they were painted.
    pub glyphs: Vec<Glyph>,
    /// Problems found and worked around. Those inside a content stream are
    /// reported at the start of its data in the file.
    pub diagnostics: Diagnostics,
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
}

impl Interpreter {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            fonts: HashMap::new(),
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
            Font::read(resolver, dictionary, self.limits, offset, diagnostics).map(Arc::new)
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
    // The forms being painted, the outermost first.
    forms: Vec<ObjectId>,
    glyphs: Vec<Glyph>,
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
            glyphs: Vec::new(),
            diagnostics,
        }
    }

    fn finish(self) -> PageContent {
        PageContent {
            glyphs: self.glyphs,
            diagnostics: self.diagnostics,
        }
    }

    // Runs `data`, whose pieces start at the given offsets in it and in the
    // file.
    fn execute(&mut self, data: &[u8], pieces: &[(usize, usize)], resources: &Resources) {
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
                Some(string) => self.show(string, offset),
                None => self.invalid(offset),
            },
            Operator::ShowTextAdjusted => match last(operands, 1).and_then(|o| o[0].as_array()) {
                Some(items) => self.show_adjusted(items, offset),
                None => self.invalid(offset),
            },
            Operator::NextLineShowText => match last(operands, 1).and_then(|o| o[0].as_string()) {
                Some(string) => {
                    self.text_position(offset);
                    let leading = self.stack.current().text.leading;
                    self.text.next_line(leading);
                    self.show(string, offset);
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
                        self.show(string, offset);
                    }
                    _ => self.invalid(offset),
                }
            }
            Operator::PaintXObject => self.paint_xobject(operands, offset, resources),
            Operator::BeginCompatibility => self.compatibility += 1,
            Operator::EndCompatibility => self.compatibility = self.compatibility.saturating_sub(1),
            Operator::Unknown(_) if self.compatibility == 0 => {
                self.diagnostics
                    .push(DiagnosticKind::UnknownOperator, offset);
            }
            // Paths, colours, inline images, shadings, marked content and
            // Type 3 glyph metrics don't change where text goes.
            _ => {}
        }
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
            Object::Dictionary(dictionary) => Font::read(
                self.resolver,
                dictionary,
                self.limits,
                0,
                &mut self.diagnostics,
            )
            .map(Arc::new),
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

    // `Do`: paints the XObject named in the resources' `/XObject`. A form is
    // run inside a saved graphics state, with its `/Matrix` and its own
    // resources, or those of what paints it when it has none (section 8.10).
    // Images come in phase 10.
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
            Some(b"Image" | b"PS") => return,
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
            Some(entry) => {
                let read = Resources::read(self.resolver, entry);
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

        // States the form leaves saved are dropped with it.
        let depth = self.stack.depth();
        if !self.stack.save() {
            self.diagnostics
                .push(DiagnosticKind::TooManySavedStates, offset);
        }
        let text = self.text;
        let in_text_object = self.in_text_object;
        self.stack.transform(&matrix);
        self.forms.push(id);
        let form_resources = own_resources.as_ref().unwrap_or(resources);
        self.execute(&data, &[(0, form_offset)], form_resources);
        self.forms.pop();
        while self.stack.depth() > depth {
            self.stack.restore();
        }
        self.text = text;
        self.in_text_object = in_text_object;
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
    fn show(&mut self, string: &PdfString, offset: usize) {
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
            self.glyphs.push(Glyph {
                code,
                text: font.text(code).map(|text| text.into_owned()),
                font: font.clone(),
                font_size: size,
                matrix,
                width,
                vertical,
                rendering_mode,
            });
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

    // `TJ`: strings and adjustments, in thousandths of the font size, that
    // move the next glyph back.
    fn show_adjusted(&mut self, items: &[Object], offset: usize) {
        for item in items {
            match item {
                Object::String(string) => self.show(string, offset),
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
