use std::fmt;

/// A recoverable problem: the engine worked around it and kept going.
///
/// Unlike an [`Error`](super::Error), a diagnostic doesn't stop the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    kind: DiagnosticKind,
    offset: usize,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    pub fn kind(&self) -> DiagnosticKind {
        self.kind
    }

    /// Position in the input, in bytes from the start, where the problem was found.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind, self.offset)
    }
}

/// What the engine worked around. New kinds are added as the engine grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticKind {
    /// A dictionary has the same key more than once. The last value is kept.
    DuplicateDictionaryKey,
    /// An indirect object has no `endobj`. The object is read anyway.
    MissingEndobj,
    /// A stream has no `/Length`. Its data ends at `endstream`.
    MissingStreamLength,
    /// A stream's `/Length` isn't a valid integer, or doesn't end where
    /// `endstream` is. The data ends at `endstream`.
    WrongStreamLength,
    /// A stream's `/Length` is a reference, which can't be resolved until the
    /// cross-reference table is read. The data ends at `endstream`.
    UnresolvedStreamLength,
    /// The `stream` keyword is followed by a lone carriage return instead of
    /// CR LF or LF.
    StreamLineEndIsCarriageReturn,
    /// The `stream` keyword isn't followed by an end of line. The data starts right
    /// after it.
    MissingStreamLineEnd,
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDictionaryKey => {
                f.write_str("repeated dictionary key, the last value is kept")
            }
            Self::MissingEndobj => f.write_str("missing endobj, the object is read anyway"),
            Self::MissingStreamLength => {
                f.write_str("stream has no length, its data ends at endstream")
            }
            Self::WrongStreamLength => {
                f.write_str("stream length is wrong, its data ends at endstream")
            }
            Self::UnresolvedStreamLength => {
                f.write_str("stream length is a reference, its data ends at endstream")
            }
            Self::StreamLineEndIsCarriageReturn => {
                f.write_str("stream keyword is followed by a lone carriage return")
            }
            Self::MissingStreamLineEnd => {
                f.write_str("stream keyword is not followed by an end of line")
            }
        }
    }
}

/// The diagnostics collected during an operation.
///
/// It keeps at most `capacity` of them, so a hostile file can't fill the memory
/// with warnings. The rest are only counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
    capacity: usize,
    dropped: usize,
}

impl Diagnostics {
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Vec::new(),
            capacity,
            dropped: 0,
        }
    }

    pub fn push(&mut self, kind: DiagnosticKind, offset: usize) {
        if self.items.len() < self.capacity {
            self.items.push(Diagnostic::new(kind, offset));
        } else {
            self.dropped += 1;
        }
    }

    /// Number of diagnostics kept.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` if nothing was reported, not even diagnostics that weren't kept.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.dropped == 0
    }

    /// Number of diagnostics that were reported after the capacity was reached.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }
}
