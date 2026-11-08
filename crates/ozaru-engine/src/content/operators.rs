use std::ops::Range;

use crate::base::{DiagnosticKind, Diagnostics, Limits};
use crate::syntax::chars::{is_delimiter, is_whitespace};
use crate::syntax::{Dictionary, Lexer, Name, Object, Token, TokenKind};

/// A content stream operator (ISO 32000-2, Annex A).
///
/// Each one is named after what it does; [`keyword`](Self::keyword) gives the
/// operator as it is written in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator<'a> {
    // General graphics state (section 8.4.4).
    /// `w`
    SetLineWidth,
    /// `J`
    SetLineCap,
    /// `j`
    SetLineJoin,
    /// `M`
    SetMiterLimit,
    /// `d`
    SetDashPattern,
    /// `ri`
    SetRenderingIntent,
    /// `i`
    SetFlatness,
    /// `gs`
    SetExtGState,
    // Special graphics state (section 8.4.4).
    /// `q`
    Save,
    /// `Q`
    Restore,
    /// `cm`
    Transform,
    // Path construction (section 8.5.2).
    /// `m`
    MoveTo,
    /// `l`
    LineTo,
    /// `c`
    CurveTo,
    /// `v`
    CurveToFromCurrent,
    /// `y`
    CurveToEnd,
    /// `h`
    ClosePath,
    /// `re`
    Rectangle,
    // Path painting (section 8.5.3).
    /// `S`
    Stroke,
    /// `s`
    CloseAndStroke,
    /// `f`
    Fill,
    /// `F`, an old spelling of `f`.
    FillObsolete,
    /// `f*`
    FillEvenOdd,
    /// `B`
    FillAndStroke,
    /// `B*`
    FillEvenOddAndStroke,
    /// `b`
    CloseFillAndStroke,
    /// `b*`
    CloseFillEvenOddAndStroke,
    /// `n`
    EndPath,
    // Clipping paths (section 8.5.4).
    /// `W`
    Clip,
    /// `W*`
    ClipEvenOdd,
    // Text objects (section 9.4).
    /// `BT`
    BeginText,
    /// `ET`
    EndText,
    // Text state (section 9.3).
    /// `Tc`
    SetCharacterSpacing,
    /// `Tw`
    SetWordSpacing,
    /// `Tz`
    SetHorizontalScaling,
    /// `TL`
    SetLeading,
    /// `Tf`
    SetFont,
    /// `Tr`
    SetTextRenderingMode,
    /// `Ts`
    SetTextRise,
    // Text positioning (section 9.4.2).
    /// `Td`
    MoveText,
    /// `TD`
    MoveTextSetLeading,
    /// `Tm`
    SetTextMatrix,
    /// `T*`
    NextLine,
    // Text showing (section 9.4.3).
    /// `Tj`
    ShowText,
    /// `TJ`
    ShowTextAdjusted,
    /// `'`
    NextLineShowText,
    /// `"`
    NextLineShowTextSpaced,
    // Type 3 fonts (section 9.6.4).
    /// `d0`
    SetGlyphWidth,
    /// `d1`
    SetGlyphWidthAndBox,
    // Colour (section 8.6.8).
    /// `CS`
    SetStrokeColorSpace,
    /// `cs`
    SetFillColorSpace,
    /// `SC`
    SetStrokeColor,
    /// `SCN`
    SetStrokeColorExtended,
    /// `sc`
    SetFillColor,
    /// `scn`
    SetFillColorExtended,
    /// `G`
    SetStrokeGray,
    /// `g`
    SetFillGray,
    /// `RG`
    SetStrokeRgb,
    /// `rg`
    SetFillRgb,
    /// `K`
    SetStrokeCmyk,
    /// `k`
    SetFillCmyk,
    // Shading (section 8.7.4.2).
    /// `sh`
    PaintShading,
    // Inline images (section 8.9.7).
    /// `BI` ... `ID` ... `EI`, read as one operation. Its dictionary and data
    /// are in [`Operation::inline_image`].
    InlineImage,
    // XObjects (section 8.8).
    /// `Do`
    PaintXObject,
    // Marked content (section 14.6).
    /// `MP`
    MarkPoint,
    /// `DP`
    MarkPointWithProperties,
    /// `BMC`
    BeginMarkedContent,
    /// `BDC`
    BeginMarkedContentWithProperties,
    /// `EMC`
    EndMarkedContent,
    // Compatibility (section 7.8.2).
    /// `BX`
    BeginCompatibility,
    /// `EX`
    EndCompatibility,
    /// An operator that isn't in the standard, as it is written.
    Unknown(&'a [u8]),
}

impl<'a> Operator<'a> {
    /// The operator written as `keyword`.
    pub fn from_keyword(keyword: &'a [u8]) -> Self {
        match keyword {
            b"w" => Self::SetLineWidth,
            b"J" => Self::SetLineCap,
            b"j" => Self::SetLineJoin,
            b"M" => Self::SetMiterLimit,
            b"d" => Self::SetDashPattern,
            b"ri" => Self::SetRenderingIntent,
            b"i" => Self::SetFlatness,
            b"gs" => Self::SetExtGState,
            b"q" => Self::Save,
            b"Q" => Self::Restore,
            b"cm" => Self::Transform,
            b"m" => Self::MoveTo,
            b"l" => Self::LineTo,
            b"c" => Self::CurveTo,
            b"v" => Self::CurveToFromCurrent,
            b"y" => Self::CurveToEnd,
            b"h" => Self::ClosePath,
            b"re" => Self::Rectangle,
            b"S" => Self::Stroke,
            b"s" => Self::CloseAndStroke,
            b"f" => Self::Fill,
            b"F" => Self::FillObsolete,
            b"f*" => Self::FillEvenOdd,
            b"B" => Self::FillAndStroke,
            b"B*" => Self::FillEvenOddAndStroke,
            b"b" => Self::CloseFillAndStroke,
            b"b*" => Self::CloseFillEvenOddAndStroke,
            b"n" => Self::EndPath,
            b"W" => Self::Clip,
            b"W*" => Self::ClipEvenOdd,
            b"BT" => Self::BeginText,
            b"ET" => Self::EndText,
            b"Tc" => Self::SetCharacterSpacing,
            b"Tw" => Self::SetWordSpacing,
            b"Tz" => Self::SetHorizontalScaling,
            b"TL" => Self::SetLeading,
            b"Tf" => Self::SetFont,
            b"Tr" => Self::SetTextRenderingMode,
            b"Ts" => Self::SetTextRise,
            b"Td" => Self::MoveText,
            b"TD" => Self::MoveTextSetLeading,
            b"Tm" => Self::SetTextMatrix,
            b"T*" => Self::NextLine,
            b"Tj" => Self::ShowText,
            b"TJ" => Self::ShowTextAdjusted,
            b"'" => Self::NextLineShowText,
            b"\"" => Self::NextLineShowTextSpaced,
            b"d0" => Self::SetGlyphWidth,
            b"d1" => Self::SetGlyphWidthAndBox,
            b"CS" => Self::SetStrokeColorSpace,
            b"cs" => Self::SetFillColorSpace,
            b"SC" => Self::SetStrokeColor,
            b"SCN" => Self::SetStrokeColorExtended,
            b"sc" => Self::SetFillColor,
            b"scn" => Self::SetFillColorExtended,
            b"G" => Self::SetStrokeGray,
            b"g" => Self::SetFillGray,
            b"RG" => Self::SetStrokeRgb,
            b"rg" => Self::SetFillRgb,
            b"K" => Self::SetStrokeCmyk,
            b"k" => Self::SetFillCmyk,
            b"sh" => Self::PaintShading,
            b"BI" => Self::InlineImage,
            b"Do" => Self::PaintXObject,
            b"MP" => Self::MarkPoint,
            b"DP" => Self::MarkPointWithProperties,
            b"BMC" => Self::BeginMarkedContent,
            b"BDC" => Self::BeginMarkedContentWithProperties,
            b"EMC" => Self::EndMarkedContent,
            b"BX" => Self::BeginCompatibility,
            b"EX" => Self::EndCompatibility,
            other => Self::Unknown(other),
        }
    }

    /// The operator as it is written in a content stream. `BI` for an inline
    /// image.
    pub fn keyword(&self) -> &'a [u8] {
        match self {
            Self::SetLineWidth => b"w",
            Self::SetLineCap => b"J",
            Self::SetLineJoin => b"j",
            Self::SetMiterLimit => b"M",
            Self::SetDashPattern => b"d",
            Self::SetRenderingIntent => b"ri",
            Self::SetFlatness => b"i",
            Self::SetExtGState => b"gs",
            Self::Save => b"q",
            Self::Restore => b"Q",
            Self::Transform => b"cm",
            Self::MoveTo => b"m",
            Self::LineTo => b"l",
            Self::CurveTo => b"c",
            Self::CurveToFromCurrent => b"v",
            Self::CurveToEnd => b"y",
            Self::ClosePath => b"h",
            Self::Rectangle => b"re",
            Self::Stroke => b"S",
            Self::CloseAndStroke => b"s",
            Self::Fill => b"f",
            Self::FillObsolete => b"F",
            Self::FillEvenOdd => b"f*",
            Self::FillAndStroke => b"B",
            Self::FillEvenOddAndStroke => b"B*",
            Self::CloseFillAndStroke => b"b",
            Self::CloseFillEvenOddAndStroke => b"b*",
            Self::EndPath => b"n",
            Self::Clip => b"W",
            Self::ClipEvenOdd => b"W*",
            Self::BeginText => b"BT",
            Self::EndText => b"ET",
            Self::SetCharacterSpacing => b"Tc",
            Self::SetWordSpacing => b"Tw",
            Self::SetHorizontalScaling => b"Tz",
            Self::SetLeading => b"TL",
            Self::SetFont => b"Tf",
            Self::SetTextRenderingMode => b"Tr",
            Self::SetTextRise => b"Ts",
            Self::MoveText => b"Td",
            Self::MoveTextSetLeading => b"TD",
            Self::SetTextMatrix => b"Tm",
            Self::NextLine => b"T*",
            Self::ShowText => b"Tj",
            Self::ShowTextAdjusted => b"TJ",
            Self::NextLineShowText => b"'",
            Self::NextLineShowTextSpaced => b"\"",
            Self::SetGlyphWidth => b"d0",
            Self::SetGlyphWidthAndBox => b"d1",
            Self::SetStrokeColorSpace => b"CS",
            Self::SetFillColorSpace => b"cs",
            Self::SetStrokeColor => b"SC",
            Self::SetStrokeColorExtended => b"SCN",
            Self::SetFillColor => b"sc",
            Self::SetFillColorExtended => b"scn",
            Self::SetStrokeGray => b"G",
            Self::SetFillGray => b"g",
            Self::SetStrokeRgb => b"RG",
            Self::SetFillRgb => b"rg",
            Self::SetStrokeCmyk => b"K",
            Self::SetFillCmyk => b"k",
            Self::PaintShading => b"sh",
            Self::InlineImage => b"BI",
            Self::PaintXObject => b"Do",
            Self::MarkPoint => b"MP",
            Self::MarkPointWithProperties => b"DP",
            Self::BeginMarkedContent => b"BMC",
            Self::BeginMarkedContentWithProperties => b"BDC",
            Self::EndMarkedContent => b"EMC",
            Self::BeginCompatibility => b"BX",
            Self::EndCompatibility => b"EX",
            Self::Unknown(keyword) => keyword,
        }
    }
}

/// One operation of a content stream: an operator and the operands written
/// before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Operation<'p> {
    pub operator: Operator<'p>,
    pub operands: &'p [Object],
    /// Offset of the operator in the content stream.
    pub offset: usize,
    /// The dictionary and data of an inline image, for
    /// [`Operator::InlineImage`].
    pub inline_image: Option<InlineImage<'p>>,
}

/// An inline image: the entries between `BI` and `ID`, kept as they are
/// written, often abbreviated, like `/W` for `/Width`, and the bytes between
/// `ID` and `EI` (ISO 32000-2, section 8.9.7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineImage<'p> {
    pub dictionary: &'p Dictionary,
    pub data: &'p [u8],
}

/// Reads the operations of a content stream, one at a time (ISO 32000-2,
/// section 7.8.2).
///
/// It never fails: syntax it can't read is skipped with a `DamagedContent`
/// diagnostic, at the offset in the content stream where it was found, and
/// reading goes on from the next token. Operands left without an operator at
/// the end are dropped with a `MissingOperator` diagnostic.
pub struct ContentParser<'a> {
    input: &'a [u8],
    lexer: Lexer<'a>,
    limits: Limits,
    operands: Vec<Object>,
    image_dictionary: Dictionary,
    image_data: Range<usize>,
    diagnostics: Diagnostics,
}

// An array or dictionary being read.
enum Container {
    Array(Vec<Object>),
    Dictionary {
        dictionary: Dictionary,
        key: Option<Name>,
    },
}

// What reading an operand found.
enum Read<'a> {
    Object(Object),
    Operator(Token<'a>),
    // The object was damaged and skipped, with a diagnostic.
    Skipped,
    End,
}

impl<'a> ContentParser<'a> {
    pub fn new(input: &'a [u8], limits: Limits) -> Self {
        Self {
            input,
            lexer: Lexer::new(input, limits),
            limits,
            operands: Vec::new(),
            image_dictionary: Dictionary::new(),
            image_data: 0..0,
            diagnostics: Diagnostics::new(limits.max_diagnostics),
        }
    }

    /// Problems found and worked around so far, with offsets in the content
    /// stream.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// Returns the diagnostics collected so far and starts a new, empty list.
    pub fn take_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(
            &mut self.diagnostics,
            Diagnostics::new(self.limits.max_diagnostics),
        )
    }

    /// The next operation, or `None` at the end of the content stream.
    pub fn next_operation(&mut self) -> Option<Operation<'_>> {
        self.operands.clear();
        let mut too_many = false;
        loop {
            let token = match self.read() {
                Read::Object(object) => {
                    if self.operands.len() < self.limits.max_array_length {
                        self.operands.push(object);
                    } else if !too_many {
                        too_many = true;
                        let offset = self.lexer.position();
                        self.diagnostics
                            .push(DiagnosticKind::DamagedContent, offset);
                    }
                    continue;
                }
                Read::Skipped => continue,
                Read::End => {
                    if !self.operands.is_empty() {
                        self.diagnostics
                            .push(DiagnosticKind::MissingOperator, self.input.len());
                        self.operands.clear();
                    }
                    return None;
                }
                Read::Operator(token) => token,
            };
            let TokenKind::Keyword(keyword) = token.kind else {
                unreachable!("only keywords are operators");
            };
            let operator = Operator::from_keyword(keyword);
            if operator == Operator::InlineImage {
                self.operands.clear();
                self.inline_image(token.start);
                return Some(Operation {
                    operator,
                    operands: &[],
                    offset: token.start,
                    inline_image: Some(InlineImage {
                        dictionary: &self.image_dictionary,
                        data: &self.input[self.image_data.clone()],
                    }),
                });
            }
            return Some(Operation {
                operator,
                operands: &self.operands,
                offset: token.start,
                inline_image: None,
            });
        }
    }

    // The next token, skipping what can't be read. `None` at the end.
    fn token(&mut self) -> Option<Token<'a>> {
        loop {
            let before = self.lexer.position();
            match self.lexer.next_token() {
                Ok(token) => return token,
                Err(error) => {
                    self.diagnostics
                        .push(DiagnosticKind::DamagedContent, error.offset());
                    // Go on just after the byte that failed, always moving on.
                    let resume = error
                        .offset()
                        .max(before)
                        .saturating_add(1)
                        .min(self.input.len());
                    self.lexer = Lexer::starting_at(self.input, resume, self.limits);
                }
            }
        }
    }

    // Goes back to `token`, so it is read again.
    fn unread(&mut self, token: &Token<'a>) {
        self.lexer = Lexer::starting_at(self.input, token.start, self.limits);
    }

    // Reads an operand, a whole array or dictionary at once, or an operator.
    fn read(&mut self) -> Read<'a> {
        let Some(token) = self.token() else {
            return Read::End;
        };
        let start = token.start;
        let container = match token.kind {
            TokenKind::Keyword(keyword) => {
                return match literal(keyword) {
                    Some(object) => Read::Object(object),
                    None => Read::Operator(token),
                };
            }
            TokenKind::Integer(value) => return Read::Object(Object::Integer(value)),
            TokenKind::Real(value) => return Read::Object(Object::Real(value)),
            TokenKind::String(value) => return Read::Object(Object::String(value)),
            TokenKind::Name(value) => return Read::Object(Object::Name(value)),
            TokenKind::ArrayStart => Container::Array(Vec::new()),
            TokenKind::DictionaryStart => Container::Dictionary {
                dictionary: Dictionary::new(),
                key: None,
            },
            TokenKind::ArrayEnd
            | TokenKind::DictionaryEnd
            | TokenKind::ProcedureStart
            | TokenKind::ProcedureEnd => {
                self.diagnostics.push(DiagnosticKind::DamagedContent, start);
                return Read::Skipped;
            }
        };
        self.container(container, start)
    }

    // Reads the rest of an array or dictionary that started at `start`, with
    // its own stack, so nesting can't overflow the call stack.
    fn container(&mut self, first: Container, start: usize) -> Read<'a> {
        let mut open = vec![first];
        loop {
            let Some(token) = self.token() else {
                // Cut short by the end of the content.
                self.diagnostics
                    .push(DiagnosticKind::DamagedContent, self.input.len());
                return Read::End;
            };
            let value = match token.kind {
                TokenKind::Keyword(keyword) => match literal(keyword) {
                    Some(object) => object,
                    None => {
                        // An operator inside an array or dictionary: the object
                        // is broken, and the operator is read again on its own.
                        self.diagnostics.push(DiagnosticKind::DamagedContent, start);
                        self.unread(&token);
                        return Read::Skipped;
                    }
                },
                TokenKind::Integer(value) => Object::Integer(value),
                TokenKind::Real(value) => Object::Real(value),
                TokenKind::String(value) => Object::String(value),
                TokenKind::Name(value) => Object::Name(value),
                TokenKind::ArrayStart | TokenKind::DictionaryStart => {
                    if open.len() >= self.limits.depth() {
                        self.diagnostics
                            .push(DiagnosticKind::DamagedContent, token.start);
                        self.skip_to_operator();
                        return Read::Skipped;
                    }
                    open.push(if matches!(token.kind, TokenKind::ArrayStart) {
                        Container::Array(Vec::new())
                    } else {
                        Container::Dictionary {
                            dictionary: Dictionary::new(),
                            key: None,
                        }
                    });
                    continue;
                }
                TokenKind::ArrayEnd => match open.pop() {
                    Some(Container::Array(items)) => Object::Array(items),
                    Some(other) => {
                        // `]` closing a dictionary: ignored.
                        self.diagnostics
                            .push(DiagnosticKind::DamagedContent, token.start);
                        open.push(other);
                        continue;
                    }
                    None => unreachable!("the stack is never empty here"),
                },
                TokenKind::DictionaryEnd => match open.pop() {
                    Some(Container::Dictionary { dictionary, key }) => {
                        if key.is_some() {
                            self.diagnostics
                                .push(DiagnosticKind::DamagedContent, token.start);
                        }
                        Object::Dictionary(dictionary)
                    }
                    Some(other) => {
                        self.diagnostics
                            .push(DiagnosticKind::DamagedContent, token.start);
                        open.push(other);
                        continue;
                    }
                    None => unreachable!("the stack is never empty here"),
                },
                TokenKind::ProcedureStart | TokenKind::ProcedureEnd => {
                    self.diagnostics
                        .push(DiagnosticKind::DamagedContent, token.start);
                    continue;
                }
            };
            let Some(parent) = open.last_mut() else {
                return Read::Object(value);
            };
            match parent {
                Container::Array(items) => {
                    if items.len() < self.limits.max_array_length {
                        items.push(value);
                    }
                }
                Container::Dictionary { dictionary, key } => match key.take() {
                    None => match value {
                        Object::Name(name) => *key = Some(name),
                        _ => self
                            .diagnostics
                            .push(DiagnosticKind::DamagedContent, token.start),
                    },
                    Some(name) => {
                        if dictionary.len() < self.limits.max_dictionary_length
                            || dictionary.contains_key(name.as_bytes())
                        {
                            dictionary.insert(name, value);
                        }
                    }
                },
            }
        }
    }

    // After an object nested too deeply: skips to the next operator, which is
    // read again on its own.
    fn skip_to_operator(&mut self) {
        while let Some(token) = self.token() {
            if let TokenKind::Keyword(keyword) = token.kind
                && literal(keyword).is_none()
            {
                self.unread(&token);
                return;
            }
        }
    }

    // Reads an inline image after its `BI` at `start`: the entries up to `ID`
    // and the data up to `EI` (section 8.9.7).
    fn inline_image(&mut self, start: usize) {
        self.image_dictionary = Dictionary::new();
        self.image_data = 0..0;
        let mut key: Option<Name> = None;
        let id_end = loop {
            match self.read() {
                Read::End => {
                    self.diagnostics
                        .push(DiagnosticKind::UnterminatedInlineImage, start);
                    return;
                }
                Read::Skipped => {}
                Read::Object(object) => match key.take() {
                    None => match object {
                        Object::Name(name) => key = Some(name),
                        _ => self.diagnostics.push(DiagnosticKind::DamagedContent, start),
                    },
                    Some(name) => {
                        if self.image_dictionary.len() < self.limits.max_dictionary_length {
                            self.image_dictionary.insert(name, object);
                        }
                    }
                },
                Read::Operator(token) => {
                    if matches!(token.kind, TokenKind::Keyword(b"ID")) {
                        break token.end;
                    }
                    // `EI` or another operator before `ID`: an image with no
                    // data, and the operator is read on its own.
                    self.diagnostics
                        .push(DiagnosticKind::UnterminatedInlineImage, start);
                    if !matches!(token.kind, TokenKind::Keyword(b"EI")) {
                        self.unread(&token);
                    }
                    return;
                }
            }
        };
        if key.is_some() {
            self.diagnostics.push(DiagnosticKind::DamagedContent, start);
        }
        // A single whitespace byte separates `ID` from the data.
        let data_start = match self.input.get(id_end) {
            Some(&byte) if is_whitespace(byte) => id_end + 1,
            _ => id_end,
        };
        let (data_end, resume) = match self.declared_length(data_start) {
            Some(found) => found,
            None => match find_end_of_image(self.input, data_start) {
                Some(found) => found,
                None => {
                    self.diagnostics
                        .push(DiagnosticKind::UnterminatedInlineImage, start);
                    (self.input.len(), self.input.len())
                }
            },
        };
        self.image_data = data_start..data_end;
        self.lexer = Lexer::starting_at(self.input, resume, self.limits);
    }

    // PDF 2.0 lets an inline image give the length of its data in `/L` or
    // `/Length`. It is used when `EI` follows the data.
    fn declared_length(&self, data_start: usize) -> Option<(usize, usize)> {
        let length = self
            .image_dictionary
            .get("L")
            .or_else(|| self.image_dictionary.get("Length"))?
            .as_integer()?;
        let data_end = data_start.checked_add(usize::try_from(length).ok()?)?;
        let mut position = data_end;
        while self
            .input
            .get(position)
            .is_some_and(|&byte| is_whitespace(byte))
        {
            position += 1;
        }
        if self.input.get(position..position + 2) == Some(b"EI")
            && ends_token(self.input, position + 2)
        {
            Some((data_end, position + 2))
        } else {
            None
        }
    }
}

// `true`, `false` and `null`, the keywords that are operands.
fn literal(keyword: &[u8]) -> Option<Object> {
    match keyword {
        b"true" => Some(Object::Boolean(true)),
        b"false" => Some(Object::Boolean(false)),
        b"null" => Some(Object::Null),
        _ => None,
    }
}

fn ends_token(input: &[u8], position: usize) -> bool {
    input
        .get(position)
        .is_none_or(|&byte| is_whitespace(byte) || is_delimiter(byte))
}

// Finds the `EI` that ends the data of an inline image starting at `start`: one
// with whitespace before it, that ends a token, and is followed by what looks
// like more content rather than binary data. Returns where the data ends,
// without the whitespace before `EI`, and where reading goes on.
fn find_end_of_image(input: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut position = start;
    while position + 2 <= input.len() {
        let found = memchr_ei(input, position)?;
        position = found + 1;
        let before_is_space = found > start && is_whitespace(input[found - 1]);
        let at_start = found == start;
        if !(before_is_space || at_start) || !ends_token(input, found + 2) {
            continue;
        }
        if !looks_like_content(&input[found + 2..]) {
            continue;
        }
        let data_end = if before_is_space { found - 1 } else { found };
        return Some((data_end, found + 2));
    }
    None
}

fn memchr_ei(input: &[u8], from: usize) -> Option<usize> {
    input
        .get(from..)?
        .windows(2)
        .position(|pair| pair == b"EI")
        .map(|index| from + index)
}

// Whether the bytes after a candidate `EI` look like content stream text: no
// control bytes or bytes above 0x7E among the next ones.
fn looks_like_content(rest: &[u8]) -> bool {
    rest.iter()
        .take(32)
        .all(|&byte| is_whitespace(byte) || (0x20..0x7F).contains(&byte))
}
