use std::sync::Arc;

use super::{Date, Page, Rectangle, decode_text_string, parse_date};
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::Resolver;

/// Where a link goes (ISO 32000-2, sections 12.3.2 and 12.6.4).
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    /// A web address or other URI.
    Uri(String),
    /// A page of this document, counted from 0.
    Page(usize),
    /// Another file, or another kind of action, that isn't followed.
    Other,
}

/// An annotation of a page: a link, a note, a highlight, a form field's
/// widget... (ISO 32000-2, section 12.5).
#[derive(Debug, Clone)]
pub struct Annotation {
    /// The annotation's object, or `None` if the page holds it directly.
    pub id: Option<ObjectId>,
    /// `Link`, `Text`, `Highlight`, `Widget`... as the `/Subtype` says.
    pub subtype: String,
    /// The annotation's rectangle, in default user space.
    pub rect: Rectangle,
    /// `/Contents`: the text of a note, or a description.
    pub contents: Option<String>,
    /// `/NM`: the name that identifies it in its page.
    pub name: Option<String>,
    /// `/T`: who wrote it, for markup annotations.
    pub author: Option<String>,
    /// `/Subj`: a short summary.
    pub subject: Option<String>,
    /// `/M`: when it was last changed.
    pub modified: Option<Date>,
    /// `/F`: the flags, as the bits of the standard (hidden is 2, print 4...).
    pub flags: u32,
    /// `/C`: the color, as 1, 3 or 4 components of 0 to 1; empty if it has none.
    pub color: Vec<f64>,
    /// The quadrilaterals of text markup, 8 numbers each (`/QuadPoints`).
    pub quad_points: Vec<[f64; 8]>,
    /// Where a link goes, from `/A` or `/Dest`.
    pub link: Option<Link>,
    /// The form that draws it: `/AP /N`, or the state `/AS` chooses.
    pub appearance: Option<ObjectId>,
}

impl Annotation {
    /// Whether viewers don't show it: the hidden or no view flags.
    pub fn is_hidden(&self) -> bool {
        self.flags & (2 | 32) != 0
    }
}

// The most annotations read from a page, and the most nodes of a name tree.
const MAX_ANNOTATIONS: usize = 100_000;
const MAX_TREE_NODES: usize = 10_000;

fn resolved(resolver: &mut Resolver<'_>, object: &Object) -> Arc<Object> {
    match object {
        Object::Reference(id) => resolver
            .resolve(*id)
            .unwrap_or_else(|_| Arc::new(Object::Null)),
        other => Arc::new(other.clone()),
    }
}

fn text_of(resolver: &mut Resolver<'_>, dictionary: &Dictionary, key: &str) -> Option<String> {
    let object = resolved(resolver, dictionary.get(key)?);
    Some(decode_text_string(object.as_string()?.as_bytes()))
}

fn numbers_of(resolver: &mut Resolver<'_>, object: &Object, limit: usize) -> Vec<f64> {
    let array = resolved(resolver, object);
    let Some(items) = array.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .take(limit)
        .map_while(|item| resolved(resolver, item).as_number())
        .filter(|number| number.is_finite())
        .collect()
}

/// The annotations of `page`, in the order of its `/Annots`. `pages` are the
/// objects of every page, in order, to turn the destinations of links into page
/// numbers; `catalog` is the catalog's dictionary, for named destinations.
///
/// What isn't an annotation is skipped, and a page with no `/Annots` has none.
pub fn read_annotations(
    resolver: &mut Resolver<'_>,
    page: &Page,
    pages: &[Option<ObjectId>],
    catalog: &Dictionary,
) -> Vec<Annotation> {
    let Some(list) = page.dictionary().get("Annots") else {
        return Vec::new();
    };
    let list = resolved(resolver, list);
    let Some(items) = list.as_array() else {
        return Vec::new();
    };
    let mut annotations = Vec::new();
    for item in items.iter().take(MAX_ANNOTATIONS) {
        let id = item.as_reference();
        let object = resolved(resolver, item);
        let Some(dictionary) = object.as_dictionary() else {
            continue;
        };
        if let Some(annotation) = read_annotation(resolver, id, dictionary, pages, catalog) {
            annotations.push(annotation);
        }
    }
    annotations
}

fn read_annotation(
    resolver: &mut Resolver<'_>,
    id: Option<ObjectId>,
    dictionary: &Dictionary,
    pages: &[Option<ObjectId>],
    catalog: &Dictionary,
) -> Option<Annotation> {
    let subtype = resolved(resolver, dictionary.get("Subtype")?)
        .as_name()
        .map(|name| String::from_utf8_lossy(name.as_bytes()).into_owned())?;
    let rect = numbers_of(resolver, dictionary.get("Rect")?, 4);
    let &[x1, y1, x2, y2] = rect.as_slice() else {
        return None;
    };
    let flags = dictionary
        .get("F")
        .and_then(|flags| resolved(resolver, flags).as_integer())
        .map_or(0, |flags| flags as u32);
    let color = dictionary
        .get("C")
        .map(|color| numbers_of(resolver, color, 4))
        .filter(|color| matches!(color.len(), 1 | 3 | 4))
        .unwrap_or_default();
    let quad_points = dictionary
        .get("QuadPoints")
        .map(|quads| numbers_of(resolver, quads, 4096))
        .unwrap_or_default()
        .as_chunks::<8>()
        .0
        .to_vec();
    let modified = text_of(resolver, dictionary, "M").and_then(|date| parse_date(&date));
    let link = read_link(resolver, dictionary, pages, catalog);
    let appearance = normal_appearance(resolver, dictionary);
    Some(Annotation {
        id,
        subtype,
        rect: Rectangle::new(x1, y1, x2, y2),
        contents: text_of(resolver, dictionary, "Contents"),
        name: text_of(resolver, dictionary, "NM"),
        author: text_of(resolver, dictionary, "T"),
        subject: text_of(resolver, dictionary, "Subj"),
        modified,
        flags,
        color,
        quad_points,
        link,
        appearance,
    })
}

// The form of `/AP /N`: the stream itself, or the one a dictionary of states
// has for `/AS`.
fn normal_appearance(resolver: &mut Resolver<'_>, dictionary: &Dictionary) -> Option<ObjectId> {
    let appearance = resolved(resolver, dictionary.get("AP")?);
    let normal = appearance.as_dictionary()?.get("N")?.clone();
    let is_stream = |resolver: &mut Resolver<'_>, id: ObjectId| {
        matches!(resolver.resolve(id).as_deref(), Ok(Object::Stream(_)))
    };
    let states = match &normal {
        Object::Reference(id) if is_stream(resolver, *id) => return Some(*id),
        other => resolved(resolver, other),
    };
    let states = states.as_dictionary()?;
    let state = resolved(resolver, dictionary.get("AS")?);
    match states.get(state.as_name()?.as_bytes())? {
        Object::Reference(id) if is_stream(resolver, *id) => Some(*id),
        _ => None,
    }
}

fn read_link(
    resolver: &mut Resolver<'_>,
    dictionary: &Dictionary,
    pages: &[Option<ObjectId>],
    catalog: &Dictionary,
) -> Option<Link> {
    if let Some(action) = dictionary.get("A") {
        let action = resolved(resolver, action);
        let action = action.as_dictionary()?;
        let kind = resolved(resolver, action.get("S")?);
        return match kind.as_name()?.as_bytes() {
            b"URI" => {
                let uri = resolved(resolver, action.get("URI")?);
                let bytes = uri.as_string()?.as_bytes();
                Some(Link::Uri(String::from_utf8_lossy(bytes).into_owned()))
            }
            b"GoTo" => Some(
                destination_page(resolver, action.get("D")?, pages, catalog)
                    .map_or(Link::Other, Link::Page),
            ),
            _ => Some(Link::Other),
        };
    }
    let destination = dictionary.get("Dest")?;
    Some(destination_page(resolver, destination, pages, catalog).map_or(Link::Other, Link::Page))
}

// The page a destination names: an array that starts with a page's object, or
// a name or string that the catalog's destinations give an array for.
fn destination_page(
    resolver: &mut Resolver<'_>,
    destination: &Object,
    pages: &[Option<ObjectId>],
    catalog: &Dictionary,
) -> Option<usize> {
    let mut destination = resolved(resolver, destination);
    for _ in 0..4 {
        let name = match &*destination {
            Object::Name(name) => name.as_bytes().to_vec(),
            Object::String(string) => string.as_bytes().to_vec(),
            _ => break,
        };
        let found = named_destination(resolver, catalog, &name)?;
        destination = resolved(resolver, &found);
        // A dictionary of the old kind holds the array in `/D`.
        if let Some(inner) = destination.as_dictionary().and_then(|d| d.get("D")) {
            destination = resolved(resolver, inner);
        }
    }
    let array = destination.as_array()?;
    match array.first()? {
        Object::Reference(id) => pages.iter().position(|page| *page == Some(*id)),
        // Remote destinations name the page by number, which isn't this file's.
        _ => None,
    }
}

// The destination called `name`: from the catalog's `/Dests`, or from the name
// tree of `/Names /Dests`.
fn named_destination(
    resolver: &mut Resolver<'_>,
    catalog: &Dictionary,
    name: &[u8],
) -> Option<Object> {
    if let Some(dests) = catalog.get("Dests") {
        let dests = resolved(resolver, dests);
        if let Some(found) = dests.as_dictionary().and_then(|dests| dests.get(name)) {
            return Some(found.clone());
        }
    }
    let names = resolved(resolver, catalog.get("Names")?);
    let tree = names.as_dictionary()?.get("Dests")?.clone();
    let mut pending = vec![tree];
    let mut visited = 0;
    while let Some(node) = pending.pop() {
        visited += 1;
        if visited > MAX_TREE_NODES {
            return None;
        }
        let node = resolved(resolver, &node);
        let node = node.as_dictionary()?;
        if let Some(pairs) = node.get("Names") {
            let pairs = resolved(resolver, pairs);
            for pair in pairs.as_array()?.chunks(2) {
                let key = resolved(resolver, &pair[0]);
                if key.as_string().map(|key| key.as_bytes()) == Some(name) {
                    return pair.get(1).cloned();
                }
            }
        }
        if let Some(kids) = node.get("Kids") {
            let kids = resolved(resolver, kids);
            if let Some(kids) = kids.as_array() {
                pending.extend(kids.iter().take(MAX_TREE_NODES).cloned());
            }
        }
    }
    None
}
