use super::{Date, decode_text_string, parse_date};
use crate::base::{DiagnosticKind, Diagnostics};
use crate::syntax::{Dictionary, Name, Object};
use crate::xref::Resolver;

/// The document information dictionary: title, author, dates and other data
/// about the document (ISO 32000-2, section 14.3.3).
///
/// PDF 2.0 deprecates it in favor of XMP metadata, but most files still have it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    dictionary: Dictionary,
}

/// Whether the document has been adjusted for printing registration, its
/// `/Trapped` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trapped {
    True,
    False,
    Unknown,
}

impl Info {
    /// The dictionary, with the references of its entries resolved.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// The text of entry `key`, decoded with [`decode_text_string`]. `None` if it
    /// is missing or isn't a string.
    pub fn text(&self, key: &str) -> Option<String> {
        let string = self.dictionary.get(key)?.as_string()?;
        Some(decode_text_string(string.as_bytes()))
    }

    pub fn title(&self) -> Option<String> {
        self.text("Title")
    }

    pub fn author(&self) -> Option<String> {
        self.text("Author")
    }

    pub fn subject(&self) -> Option<String> {
        self.text("Subject")
    }

    pub fn keywords(&self) -> Option<String> {
        self.text("Keywords")
    }

    /// The application that created the original document.
    pub fn creator(&self) -> Option<String> {
        self.text("Creator")
    }

    /// The application that converted it to PDF.
    pub fn producer(&self) -> Option<String> {
        self.text("Producer")
    }

    /// The creation date, if it can be parsed. Its text is in
    /// `text("CreationDate")`.
    pub fn creation_date(&self) -> Option<Date> {
        parse_date(&self.text("CreationDate")?)
    }

    /// The date of the last change, if it can be parsed. Its text is in
    /// `text("ModDate")`.
    pub fn modification_date(&self) -> Option<Date> {
        parse_date(&self.text("ModDate")?)
    }

    /// The `/Trapped` entry: a name, or in older files a boolean.
    pub fn trapped(&self) -> Option<Trapped> {
        match self.dictionary.get("Trapped")? {
            Object::Boolean(true) => Some(Trapped::True),
            Object::Boolean(false) => Some(Trapped::False),
            Object::Name(name) => match name.as_bytes() {
                b"True" => Some(Trapped::True),
                b"False" => Some(Trapped::False),
                b"Unknown" => Some(Trapped::Unknown),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Reads the information dictionary that the trailer's `/Info` refers to, with
/// the references of its entries resolved.
///
/// It is `None` if the file has none. It is also `None`, with a diagnostic, if
/// `/Info` isn't a dictionary, with `InvalidInfo`, or if the file is encrypted
/// and still locked, with `LockedInfo`, since its strings can't be decrypted yet.
pub fn read_info(resolver: &mut Resolver<'_>, diagnostics: &mut Diagnostics) -> Option<Info> {
    let entry = resolver.trailer().get("Info")?.clone();
    let id = entry.as_reference();
    let offset = id.map_or(0, |id| resolver.object_offset(id.number));
    // The resolver only decrypts indirect objects, so the strings of a dictionary
    // held by the trailer of an encrypted file can't be decrypted.
    if resolver.needs_password() || (resolver.is_encrypted() && id.is_none()) {
        diagnostics.push(DiagnosticKind::LockedInfo, offset);
        return None;
    }
    let object = match id {
        Some(id) => resolver.resolve(id).ok(),
        None => Some(std::sync::Arc::new(entry)),
    };
    let Some(dictionary) = object.as_ref().and_then(|object| object.as_dictionary()) else {
        if !object.is_some_and(|object| object.is_null()) {
            diagnostics.push(DiagnosticKind::InvalidInfo, offset);
        }
        return None;
    };

    let mut resolved = Dictionary::new();
    for (key, value) in dictionary.iter() {
        let value = match value {
            Object::Reference(id) => match resolver.resolve(*id) {
                Ok(object) => (*object).clone(),
                Err(_) => continue,
            },
            value => value.clone(),
        };
        resolved.insert(Name::new(key.as_bytes()), value);
    }
    Some(Info {
        dictionary: resolved,
    })
}
