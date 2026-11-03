use std::fmt;

use super::Limit;

/// The result of an engine operation that can fail.
pub type Result<T> = std::result::Result<T, Error>;

/// An error that stops an operation, with the byte offset where it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    offset: usize,
}

impl Error {
    pub fn new(kind: ErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Position in the input, in bytes from the start, where the error was found.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind, self.offset)
    }
}

impl std::error::Error for Error {}

/// What went wrong. New kinds are added as the engine grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The input ended in the middle of an object.
    UnexpectedEnd,
    /// A byte that can't start a token here, such as a lone `)` or `>`.
    UnexpectedByte(u8),
    /// Text that starts like a number but isn't one, such as `1.2.3` or `1e5`.
    InvalidNumber,
    /// An integer too large to store.
    NumberOutOfRange,
    /// A character that isn't a hexadecimal digit inside `<...>`.
    InvalidHexString,
    /// A token that can't appear here, such as an unknown keyword or a `]` with no
    /// matching `[`.
    UnexpectedToken,
    /// A dictionary key that isn't a name.
    InvalidDictionaryKey,
    /// A dictionary key with no value before `>>`.
    MissingDictionaryValue,
    /// A reference whose object number or generation is out of range.
    InvalidReference,
    /// An indirect object that doesn't start with `n g obj`.
    InvalidObjectHeader,
    /// A stream with no `endstream` after its data.
    MissingEndstream,
    /// No `startxref` keyword near the end of the file.
    MissingStartxref,
    /// The value after `startxref` isn't an offset inside the file.
    InvalidStartxref,
    /// There is no `xref` keyword where the cross-reference table should start.
    NotAnXrefTable,
    /// A cross-reference subsection header isn't two integers, `first count`, or
    /// numbers objects past the largest object number.
    InvalidXrefSubsection,
    /// A cross-reference entry isn't `offset generation n` or `... f`.
    InvalidXrefEntry,
    /// A cross-reference table isn't followed by `trailer`.
    MissingTrailer,
    /// The `trailer` keyword isn't followed by a dictionary.
    InvalidTrailer,
    /// The input is larger or deeper than a configured limit allows.
    LimitExceeded(Limit),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => f.write_str("unexpected end of input"),
            Self::UnexpectedByte(byte) => write!(f, "unexpected byte 0x{byte:02X}"),
            Self::InvalidNumber => f.write_str("invalid number"),
            Self::NumberOutOfRange => f.write_str("number out of range"),
            Self::InvalidHexString => f.write_str("invalid hexadecimal string"),
            Self::UnexpectedToken => f.write_str("unexpected token"),
            Self::InvalidDictionaryKey => f.write_str("dictionary key is not a name"),
            Self::MissingDictionaryValue => f.write_str("dictionary key has no value"),
            Self::InvalidReference => f.write_str("invalid reference"),
            Self::InvalidObjectHeader => f.write_str("invalid object header"),
            Self::MissingEndstream => f.write_str("missing endstream"),
            Self::MissingStartxref => f.write_str("missing startxref"),
            Self::InvalidStartxref => f.write_str("invalid startxref offset"),
            Self::NotAnXrefTable => f.write_str("not a cross-reference table"),
            Self::InvalidXrefSubsection => f.write_str("invalid cross-reference subsection"),
            Self::InvalidXrefEntry => f.write_str("invalid cross-reference entry"),
            Self::MissingTrailer => f.write_str("missing trailer"),
            Self::InvalidTrailer => f.write_str("trailer is not a dictionary"),
            Self::LimitExceeded(limit) => write!(f, "{limit} limit exceeded"),
        }
    }
}
