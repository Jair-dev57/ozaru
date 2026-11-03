use std::fmt;

use super::Stream;
use super::chars::is_regular;

/// A PDF object (ISO 32000-2, section 7.3).
#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(PdfString),
    Name(Name),
    Array(Vec<Object>),
    Dictionary(Dictionary),
    /// Only found as the value of an indirect object.
    Stream(Stream),
    /// A reference to an indirect object, written `5 0 R` in a PDF.
    Reference(ObjectId),
}

impl Object {
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The value of an integer or a real, since PDF accepts either where a number
    /// is expected.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Real(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&PdfString> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&Name> {
        match self {
            Self::Name(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Object]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_dictionary(&self) -> Option<&Dictionary> {
        match self {
            Self::Dictionary(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_stream(&self) -> Option<&Stream> {
        match self {
            Self::Stream(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<ObjectId> {
        match self {
            Self::Reference(value) => Some(*value),
            _ => None,
        }
    }
}

/// Identifies an indirect object by its object number and generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    pub number: u32,
    pub generation: u16,
}

impl ObjectId {
    pub const fn new(number: u32, generation: u16) -> Self {
        Self { number, generation }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} R", self.number, self.generation)
    }
}

/// How a string was written in the file. Both forms hold the same kind of data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringFormat {
    /// Written between parentheses: `(Hello)`.
    Literal,
    /// Written as hexadecimal digits: `<48656C6C6F>`.
    Hexadecimal,
}

/// A PDF string. It holds bytes, not text: turning them into text depends on the
/// font or encoding, which later phases handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfString {
    bytes: Vec<u8>,
    format: StringFormat,
}

impl PdfString {
    pub fn new(bytes: impl Into<Vec<u8>>, format: StringFormat) -> Self {
        Self {
            bytes: bytes.into(),
            format,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn format(&self) -> StringFormat {
        self.format
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// A PDF name, such as `/Type`. It holds the bytes after the slash, with `#xx`
/// escapes already decoded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(Vec<u8>);

impl Name {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Writes the name as it would appear in a PDF, escaping bytes that need it.
impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("/")?;
        for &byte in &self.0 {
            // Bytes outside printable ASCII, and `#` itself, are escaped as #xx.
            if is_regular(byte) && byte != b'#' && (b'!'..=b'~').contains(&byte) {
                write!(f, "{}", byte as char)?;
            } else {
                write!(f, "#{byte:02X}")?;
            }
        }
        Ok(())
    }
}

/// A PDF dictionary. It keeps entries in the order they were inserted.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dictionary {
    entries: Vec<(Name, Object)>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<&Object> {
        let key = key.as_ref();
        self.entries
            .iter()
            .find(|(name, _)| name.as_bytes() == key)
            .map(|(_, value)| value)
    }

    pub fn contains_key(&self, key: impl AsRef<[u8]>) -> bool {
        self.get(key).is_some()
    }

    /// Adds an entry. If the key already exists, its value is replaced in place and
    /// the old value is returned.
    pub fn insert(&mut self, key: Name, value: Object) -> Option<Object> {
        match self.entries.iter_mut().find(|(name, _)| *name == key) {
            Some((_, existing)) => Some(std::mem::replace(existing, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Name, &Object)> {
        self.entries.iter().map(|(name, value)| (name, value))
    }
}
