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
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDictionaryKey => {
                f.write_str("repeated dictionary key, the last value is kept")
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
