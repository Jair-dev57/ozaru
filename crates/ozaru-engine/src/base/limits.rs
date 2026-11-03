use std::fmt;

/// Upper bounds that protect the engine from hostile or broken PDFs.
///
/// The defaults are generous enough for real documents and small enough to stop
/// a file from exhausting memory or the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How deeply arrays and dictionaries can be nested inside each other. Values
    /// above [`Limits::MAX_DEPTH`] are treated as `MAX_DEPTH`.
    pub max_depth: usize,
    /// Maximum length of a string, in bytes.
    pub max_string_length: usize,
    /// Maximum length of a name, in bytes.
    pub max_name_length: usize,
    /// Maximum number of items in an array.
    pub max_array_length: usize,
    /// Maximum number of entries in a dictionary.
    pub max_dictionary_length: usize,
    /// Maximum size of decoded stream data, in bytes. It stops a small compressed
    /// stream from expanding to gigabytes.
    pub max_decoded_length: usize,
    /// Maximum number of diagnostics kept for one operation. Later ones are only
    /// counted.
    pub max_diagnostics: usize,
}

impl Limits {
    /// The deepest nesting allowed, whatever `max_depth` says.
    ///
    /// Dropping, cloning, comparing or printing an object visits its nested objects
    /// recursively, so a deeper object could overflow the stack. 1000 levels were
    /// measured to work with a 1 MiB stack, the smallest common default.
    pub const MAX_DEPTH: usize = 1_000;

    /// The nesting depth actually allowed: `max_depth`, capped at `MAX_DEPTH`.
    pub fn depth(&self) -> usize {
        self.max_depth.min(Self::MAX_DEPTH)
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_string_length: 64 * 1024 * 1024,
            max_name_length: 64 * 1024,
            max_array_length: 1_000_000,
            max_dictionary_length: 100_000,
            max_decoded_length: 256 * 1024 * 1024,
            max_diagnostics: 1_000,
        }
    }
}

/// Identifies one of the [`Limits`], to report which one was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Depth,
    StringLength,
    NameLength,
    ArrayLength,
    DictionaryLength,
    DecodedLength,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Depth => "nesting depth",
            Self::StringLength => "string length",
            Self::NameLength => "name length",
            Self::ArrayLength => "array length",
            Self::DictionaryLength => "dictionary length",
            Self::DecodedLength => "decoded stream length",
        })
    }
}
