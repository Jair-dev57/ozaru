use std::collections::HashSet;
use std::sync::Arc;

use super::{Catalog, Resources};
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::{Resolver, XrefEntry};

/// A page of the document: a leaf of the page tree (ISO 32000-2, section
/// 7.7.3.3).
#[derive(Debug, Clone)]
pub struct Page {
    id: Option<ObjectId>,
    object: Arc<Object>,
    damaged: bool,
    media_box: Rectangle,
    crop_box: Rectangle,
    rotation: u16,
    resources: Resources,
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

    /// The boundaries of the physical medium, in default user space units (1/72
    /// inch). US Letter, `[0 0 612 792]`, if neither the page nor its ancestors
    /// have a usable one.
    pub fn media_box(&self) -> Rectangle {
        self.media_box
    }

    /// The visible region of the page: its `/CropBox` clipped to the media box,
    /// or the media box itself if it has none.
    pub fn crop_box(&self) -> Rectangle {
        self.crop_box
    }

    /// How many degrees the page is rotated clockwise when shown: 0, 90, 180 or
    /// 270.
    pub fn rotation(&self) -> u16 {
        self.rotation
    }

    /// The resources the page's content uses, such as its fonts. Empty if neither
    /// the page nor its ancestors have any.
    pub fn resources(&self) -> &Resources {
        &self.resources
    }
}

/// A rectangle in default user space, with its corners in order: `left <= right`
/// and `bottom <= top` (ISO 32000-2, section 7.9.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectangle {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

impl Rectangle {
    /// US Letter, the default media box.
    pub const LETTER: Self = Self {
        left: 0.0,
        bottom: 0.0,
        right: 612.0,
        top: 792.0,
    };

    /// The rectangle with corners at `(x1, y1)` and `(x2, y2)`, in any order.
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            left: x1.min(x2),
            bottom: y1.min(y2),
            right: x1.max(x2),
            top: y1.max(y2),
        }
    }

    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.top - self.bottom
    }

    /// The part of this rectangle inside `other`, if they overlap with some area.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let clipped = Self {
            left: self.left.max(other.left),
            bottom: self.bottom.max(other.bottom),
            right: self.right.min(other.right),
            top: self.top.min(other.top),
        };
        clipped.has_area().then_some(clipped)
    }

    fn has_area(&self) -> bool {
        self.width() > 0.0 && self.height() > 0.0
    }
}

// The attributes a page inherits from its ancestors (ISO 32000-2, section
// 7.7.3.4): the values of the nearest ancestor that has each, unresolved. For
// the resources, the ancestor itself, so they are shared and not copied.
#[derive(Debug, Clone, Default)]
struct Inherited {
    media_box: Option<Object>,
    crop_box: Option<Object>,
    rotate: Option<Object>,
    resources: Option<Arc<Object>>,
}

impl Inherited {
    // These attributes, with the ones of `node`, a dictionary, replacing them.
    fn with(self: &Arc<Self>, node: &Arc<Object>) -> Arc<Self> {
        let dictionary = node.as_dictionary().expect("a node is always a dictionary");
        let keys = ["MediaBox", "CropBox", "Rotate", "Resources"];
        if !keys.iter().any(|&key| dictionary.contains_key(key)) {
            return self.clone();
        }
        let pick = |key: &str, inherited: &Option<Object>| {
            dictionary.get(key).cloned().or(inherited.clone())
        };
        let resources = if dictionary.contains_key("Resources") {
            Some(node.clone())
        } else {
            self.resources.clone()
        };
        Arc::new(Self {
            media_box: pick("MediaBox", &self.media_box),
            crop_box: pick("CropBox", &self.crop_box),
            rotate: pick("Rotate", &self.rotate),
            resources,
        })
    }
}

// A node of the page tree waiting to be visited.
struct Pending {
    node: Object,
    inherited: Arc<Inherited>,
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
/// - **Attributes:** each page gets the `/MediaBox`, `/CropBox`, `/Rotate` and
///   `/Resources` of its nearest ancestor that has them, unless it has its own.
///   A missing media box is US Letter, with `MissingMediaBox`. Unusable boxes,
///   rotations and resources get their default, with `InvalidPageBox`,
///   `InvalidRotation` or `InvalidResources`.
/// - **A wrong `/Count`:** only the root's `/Count` is checked, with
///   `WrongPageCount`, and the pages found are the ones used. A root that is a
///   page, as some files have, is the only page.
///
/// - **A destroyed root:** if `/Pages` doesn't lead to a dictionary, the pages
///   are rebuilt from the objects of the file, with `PageTreeRebuilt`: see
///   [`rebuild_pages`].
///
/// It is an `InvalidPageTree` error if the root is destroyed and the file has no
/// page objects either, a `LimitExceeded` error past `max_pages` pages, and a `PasswordRequired` error
/// if a node of the tree is in an object stream that can't be decrypted yet.
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
        Object::Reference(id) => match resolver.resolve(*id) {
            Ok(root) => Some(root),
            Err(error) if error.kind() == ErrorKind::PasswordRequired => return Err(error),
            Err(_) => None,
        },
        object => Some(Arc::new(object.clone())),
    };
    let Some(root) = root.as_ref().and_then(|root| root.as_dictionary()) else {
        return rebuild_pages(resolver, root_offset, limits, diagnostics);
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
        inherited: Arc::default(),
        offset: root_offset,
    }];
    while let Some(Pending {
        node,
        inherited,
        offset,
    }) = stack.pop()
    {
        let (id, offset, object) = match node {
            Object::Reference(id) => {
                let offset = resolver.object_offset(id.number);
                if !visited.insert(id) {
                    diagnostics.push(DiagnosticKind::PageTreeCycle, offset);
                    continue;
                }
                match resolver.resolve(id) {
                    Ok(object) => (Some(id), offset, object),
                    // A node that can't be decrypted yet may hold many pages, so
                    // the pages can't be counted.
                    Err(error) if error.kind() == ErrorKind::PasswordRequired => {
                        return Err(error);
                    }
                    Err(_) => {
                        diagnostics.push(DiagnosticKind::DamagedPage, offset);
                        if pages.len() == limits.max_pages {
                            return Err(Error::new(ErrorKind::LimitExceeded(Limit::Pages), offset));
                        }
                        // It gets the attributes of its ancestors. Problems with
                        // them aren't reported, as the page is reported already.
                        let object = Arc::new(Object::Dictionary(Dictionary::new()));
                        let mut ignored = Diagnostics::new(0);
                        let page =
                            make_page(resolver, Some(id), object, &inherited, offset, &mut ignored);
                        pages.push(Page {
                            damaged: true,
                            ..page
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
                let page = make_page(resolver, id, object, &inherited, offset, diagnostics);
                pages.push(page);
            }
            NodeKind::Pages => {
                let kids = dictionary
                    .get("Kids")
                    .and_then(Object::as_array)
                    .unwrap_or_default();
                let inherited = inherited.with(&object);
                // Pushed last to first, so the first kid is visited first.
                stack.extend(kids.iter().rev().map(|kid| Pending {
                    node: kid.clone(),
                    inherited: inherited.clone(),
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

/// The pages of a file whose page tree root is destroyed, found among its
/// objects: every dictionary of `/Type /Page`, in the order they appear in the
/// file, which is the order most writers use. Without the tree, that order is a
/// guess, so it gets a `PageTreeRebuilt` diagnostic at `offset`.
///
/// Each page still inherits from the ancestors it can reach through `/Parent`,
/// so only what the destroyed nodes gave is lost. An object that can't be read
/// but whose bytes have `/Type /Page` is kept as a [damaged](Page::is_damaged)
/// page, with `DamagedPage`. It is an `InvalidPageTree` error if there is no page
/// object at all.
pub fn rebuild_pages(
    resolver: &mut Resolver<'_>,
    offset: usize,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Result<Vec<Page>> {
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
        .map(|id| (resolver.object_offset(id.number), id))
        .collect();
    candidates.sort();

    let mut pages = Vec::new();
    let mut found = Diagnostics::new(limits.max_diagnostics);
    for (page_offset, id) in candidates {
        let object = match resolver.resolve(id) {
            Ok(object) => object,
            // An object that can't be read, but whose bytes say it is a page, is
            // kept as a damaged page, as in the page tree.
            Err(_) if looks_like_a_page(resolver, id, page_offset) => {
                if pages.len() == limits.max_pages {
                    return Err(Error::new(
                        ErrorKind::LimitExceeded(Limit::Pages),
                        page_offset,
                    ));
                }
                found.push(DiagnosticKind::DamagedPage, page_offset);
                let empty = Arc::new(Object::Dictionary(Dictionary::new()));
                let mut ignored = Diagnostics::new(0);
                let page = make_page(
                    resolver,
                    Some(id),
                    empty,
                    &Arc::default(),
                    page_offset,
                    &mut ignored,
                );
                pages.push(Page {
                    damaged: true,
                    ..page
                });
                continue;
            }
            Err(_) => continue,
        };
        let is_page = object.as_dictionary().is_some_and(|dictionary| {
            dictionary
                .get("Type")
                .and_then(Object::as_name)
                .is_some_and(|name| name.as_bytes() == b"Page")
        });
        if !is_page {
            continue;
        }
        if pages.len() == limits.max_pages {
            return Err(Error::new(
                ErrorKind::LimitExceeded(Limit::Pages),
                page_offset,
            ));
        }
        let dictionary = object.as_dictionary().expect("checked above");
        let inherited = ancestors_of(resolver, dictionary, limits);
        pages.push(make_page(
            resolver,
            Some(id),
            object.clone(),
            &inherited,
            page_offset,
            &mut found,
        ));
    }
    if pages.is_empty() {
        return Err(Error::new(ErrorKind::InvalidPageTree, offset));
    }
    diagnostics.push(DiagnosticKind::PageTreeRebuilt, offset);
    diagnostics.append(&found);
    Ok(pages)
}

// Whether the bytes of object `id`, up to its `endobj`, have `/Type /Page`, and
// not `/Type /Pages`. Only objects outside object streams can be checked.
fn looks_like_a_page(resolver: &Resolver<'_>, id: ObjectId, offset: usize) -> bool {
    const WINDOW: usize = 4096;
    if !matches!(
        resolver.table().get(id.number),
        Some(XrefEntry::InUse { .. })
    ) {
        return false;
    }
    let file = resolver.file();
    let start = offset.min(file.len());
    let end = file.len().min(start + WINDOW);
    let bytes = &file[start..end];
    let bytes = match bytes.windows(6).position(|w| w == b"endobj") {
        Some(position) => &bytes[..position],
        None => bytes,
    };
    let key = b"/Type";
    bytes.windows(key.len()).enumerate().any(|(i, w)| {
        if w != key {
            return false;
        }
        let rest = &bytes[i + key.len()..];
        let value = rest.iter().position(|&b| b == b'/').map(|p| &rest[p..]);
        // Only whitespace between the key and the value.
        let only_spaces = rest
            .iter()
            .take_while(|&&b| b != b'/')
            .all(|&b| b.is_ascii_whitespace());
        only_spaces
            && value.is_some_and(|value| {
                value.starts_with(b"/Page") && value.get(5).is_none_or(|&b| b != b's')
            })
    })
}

// What a page inherits from the ancestors it reaches through /Parent, nearest
// last so it wins. The chain stops at a cycle, a node that can't be read, or
// past `max_depth` nodes.
fn ancestors_of(resolver: &mut Resolver<'_>, page: &Dictionary, limits: Limits) -> Arc<Inherited> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();
    let mut next = page.get("Parent").and_then(Object::as_reference);
    while let Some(id) = next {
        if !visited.insert(id) || chain.len() >= limits.depth() {
            break;
        }
        let Ok(node) = resolver.resolve(id) else {
            break;
        };
        let Some(dictionary) = node.as_dictionary() else {
            break;
        };
        next = dictionary.get("Parent").and_then(Object::as_reference);
        chain.push(node.clone());
    }
    chain
        .iter()
        .rev()
        .fold(Arc::default(), |inherited, node| inherited.with(node))
}

// The page of `object`, with the attributes it has or inherits, checked.
fn make_page(
    resolver: &mut Resolver<'_>,
    id: Option<ObjectId>,
    object: Arc<Object>,
    inherited: &Arc<Inherited>,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> Page {
    let dictionary = object
        .as_dictionary()
        .expect("a page is always a dictionary");
    // The page's own entries, or else the inherited ones, read in place.
    let attribute = |key: &str| dictionary.get(key);

    let media_box = match attribute("MediaBox").or(inherited.media_box.as_ref()) {
        None => {
            diagnostics.push(DiagnosticKind::MissingMediaBox, offset);
            Rectangle::LETTER
        }
        Some(entry) => match rectangle(resolver, entry).filter(Rectangle::has_area) {
            Some(media_box) => media_box,
            None => {
                diagnostics.push(DiagnosticKind::InvalidPageBox, offset);
                Rectangle::LETTER
            }
        },
    };
    let crop_box = match attribute("CropBox").or(inherited.crop_box.as_ref()) {
        None => media_box,
        Some(entry) => {
            let clipped = rectangle(resolver, entry).and_then(|crop| crop.intersection(&media_box));
            clipped.unwrap_or_else(|| {
                diagnostics.push(DiagnosticKind::InvalidPageBox, offset);
                media_box
            })
        }
    };
    let rotation = match attribute("Rotate").or(inherited.rotate.as_ref()) {
        None => 0,
        Some(entry) => rotation(resolver, entry).unwrap_or_else(|| {
            diagnostics.push(DiagnosticKind::InvalidRotation, offset);
            0
        }),
    };
    let holder = match attribute("Resources") {
        Some(_) => Some(&object),
        None => inherited.resources.as_ref(),
    };
    let resources = match holder {
        None => Resources::default(),
        Some(holder) => Resources::of(resolver, holder).unwrap_or_else(|| {
            diagnostics.push(DiagnosticKind::InvalidResources, offset);
            Resources::default()
        }),
    };

    Page {
        id,
        object,
        damaged: false,
        media_box,
        crop_box,
        rotation,
        resources,
    }
}

// `object`, resolved if it is a reference. None if it can't be.
fn resolved(resolver: &mut Resolver<'_>, object: &Object) -> Option<Arc<Object>> {
    match object {
        Object::Reference(id) => resolver.resolve(*id).ok(),
        object => Some(Arc::new(object.clone())),
    }
}

// An array of four finite numbers, each possibly a reference.
fn rectangle(resolver: &mut Resolver<'_>, entry: &Object) -> Option<Rectangle> {
    let array = resolved(resolver, entry)?;
    let [x1, y1, x2, y2] = array.as_array()? else {
        return None;
    };
    let mut number = |item: &Object| {
        resolved(resolver, item)?
            .as_number()
            .filter(|value| value.is_finite())
    };
    Some(Rectangle::new(
        number(x1)?,
        number(y1)?,
        number(x2)?,
        number(y2)?,
    ))
}

// A whole multiple of 90 degrees, such as -90 or 450, brought into 0 to 270.
fn rotation(resolver: &mut Resolver<'_>, entry: &Object) -> Option<u16> {
    let degrees = resolved(resolver, entry)?.as_number()?;
    if !degrees.is_finite() || degrees % 90.0 != 0.0 {
        return None;
    }
    let turns = (degrees / 90.0).rem_euclid(4.0);
    Some(turns as u16 * 90)
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
