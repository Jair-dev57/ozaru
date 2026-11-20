use std::sync::Arc;

use crate::syntax::{Dictionary, Object};
use crate::xref::Resolver;

/// The resources a content stream uses: fonts, images, graphics states and
/// others, each category in a dictionary of names (ISO 32000-2, section 7.8.3).
///
/// The entries are kept as they are in the file, usually references, and are
/// only resolved when they are used. The dictionary isn't copied: pages that
/// share their resources share them here too.
#[derive(Debug, Clone, Default)]
pub struct Resources {
    source: Source,
}

// Where the resource dictionary is.
#[derive(Debug, Clone, Default)]
enum Source {
    #[default]
    Empty,
    // An indirect object that is the dictionary.
    Object(Arc<Object>),
    // A page, page tree node or form whose `/Resources` is the dictionary.
    Entry(Arc<Object>),
}

static EMPTY: Dictionary = Dictionary::new();

impl Resources {
    /// The resources of `holder`, a page, page tree node or form: its
    /// `/Resources`, a dictionary or a reference to one. None if it has none, or
    /// it isn't one.
    pub(crate) fn of(resolver: &mut Resolver<'_>, holder: &Arc<Object>) -> Option<Self> {
        let source = match entry_of(holder)? {
            Object::Reference(id) => {
                let object = resolver.resolve(*id).ok()?;
                object.as_dictionary()?;
                Source::Object(object)
            }
            Object::Dictionary(_) => Source::Entry(holder.clone()),
            _ => return None,
        };
        Some(Self { source })
    }

    /// The resource dictionary.
    pub fn dictionary(&self) -> &Dictionary {
        let dictionary = match &self.source {
            Source::Empty => None,
            Source::Object(object) => object.as_dictionary(),
            Source::Entry(holder) => entry_of(holder).and_then(Object::as_dictionary),
        };
        dictionary.unwrap_or(&EMPTY)
    }

    /// One category, such as `Font` or `XObject`, as it is in the file: a
    /// dictionary or a reference to one.
    pub fn category(&self, name: &str) -> Option<&Object> {
        self.dictionary().get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.dictionary().is_empty()
    }
}

impl PartialEq for Resources {
    fn eq(&self, other: &Self) -> bool {
        self.dictionary() == other.dictionary()
    }
}

// The `/Resources` entry of a dictionary or a stream's dictionary.
fn entry_of(holder: &Object) -> Option<&Object> {
    let dictionary = match holder {
        Object::Stream(stream) => stream.dictionary(),
        other => other.as_dictionary()?,
    };
    dictionary.get("Resources")
}
