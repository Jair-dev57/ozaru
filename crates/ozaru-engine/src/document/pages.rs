use std::collections::HashSet;
use std::sync::Arc;

use super::{Catalog, Resources};
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
// 7.7.3.4): the values of the nearest ancestor that has each, unresolved.
#[derive(Debug, Clone, Default)]
struct Inherited {
    media_box: Option<Object>,
    crop_box: Option<Object>,
    rotate: Option<Object>,
    resources: Option<Object>,
}

impl Inherited {
    // These attributes, with the ones of `node` replacing them.
    fn with(self: &Arc<Self>, node: &Dictionary) -> Arc<Self> {
        let keys = ["MediaBox", "CropBox", "Rotate", "Resources"];
        if !keys.iter().any(|&key| node.contains_key(key)) {
            return self.clone();
        }
        let pick =
            |key: &str, inherited: &Option<Object>| node.get(key).cloned().or(inherited.clone());
        Arc::new(Self {
            media_box: pick("MediaBox", &self.media_box),
            crop_box: pick("CropBox", &self.crop_box),
            rotate: pick("Rotate", &self.rotate),
            resources: pick("Resources", &self.resources),
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
                let inherited = inherited.with(dictionary);
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
    let attributes = inherited.with(dictionary);

    let media_box = match &attributes.media_box {
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
    let crop_box = match &attributes.crop_box {
        None => media_box,
        Some(entry) => {
            let clipped = rectangle(resolver, entry).and_then(|crop| crop.intersection(&media_box));
            clipped.unwrap_or_else(|| {
                diagnostics.push(DiagnosticKind::InvalidPageBox, offset);
                media_box
            })
        }
    };
    let rotation = match &attributes.rotate {
        None => 0,
        Some(entry) => rotation(resolver, entry).unwrap_or_else(|| {
            diagnostics.push(DiagnosticKind::InvalidRotation, offset);
            0
        }),
    };
    let resources = match &attributes.resources {
        None => Resources::default(),
        Some(entry) => Resources::read(resolver, entry).unwrap_or_else(|| {
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
