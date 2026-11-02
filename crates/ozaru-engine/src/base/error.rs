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
            Self::LimitExceeded(limit) => write!(f, "{limit} limit exceeded"),
        }
    }
}
