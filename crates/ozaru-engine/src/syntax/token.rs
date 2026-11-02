use super::{Name, PdfString};

/// A piece of PDF syntax, with the range of bytes it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Offset of the first byte of the token.
    pub start: usize,
    /// Offset just past the last byte of the token.
    pub end: usize,
}

/// The kinds of token the lexer produces.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Integer(i64),
    Real(f64),
    String(PdfString),
    Name(Name),
    /// A run of regular characters that isn't a number, such as `true`, `null`,
    /// `R`, `obj` or a content stream operator like `Tj`. The parser decides what
    /// it means.
    Keyword(Vec<u8>),
    /// `[`
    ArrayStart,
    /// `]`
    ArrayEnd,
    /// `<<`
    DictionaryStart,
    /// `>>`
    DictionaryEnd,
    /// `{`, used by PostScript calculator functions.
    ProcedureStart,
    /// `}`
    ProcedureEnd,
}
