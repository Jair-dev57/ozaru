//! What a reader uses to move around a document: bookmarks (the outline), page
//! labels, named destinations and attached files (ISO 32000-2, sections 12.3.3,
//! 12.4.2, 12.3.2.3 and 7.11.4).

use std::collections::HashSet;

use super::annotations::{destination_page, read_link, resolved, text_of};
use super::{Date, Link, decode_text_string, parse_date};
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::Resolver;

/// The most bookmarks, destinations, labels and attachments read, the most
/// nodes of a tree walked, and the deepest the bookmarks go.
const MAX_ITEMS: usize = 100_000;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 64;

/// A bookmark: an entry of the outline a viewer shows beside the pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub title: String,
    /// How deep it is, from 1 for the top ones.
    pub level: usize,
    /// The page it goes to, counted from 0, if it goes to one of this file.
    pub page: Option<usize>,
    /// The address it opens, if it is a link.
    pub uri: Option<String>,
    /// Whether a viewer shows its children (`/Count` is positive).
    pub open: bool,
}

/// The bookmarks of the document in the order a viewer shows them: each one
/// followed by its children. `pages` are the objects of every page, in order.
/// Empty if there is no outline.
pub fn read_outlines(
    resolver: &mut Resolver<'_>,
    catalog: &Dictionary,
    pages: &[Option<ObjectId>],
) -> Vec<Bookmark> {
    let mut bookmarks = Vec::new();
    let Some(outlines) = catalog.get("Outlines") else {
        return bookmarks;
    };
    let outlines = resolved(resolver, outlines);
    let Some(first) = outlines
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("First"))
        .and_then(Object::as_reference)
    else {
        return bookmarks;
    };
    // The next item to read and its level; the siblings wait under the children.
    let mut pending = vec![(first, 1)];
    let mut seen = HashSet::new();
    while let Some((id, level)) = pending.pop() {
        if bookmarks.len() >= MAX_ITEMS || !seen.insert(id.number) {
            continue;
        }
        let object = resolved(resolver, &Object::Reference(id));
        let Some(item) = object.as_dictionary() else {
            continue;
        };
        if let Some(next) = item.get("Next").and_then(Object::as_reference) {
            pending.push((next, level));
        }
        let children = item.get("First").and_then(Object::as_reference);
        if let Some(child) = children.filter(|_| level < MAX_DEPTH) {
            pending.push((child, level + 1));
        }
        let (page, uri) = match read_link(resolver, item, pages, &Dictionary::new()) {
            Some(Link::Page(page)) => (Some(page), None),
            Some(Link::Uri(uri)) => (None, Some(uri)),
            _ => (None, None),
        };
        let open = item
            .get("Count")
            .and_then(|count| resolved(resolver, count).as_integer())
            .is_some_and(|count| count > 0);
        bookmarks.push(Bookmark {
            title: text_of(resolver, item, "Title").unwrap_or_default(),
            level,
            page,
            uri,
            open,
        });
    }
    bookmarks
}

// The pairs of a tree, in its order: the key and the value of each entry of
// `leaf` (`/Names` or `/Nums`) of its nodes, following `/Kids`.
fn tree(resolver: &mut Resolver<'_>, root: &Object, leaf: &str) -> Vec<(Object, Object)> {
    let mut pairs = Vec::new();
    let mut pending = vec![root.clone()];
    let mut visited = 0;
    let mut seen = HashSet::new();
    while let Some(node) = pending.pop() {
        visited += 1;
        if visited > MAX_NODES || pairs.len() >= MAX_ITEMS {
            break;
        }
        if let Some(id) = node.as_reference()
            && !seen.insert(id.number)
        {
            continue;
        }
        let node = resolved(resolver, &node);
        let Some(node) = node.as_dictionary() else {
            continue;
        };
        if let Some(entries) = node.get(leaf) {
            let entries = resolved(resolver, entries);
            if let Some(entries) = entries.as_array() {
                for pair in entries.chunks(2) {
                    if let [key, value] = pair {
                        pairs.push((resolved(resolver, key).as_ref().clone(), value.clone()));
                    }
                }
            }
        }
        if let Some(kids) = node.get("Kids") {
            let kids = resolved(resolver, kids);
            // Pushed backwards, so the first kid is read first.
            for kid in kids
                .as_array()
                .unwrap_or_default()
                .iter()
                .take(MAX_NODES)
                .rev()
            {
                pending.push(kid.clone());
            }
        }
    }
    pairs
}

// The pairs of a name tree: the bytes of each key, and its value.
fn name_tree(resolver: &mut Resolver<'_>, root: &Object) -> Vec<(Vec<u8>, Object)> {
    tree(resolver, root, "Names")
        .into_iter()
        .filter_map(|(key, value)| Some((key.as_string()?.as_bytes().to_vec(), value)))
        .collect()
}

// `/Names /<key>` of the catalog, the root of a name tree.
fn names_entry(resolver: &mut Resolver<'_>, catalog: &Dictionary, key: &str) -> Option<Object> {
    let names = resolved(resolver, catalog.get("Names")?);
    names.as_dictionary()?.get(key).cloned()
}

/// A named destination: a name that links and actions can use.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedDestination {
    pub name: String,
    /// The page it goes to, counted from 0, if it is one of this file.
    pub page: Option<usize>,
}

/// The named destinations, from the catalog's `/Dests` and its name tree, in
/// that order; a name that is in both comes once.
pub fn read_named_destinations(
    resolver: &mut Resolver<'_>,
    catalog: &Dictionary,
    pages: &[Option<ObjectId>],
) -> Vec<NamedDestination> {
    let mut found: Vec<(String, Object)> = Vec::new();
    if let Some(dests) = catalog.get("Dests") {
        let dests = resolved(resolver, dests);
        if let Some(dests) = dests.as_dictionary() {
            for (name, value) in dests.iter().take(MAX_ITEMS) {
                let name = String::from_utf8_lossy(name.as_bytes()).into_owned();
                found.push((name, value.clone()));
            }
        }
    }
    if let Some(root) = names_entry(resolver, catalog, "Dests") {
        for (name, value) in name_tree(resolver, &root) {
            found.push((decode_text_string(&name), value));
        }
    }
    let mut names = HashSet::new();
    let mut destinations = Vec::new();
    for (name, value) in found {
        if !names.insert(name.clone()) {
            continue;
        }
        let value = resolved(resolver, &value);
        // The old kind of value is a dictionary with the array in `/D`.
        let array = match value.as_dictionary().and_then(|d| d.get("D")) {
            Some(inner) => inner.clone(),
            None => value.as_ref().clone(),
        };
        let page = destination_page(resolver, &array, pages, &Dictionary::new());
        destinations.push(NamedDestination { name, page });
    }
    destinations
}

/// The label of every page, as a viewer shows it in place of the number: `i`,
/// `ii`, `1`, `A-1`... `None` if the document has no page labels.
pub fn read_page_labels(
    resolver: &mut Resolver<'_>,
    catalog: &Dictionary,
    count: usize,
) -> Option<Vec<String>> {
    let root = catalog.get("PageLabels")?.clone();
    let mut ranges: Vec<(i64, Object)> = tree(resolver, &root, "Nums")
        .into_iter()
        .filter_map(|(key, value)| Some((key.as_integer().filter(|&k| k >= 0)?, value)))
        .collect();
    ranges.sort_by_key(|(start, _)| *start);
    if ranges.is_empty() {
        return None;
    }
    let count = count.min(MAX_ITEMS);
    let mut labels = Vec::with_capacity(count);
    let mut range = 0;
    for index in 0..count {
        let at = index as i64;
        while range + 1 < ranges.len() && ranges[range + 1].0 <= at {
            range += 1;
        }
        // Pages before the first range are numbered in decimal from 1.
        let label = if ranges[range].0 <= at {
            let (start, value) = &ranges[range];
            let value = resolved(resolver, value);
            let first = value
                .as_dictionary()
                .and_then(|dictionary| dictionary.get("St"))
                .and_then(|first| resolved(resolver, first).as_integer())
                .unwrap_or(1);
            let number = first.saturating_add(at - start);
            label_of(resolver, value.as_dictionary(), number)
        } else {
            (index + 1).to_string()
        };
        labels.push(label);
    }
    Some(labels)
}

fn label_of(resolver: &mut Resolver<'_>, range: Option<&Dictionary>, number: i64) -> String {
    let Some(range) = range else {
        return number.to_string();
    };
    let prefix = text_of(resolver, range, "P").unwrap_or_default();
    let style = range
        .get("S")
        .map(|style| resolved(resolver, style))
        .and_then(|style| style.as_name().map(|name| name.as_bytes().to_vec()));
    let counted = match style.as_deref() {
        Some(b"D") => number.to_string(),
        Some(b"R") => roman(number, true),
        Some(b"r") => roman(number, false),
        Some(b"A") => letters(number, true),
        Some(b"a") => letters(number, false),
        _ => String::new(),
    };
    format!("{prefix}{counted}")
}

// A number as Roman numerals; numbers that have none are in decimal.
fn roman(number: i64, upper: bool) -> String {
    if !(1..4000).contains(&number) {
        return number.to_string();
    }
    const VALUES: [(i64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut left = number;
    let mut text = String::new();
    for (value, digits) in VALUES {
        while left >= value {
            text.push_str(digits);
            left -= value;
        }
    }
    if upper { text.to_uppercase() } else { text }
}

// Letters as the standard numbers them: A to Z, then AA to ZZ, then AAA...
fn letters(number: i64, upper: bool) -> String {
    if number < 1 {
        return number.to_string();
    }
    let index = (number - 1) % 26;
    let repeats = ((number - 1) / 26 + 1).min(1000);
    let letter = char::from(b'a' + index as u8);
    let letter = if upper {
        letter.to_ascii_uppercase()
    } else {
        letter
    };
    std::iter::repeat_n(letter, repeats as usize).collect()
}

/// A file attached to the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    /// The name it has in the document's list of attachments.
    pub name: String,
    /// The file name, from `/UF` or `/F`.
    pub filename: Option<String>,
    pub description: Option<String>,
    /// The media type, such as `application/pdf`.
    pub mime_type: Option<String>,
    /// The size in bytes, as the file says it.
    pub size: Option<usize>,
    pub created: Option<Date>,
    pub modified: Option<Date>,
    /// The stream with its data, to read with `Resolver::stream_data`.
    pub stream: Option<ObjectId>,
}

/// The attached files of the document, from the name tree `/Names /EmbeddedFiles`.
pub fn read_attachments(resolver: &mut Resolver<'_>, catalog: &Dictionary) -> Vec<Attachment> {
    let Some(root) = names_entry(resolver, catalog, "EmbeddedFiles") else {
        return Vec::new();
    };
    let mut attachments = Vec::new();
    for (name, value) in name_tree(resolver, &root) {
        let value = resolved(resolver, &value);
        let Some(spec) = value.as_dictionary() else {
            continue;
        };
        let files = spec.get("EF").map(|files| resolved(resolver, files));
        let stream = files.as_ref().and_then(|files| {
            let files = files.as_dictionary()?;
            files.get("UF").or_else(|| files.get("F"))?.as_reference()
        });
        let object = stream.map(|id| resolved(resolver, &Object::Reference(id)));
        let stream_dictionary = object
            .as_ref()
            .and_then(|object| object.as_stream())
            .map(|stream| stream.dictionary());
        let mime_type = stream_dictionary
            .and_then(|dictionary| dictionary.get("Subtype"))
            .map(|subtype| resolved(resolver, subtype))
            .and_then(|subtype| {
                let name = subtype.as_name()?;
                Some(String::from_utf8_lossy(name.as_bytes()).into_owned())
            });
        let params = stream_dictionary
            .and_then(|dictionary| dictionary.get("Params"))
            .map(|params| resolved(resolver, params));
        let params = params.as_ref().and_then(|params| params.as_dictionary());
        let mut date = |key: &str| {
            let text = text_of(resolver, params?, key)?;
            parse_date(&text)
        };
        let created = date("CreationDate");
        let modified = date("ModDate");
        let size = params
            .and_then(|params| params.get("Size"))
            .and_then(|size| resolved(resolver, size).as_integer())
            .and_then(|size| usize::try_from(size).ok());
        attachments.push(Attachment {
            name: decode_text_string(&name),
            filename: text_of(resolver, spec, "UF").or_else(|| text_of(resolver, spec, "F")),
            description: text_of(resolver, spec, "Desc"),
            mime_type,
            size,
            created,
            modified,
            stream,
        });
    }
    attachments
}
