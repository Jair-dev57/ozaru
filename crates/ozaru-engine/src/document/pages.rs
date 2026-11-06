use std::collections::HashSet;
use std::sync::Arc;

use super::Catalog;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::Resolver;

/// A page of the document: a leaf of the page tree (ISO 32000-2, section
/// 7.7.3.3).
#[derive(Debug, Clone)]
pub struct Page {
    id: Option<ObjectId>,
    object: Arc<Object>,
    damaged: bool,
}

impl Page {
    /// The page's object, or `None` if its parent holds it directly instead of
    /// referring to it.
    pub fn id(&self) -> Option<ObjectId> {
        self.id
    }

    /// The page dictionary, without the attributes it inherits. It is empty for a
    /// damaged page.
    pub fn dictionary(&self) -> &Dictionary {
        self.object
            .as_dictionary()
            .expect("a page is always a dictionary")
    }

    /// Whether the page's object exists but can't be read. The page is kept, so
    /// the pages after it keep their numbers, and readers can report it.
    pub fn is_damaged(&self) -> bool {
        self.damaged
    }
}

// A node of the page tree waiting to be visited.
struct Pending {
    node: Object,
    // Where to report problems with the node: its own offset, or its parent's
    // when it isn't an indirect object.
    offset: usize,
}

/// Reads the pages of the document, in order, walking the page tree from the
/// catalog's `/Pages` (ISO 32000-2, section 7.7.3).
///
/// The walk uses its own stack, so a deep tree can't overflow the program's.
/// Damaged trees are worked around, with a diagnostic each:
///
/// - **Cycles and repeats:** a node or page already visited is skipped, with
///   `PageTreeCycle`.
/// - **Unreadable kids:** a kid whose object exists but can't be read is kept as
///   a [damaged](Page::is_damaged) page, with `DamagedPage`, as most are pages
///   and the pages after it keep their numbers.
/// - **Broken kids:** a kid that resolves to null or isn't a dictionary is
///   skipped, with `InvalidPageTreeNode`. So is a kid of a type other than
///   `/Page` or `/Pages` without `/Kids`.
/// - **Missing types:** a node without `/Type` is a page tree node if it has
///   `/Kids`, and a page otherwise.
/// - **A wrong `/Count`:** only the root's `/Count` is checked, with
///   `WrongPageCount`, and the pages found are the ones used. A root that is a
///   page, as some files have, is the only page.
///
/// It is an `InvalidPageTree` error if `/Pages` doesn't lead to a dictionary, and
/// a `LimitExceeded` error past `max_pages` pages.
pub fn read_pages(
    resolver: &mut Resolver<'_>,
    catalog: &Catalog,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Result<Vec<Page>> {
    let root_offset = catalog
        .pages()
        .as_reference()
        .or(catalog.id())
        .map_or(0, |id| resolver.object_offset(id.number));
    let root = match catalog.pages() {
        Object::Reference(id) => resolver.resolve(*id).ok(),
        object => Some(Arc::new(object.clone())),
    };
    let Some(root) = root.as_ref().and_then(|root| root.as_dictionary()) else {
        return Err(Error::new(ErrorKind::InvalidPageTree, root_offset));
    };
    // Only a page tree node has a /Count: some files have a single page as the
    // root.
    let count = match node_kind(root) {
        NodeKind::Pages => Some(root.get("Count").and_then(Object::as_integer)),
        _ => None,
    };

    let mut pages = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = vec![Pending {
        node: catalog.pages().clone(),
        offset: root_offset,
    }];
    while let Some(Pending { node, offset }) = stack.pop() {
        let (id, offset, object) = match node {
            Object::Reference(id) => {
                let offset = resolver.object_offset(id.number);
                if !visited.insert(id) {
                    diagnostics.push(DiagnosticKind::PageTreeCycle, offset);
                    continue;
                }
                match resolver.resolve(id) {
                    Ok(object) => (Some(id), offset, object),
                    Err(_) => {
                        diagnostics.push(DiagnosticKind::DamagedPage, offset);
                        if pages.len() == limits.max_pages {
                            return Err(Error::new(ErrorKind::LimitExceeded(Limit::Pages), offset));
                        }
                        pages.push(Page {
                            id: Some(id),
                            object: Arc::new(Object::Dictionary(Dictionary::new())),
                            damaged: true,
                        });
                        continue;
                    }
                }
            }
            object => (None, offset, Arc::new(object)),
        };
        let Some(dictionary) = object.as_dictionary() else {
            diagnostics.push(DiagnosticKind::InvalidPageTreeNode, offset);
            continue;
        };

        match node_kind(dictionary) {
            NodeKind::Page => {
                if pages.len() == limits.max_pages {
                    return Err(Error::new(ErrorKind::LimitExceeded(Limit::Pages), offset));
                }
                pages.push(Page {
                    id,
                    object,
                    damaged: false,
                });
            }
            NodeKind::Pages => {
                let kids = dictionary
                    .get("Kids")
                    .and_then(Object::as_array)
                    .unwrap_or_default();
                // Pushed last to first, so the first kid is visited first.
                stack.extend(kids.iter().rev().map(|kid| Pending {
                    node: kid.clone(),
                    offset,
                }));
            }
            NodeKind::Other => diagnostics.push(DiagnosticKind::InvalidPageTreeNode, offset),
        }
    }

    if let Some(count) = count
        && count != i64::try_from(pages.len()).ok()
    {
        diagnostics.push(DiagnosticKind::WrongPageCount, root_offset);
    }
    Ok(pages)
}

enum NodeKind {
    Page,
    Pages,
    Other,
}

fn node_kind(dictionary: &Dictionary) -> NodeKind {
    let has_kids = || dictionary.get("Kids").and_then(Object::as_array).is_some();
    match dictionary.get("Type").and_then(Object::as_name) {
        Some(name) if name.as_bytes() == b"Page" => NodeKind::Page,
        Some(name) if name.as_bytes() == b"Pages" => NodeKind::Pages,
        _ if has_kids() => NodeKind::Pages,
        None => NodeKind::Page,
        Some(_) => NodeKind::Other,
    }
}
