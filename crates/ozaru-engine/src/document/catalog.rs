use std::sync::Arc;

use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Result};
use crate::syntax::{Dictionary, Name, Object, ObjectId};
use crate::xref::{Resolver, XrefEntry};

/// The document catalog, the root of a PDF's objects (ISO 32000-2, section
/// 7.7.2). It leads to the page tree and to document-wide data.
#[derive(Debug, Clone)]
pub struct Catalog {
    id: Option<ObjectId>,
    object: Arc<Object>,
}

impl Catalog {
    /// The catalog's object, or `None` if the trailer holds it directly instead of
    /// referring to it.
    pub fn id(&self) -> Option<ObjectId> {
        self.id
    }

    /// The catalog dictionary.
    pub fn dictionary(&self) -> &Dictionary {
        self.object
            .as_dictionary()
            .expect("a catalog is always a dictionary")
    }

    /// The `/Pages` entry, the root of the page tree: a reference, or rarely the
    /// dictionary itself.
    pub fn pages(&self) -> &Object {
        self.dictionary()
            .get("Pages")
            .expect("a catalog always has /Pages")
    }
}

/// Reads the catalog that the trailer's `/Root` refers to.
///
/// A dictionary with a `/Pages` reference or dictionary is accepted as the
/// catalog. If it isn't of `/Type /Catalog`, it is still used, with a
/// `MissingCatalogType` diagnostic, since many real files omit the type.
///
/// Damaged files are worked around, searching the objects of the file:
///
/// 1. If `/Root` is missing, can't be resolved or isn't such a dictionary, the
///    object of `/Type /Catalog` with `/Pages` furthest into the file is used,
///    since incremental updates append newer versions. It gets a `CatalogFound`
///    diagnostic.
/// 2. If no catalog has a usable `/Pages`, the root of the page tree is searched
///    for: the object of `/Type /Pages` without `/Parent` furthest into the file.
///    The broken catalog is used with its `/Pages` replaced, or an empty one is
///    made, with a `PageTreeFound` diagnostic.
///
/// It is a `MissingCatalog` error only if neither is found.
pub fn read_catalog(resolver: &mut Resolver<'_>, diagnostics: &mut Diagnostics) -> Result<Catalog> {
    let root = resolver.trailer().get("Root").cloned();
    let root_id = root.as_ref().and_then(Object::as_reference);
    let root = match root {
        Some(Object::Reference(id)) => resolver.resolve(id).ok().map(|object| (Some(id), object)),
        Some(object @ Object::Dictionary(_)) => Some((None, Arc::new(object))),
        _ => None,
    }
    .filter(|(_, object)| object.as_dictionary().is_some());
    if let Some((id, object)) = &root
        && let Some(dictionary) = object.as_dictionary()
        && has_pages(dictionary)
    {
        if !is_catalog(dictionary) {
            let offset = id.map_or(0, |id| resolver.object_offset(id.number));
            diagnostics.push(DiagnosticKind::MissingCatalogType, offset);
        }
        return Ok(Catalog {
            id: *id,
            object: object.clone(),
        });
    }

    let candidates = candidates(resolver, root_id);
    if let Some((id, object)) = find_last(resolver, &candidates, |d| is_catalog(d) && has_pages(d))
    {
        diagnostics.push(
            DiagnosticKind::CatalogFound,
            resolver.object_offset(id.number),
        );
        return Ok(Catalog {
            id: Some(id),
            object,
        });
    }

    let missing_at = root_id.map_or(0, |id| resolver.object_offset(id.number));
    let Some((tree, _)) = find_last(resolver, &candidates, is_page_tree_root) else {
        return Err(Error::new(ErrorKind::MissingCatalog, missing_at));
    };
    diagnostics.push(
        DiagnosticKind::PageTreeFound,
        resolver.object_offset(tree.number),
    );
    let (id, mut dictionary) = match root {
        Some((id, object)) => (id, object.as_dictionary().cloned()),
        None => match find_last(resolver, &candidates, is_catalog) {
            Some((id, object)) => (Some(id), object.as_dictionary().cloned()),
            None => (None, None),
        },
    };
    let dictionary = dictionary.get_or_insert_with(|| {
        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), Object::Name(Name::from("Catalog")));
        dictionary
    });
    dictionary.insert(Name::from("Pages"), Object::Reference(tree));
    Ok(Catalog {
        id,
        object: Arc::new(Object::Dictionary(dictionary.clone())),
    })
}

// The objects of the file other than `skip`, which was already tried, furthest
// into the file first, and for objects in the same object stream, the highest
// number first.
fn candidates(resolver: &Resolver<'_>, skip: Option<ObjectId>) -> Vec<ObjectId> {
    let mut candidates: Vec<(usize, ObjectId)> = resolver
        .table()
        .iter()
        .filter_map(|(number, entry)| {
            let generation = match entry {
                XrefEntry::InUse { generation, .. } => generation,
                XrefEntry::Compressed { .. } => 0,
                XrefEntry::Free { .. } => return None,
            };
            Some(ObjectId::new(number, generation))
        })
        .filter(|&id| Some(id) != skip)
        .map(|id| (resolver.object_offset(id.number), id))
        .collect();
    candidates.sort_by(|a, b| b.cmp(a));
    candidates.into_iter().map(|(_, id)| id).collect()
}

// The first of `candidates` that is a dictionary matching `accept`.
fn find_last(
    resolver: &mut Resolver<'_>,
    candidates: &[ObjectId],
    accept: impl Fn(&Dictionary) -> bool,
) -> Option<(ObjectId, Arc<Object>)> {
    candidates.iter().find_map(|&id| {
        let object = resolver.resolve(id).ok()?;
        accept(object.as_dictionary()?).then_some((id, object))
    })
}

fn has_type(dictionary: &Dictionary, kind: &[u8]) -> bool {
    dictionary
        .get("Type")
        .and_then(Object::as_name)
        .is_some_and(|name| name.as_bytes() == kind)
}

fn is_catalog(dictionary: &Dictionary) -> bool {
    has_type(dictionary, b"Catalog")
}

fn is_page_tree_root(dictionary: &Dictionary) -> bool {
    has_type(dictionary, b"Pages") && !dictionary.contains_key("Parent")
}

fn has_pages(dictionary: &Dictionary) -> bool {
    matches!(
        dictionary.get("Pages"),
        Some(Object::Reference(_) | Object::Dictionary(_))
    )
}
