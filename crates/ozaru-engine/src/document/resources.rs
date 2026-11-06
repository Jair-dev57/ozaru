use crate::syntax::{Dictionary, Object};
use crate::xref::Resolver;

/// The resources a content stream uses: fonts, images, graphics states and
/// others, each category in a dictionary of names (ISO 32000-2, section 7.8.3).
///
/// The entries are kept as they are in the file, usually references, and are
/// only resolved when they are used.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resources {
    dictionary: Dictionary,
}

impl Resources {
    /// The resources of `entry`, a dictionary or a reference to one. None if it
    /// isn't one.
    pub(crate) fn read(resolver: &mut Resolver<'_>, entry: &Object) -> Option<Self> {
        let dictionary = match entry {
            Object::Reference(id) => resolver.resolve(*id).ok()?.as_dictionary()?.clone(),
            Object::Dictionary(dictionary) => dictionary.clone(),
            _ => return None,
        };
        Some(Self { dictionary })
    }

    /// The resource dictionary.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// One category, such as `Font` or `XObject`, as it is in the file: a
    /// dictionary or a reference to one.
    pub fn category(&self, name: &str) -> Option<&Object> {
        self.dictionary.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.dictionary.is_empty()
    }
}
