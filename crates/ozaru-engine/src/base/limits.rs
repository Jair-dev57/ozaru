use std::fmt;

/// Upper bounds that protect the engine from hostile or broken PDFs.
///
/// The defaults are generous enough for real documents and small enough to stop
/// a file from exhausting memory or the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How deeply arrays and dictionaries can be nested inside each other.
    pub max_depth: usize,
    /// Maximum length of a string, in bytes.
    pub max_string_length: usize,
    /// Maximum length of a name, in bytes.
    pub max_name_length: usize,
    /// Maximum number of items in an array.
    pub max_array_length: usize,
    /// Maximum number of entries in a dictionary.
    pub max_dictionary_length: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_string_length: 64 * 1024 * 1024,
            max_name_length: 64 * 1024,
            max_array_length: 1_000_000,
            max_dictionary_length: 100_000,
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
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Depth => "nesting depth",
            Self::StringLength => "string length",
            Self::NameLength => "name length",
            Self::ArrayLength => "array length",
            Self::DictionaryLength => "dictionary length",
        })
    }
}
