//! Writing what a reader uses to move around and describe a document: the
//! information dictionary, XMP, bookmarks, page labels and attached files; and
//! cropping, deleting and adding pages.

use std::collections::HashSet;

use super::{Editor, PageSlot, number, rectangle};
use crate::base::{Error, ErrorKind, Result};
use crate::crypto::Md5;
use crate::document::{Bookmark, Date, Rectangle};
use crate::filters::deflate_zlib;
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
use crate::writer::forms::text_string;

/// The most nodes of a name tree the editor reads when it adds an attachment.
const MAX_NODES: usize = 100_000;

/// A range of pages that are labeled alike: `i`, `ii`, `iii`, or `A-1`, `A-2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLabel {
    /// The first page of the range, from 0.
    pub page: usize,
    /// How the number is written: `D` decimal, `R` and `r` Roman numerals,
    /// `A` and `a` letters; `None` for no number, only the prefix.
    pub style: Option<char>,
    /// Text before the number.
    pub prefix: String,
    /// The number of the first page of the range, 1 or more.
    pub start: i64,
}

/// A file to attach to a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAttachment {
    /// The name in the document's list of attachments: attaching a file with a
    /// name that is there replaces it.
    pub name: String,
    /// The file name shown to the person who saves it.
    pub filename: String,
    pub data: Vec<u8>,
    pub description: Option<String>,
    /// The media type, such as `text/plain`.
    pub mime_type: Option<String>,
}

impl Editor<'_, '_> {
    /// Sets the text entry `key` of the information dictionary (`Title`,
    /// `Author`, `Subject`, `Keywords`, `Creator`, `Producer`...), or removes
    /// it if `value` is `None`. The dictionary is made if the document has none.
    pub fn set_info_text(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        self.change_info(key, value.map(text_string))
    }

    /// Sets the date entry `key` of the information dictionary (`CreationDate`
    /// or `ModDate`), or removes it if `value` is `None`.
    pub fn set_info_date(&mut self, key: &str, value: Option<&Date>) -> Result<()> {
        let date = value.map(|date| {
            Object::String(PdfString::new(
                date.to_pdf_string().into_bytes(),
                StringFormat::Literal,
            ))
        });
        self.change_info(key, date)
    }

    fn change_info(&mut self, key: &str, value: Option<Object>) -> Result<()> {
        let id = match self.info {
            Some(id) => id,
            None if value.is_none() => return Ok(()),
            None => {
                let id = self.add(Object::Dictionary(Dictionary::new()));
                self.info = Some(id);
                id
            }
        };
        let mut info = self
            .object(id)?
            .as_dictionary()
            .cloned()
            .unwrap_or_default();
        match value {
            Some(value) => {
                info.insert(Name::from(key), value);
            }
            None => {
                info.remove(key);
            }
        }
        self.set(id, Object::Dictionary(info));
        Ok(())
    }

    /// Sets the XMP metadata of the document to `xml`, or removes it if `None`.
    /// It is written as it is, not compressed, as the standard recommends.
    pub fn set_xmp(&mut self, xml: Option<&str>) -> Result<()> {
        let mut catalog = self.catalog_dictionary()?;
        let existing = catalog.get("Metadata").and_then(Object::as_reference);
        let Some(xml) = xml else {
            catalog.remove("Metadata");
            self.set(self.catalog, Object::Dictionary(catalog));
            return Ok(());
        };
        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), Object::Name(Name::from("Metadata")));
        dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("XML")));
        match existing {
            Some(id) => self.set_stream(id, dictionary, xml.as_bytes().to_vec()),
            None => {
                let id = self.add_stream(dictionary, xml.as_bytes().to_vec());
                catalog.insert(Name::from("Metadata"), Object::Reference(id));
                self.set(self.catalog, Object::Dictionary(catalog));
            }
        }
        Ok(())
    }

    // The object of page `index`, made an object of its own if the tree held
    // it directly.
    pub(super) fn page_object(&mut self, index: usize) -> Result<ObjectId> {
        let slot = *self
            .pages
            .get(index)
            .ok_or_else(|| Error::new(ErrorKind::PageNotFound, 0))?;
        match slot.id {
            Some(id) => Ok(id),
            None => {
                self.flatten_pages()?;
                Ok(self.pages[index].id.expect("flattened pages are objects"))
            }
        }
    }

    /// Replaces the bookmarks with `bookmarks`, in the order a viewer shows
    /// them, each followed by its children: `level` is 1 for the top ones, and
    /// can be at most one more than the bookmark before. An empty list removes
    /// the outline.
    ///
    /// A bookmark goes to its `page` (from 0), or opens its `uri`, or does
    /// neither. It is an `InvalidOutline` error if the levels don't follow
    /// that rule, and a `PageNotFound` error if a page is past the last.
    pub fn set_outlines(&mut self, bookmarks: &[Bookmark]) -> Result<()> {
        let mut previous = 0;
        for bookmark in bookmarks {
            if bookmark.level == 0 || bookmark.level > previous + 1 {
                return Err(Error::new(ErrorKind::InvalidOutline, 0));
            }
            if bookmark.page.is_some_and(|page| page >= self.pages.len()) {
                return Err(Error::new(ErrorKind::PageNotFound, 0));
            }
            previous = bookmark.level;
        }
        let mut catalog = self.catalog_dictionary()?;
        if bookmarks.is_empty() {
            catalog.remove("Outlines");
            self.set(self.catalog, Object::Dictionary(catalog));
            return Ok(());
        }
        let root = self.allocate();
        let ids: Vec<ObjectId> = bookmarks.iter().map(|_| self.allocate()).collect();
        // The parent of each bookmark, and the children of each (the last
        // entry is the root's).
        let mut parents = Vec::with_capacity(bookmarks.len());
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); bookmarks.len() + 1];
        let mut ancestors: Vec<usize> = Vec::new();
        let root_index = bookmarks.len();
        for (index, bookmark) in bookmarks.iter().enumerate() {
            ancestors.truncate(bookmark.level - 1);
            let parent = ancestors.last().copied().unwrap_or(root_index);
            parents.push(parent);
            children[parent].push(index);
            ancestors.push(index);
        }
        // What a viewer shows under each, when open.
        let mut visible = vec![0_i64; bookmarks.len() + 1];
        for index in (0..bookmarks.len()).rev() {
            let shown: i64 = children[index]
                .iter()
                .map(|&child| {
                    1 + if bookmarks[child].open {
                        visible[child]
                    } else {
                        0
                    }
                })
                .sum();
            visible[index] = shown;
        }
        visible[root_index] = children[root_index]
            .iter()
            .map(|&child| {
                1 + if bookmarks[child].open {
                    visible[child]
                } else {
                    0
                }
            })
            .sum();
        let reference = |index: usize| {
            if index == root_index {
                Object::Reference(root)
            } else {
                Object::Reference(ids[index])
            }
        };
        for (index, bookmark) in bookmarks.iter().enumerate() {
            let mut item = Dictionary::new();
            item.insert(Name::from("Title"), text_string(&bookmark.title));
            item.insert(Name::from("Parent"), reference(parents[index]));
            let siblings = &children[parents[index]];
            let at = siblings.iter().position(|&s| s == index).unwrap_or(0);
            if at > 0 {
                item.insert(Name::from("Prev"), reference(siblings[at - 1]));
            }
            if let Some(&next) = siblings.get(at + 1) {
                item.insert(Name::from("Next"), reference(next));
            }
            if let (Some(&first), Some(&last)) = (children[index].first(), children[index].last()) {
                item.insert(Name::from("First"), reference(first));
                item.insert(Name::from("Last"), reference(last));
                let count = if bookmark.open {
                    visible[index]
                } else {
                    -visible[index]
                };
                item.insert(Name::from("Count"), Object::Integer(count));
            }
            if let Some(page) = bookmark.page {
                let page = self.page_object(page)?;
                let destination = vec![
                    Object::Reference(page),
                    Object::Name(Name::from("XYZ")),
                    Object::Null,
                    Object::Null,
                    Object::Null,
                ];
                item.insert(Name::from("Dest"), Object::Array(destination));
            } else if let Some(uri) = &bookmark.uri {
                let mut action = Dictionary::new();
                action.insert(Name::from("S"), Object::Name(Name::from("URI")));
                action.insert(
                    Name::from("URI"),
                    Object::String(PdfString::new(
                        uri.as_bytes().to_vec(),
                        StringFormat::Literal,
                    )),
                );
                item.insert(Name::from("A"), Object::Dictionary(action));
            }
            self.set(ids[index], Object::Dictionary(item));
        }
        let mut outlines = Dictionary::new();
        outlines.insert(Name::from("Type"), Object::Name(Name::from("Outlines")));
        outlines.insert(Name::from("First"), reference(children[root_index][0]));
        let last = *children[root_index].last().expect("there is a bookmark");
        outlines.insert(Name::from("Last"), reference(last));
        outlines.insert(Name::from("Count"), Object::Integer(visible[root_index]));
        self.set(root, Object::Dictionary(outlines));
        catalog.insert(Name::from("Outlines"), Object::Reference(root));
        self.set(self.catalog, Object::Dictionary(catalog));
        Ok(())
    }

    /// Replaces the page labels with `labels`. An empty list removes them.
    ///
    /// The first range must start at page 0 and the ranges must be in order,
    /// each page past the one before and before the end: otherwise it is an
    /// `InvalidPageLabels` error.
    pub fn set_page_labels(&mut self, labels: &[PageLabel]) -> Result<()> {
        let mut catalog = self.catalog_dictionary()?;
        if labels.is_empty() {
            catalog.remove("PageLabels");
            self.set(self.catalog, Object::Dictionary(catalog));
            return Ok(());
        }
        let valid_style =
            |style: Option<char>| matches!(style, None | Some('D' | 'R' | 'r' | 'A' | 'a'));
        let ordered = labels.windows(2).all(|pair| pair[0].page < pair[1].page);
        if labels[0].page != 0
            || !ordered
            || labels.iter().any(|label| {
                label.page >= self.pages.len() || label.start < 1 || !valid_style(label.style)
            })
        {
            return Err(Error::new(ErrorKind::InvalidPageLabels, 0));
        }
        let mut numbers = Vec::new();
        for label in labels {
            let mut range = Dictionary::new();
            if let Some(style) = label.style {
                let mut name = [0; 4];
                let name = style.encode_utf8(&mut name);
                range.insert(Name::from("S"), Object::Name(Name::from(&*name)));
            }
            if !label.prefix.is_empty() {
                range.insert(Name::from("P"), text_string(&label.prefix));
            }
            if label.start != 1 {
                range.insert(Name::from("St"), Object::Integer(label.start));
            }
            numbers.push(Object::Integer(i64::try_from(label.page).unwrap_or(0)));
            numbers.push(Object::Dictionary(range));
        }
        let mut tree = Dictionary::new();
        tree.insert(Name::from("Nums"), Object::Array(numbers));
        catalog.insert(Name::from("PageLabels"), Object::Dictionary(tree));
        self.set(self.catalog, Object::Dictionary(catalog));
        Ok(())
    }

    // The entries of the name tree of the catalog's `/Names /<key>`, in a flat
    // list, with each key as its bytes.
    fn name_entries(&mut self, key: &str) -> Result<Vec<(Vec<u8>, Object)>> {
        let names = self.names_dictionary()?;
        let Some(root) = names.get(key).cloned() else {
            return Ok(Vec::new());
        };
        let mut entries = Vec::new();
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        let mut visited = 0;
        while let Some(node) = pending.pop() {
            visited += 1;
            if visited > MAX_NODES {
                break;
            }
            let node = match node {
                Object::Reference(id) => {
                    if !seen.insert(id.number) {
                        continue;
                    }
                    self.object(id)?
                }
                other => other,
            };
            let Some(node) = node.as_dictionary() else {
                continue;
            };
            let leaf = match node.get("Names") {
                Some(Object::Reference(id)) => self.object(*id)?,
                Some(other) => other.clone(),
                None => Object::Null,
            };
            if let Some(items) = leaf.as_array() {
                for pair in items.chunks(2) {
                    if let [name, value] = pair {
                        let name = match name {
                            Object::Reference(id) => self.object(*id)?,
                            other => other.clone(),
                        };
                        if let Some(name) = name.as_string() {
                            entries.push((name.as_bytes().to_vec(), value.clone()));
                        }
                    }
                }
            }
            let kids = match node.get("Kids") {
                Some(Object::Reference(id)) => self.object(*id)?,
                Some(other) => other.clone(),
                None => Object::Null,
            };
            if let Some(kids) = kids.as_array() {
                pending.extend(kids.iter().rev().take(MAX_NODES).cloned());
            }
        }
        Ok(entries)
    }

    // The catalog's `/Names` dictionary, empty if it has none.
    fn names_dictionary(&mut self) -> Result<Dictionary> {
        let catalog = self.catalog_dictionary()?;
        Ok(match catalog.get("Names") {
            Some(Object::Reference(id)) => self
                .object(*id)?
                .as_dictionary()
                .cloned()
                .unwrap_or_default(),
            Some(Object::Dictionary(names)) => names.clone(),
            _ => Dictionary::new(),
        })
    }

    // Writes the entries as a flat name tree under `/Names /<key>`, in the
    // order the standard asks for, or removes the tree if there are none.
    fn set_name_entries(&mut self, key: &str, mut entries: Vec<(Vec<u8>, Object)>) -> Result<()> {
        let mut names = self.names_dictionary()?;
        if entries.is_empty() {
            names.remove(key);
        } else {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut items = Vec::new();
            for (name, value) in entries {
                items.push(Object::String(PdfString::new(name, StringFormat::Literal)));
                items.push(value);
            }
            let mut tree = Dictionary::new();
            tree.insert(Name::from("Names"), Object::Array(items));
            names.insert(Name::from(key), Object::Dictionary(tree));
        }
        let mut catalog = self.catalog_dictionary()?;
        match catalog.get("Names") {
            Some(Object::Reference(id)) => {
                let id = *id;
                self.set(id, Object::Dictionary(names));
            }
            _ if names.is_empty() => {
                catalog.remove("Names");
                self.set(self.catalog, Object::Dictionary(catalog));
            }
            _ => {
                catalog.insert(Name::from("Names"), Object::Dictionary(names));
                self.set(self.catalog, Object::Dictionary(catalog));
            }
        }
        Ok(())
    }

    /// Attaches a file to the document. One with the same name is replaced.
    /// It is an `InvalidAttachment` error if the name is empty.
    pub fn add_attachment(&mut self, attachment: &NewAttachment) -> Result<()> {
        if attachment.name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidAttachment, 0));
        }
        let mut params = Dictionary::new();
        params.insert(
            Name::from("Size"),
            Object::Integer(i64::try_from(attachment.data.len()).unwrap_or(i64::MAX)),
        );
        params.insert(
            Name::from("CheckSum"),
            Object::String(PdfString::new(
                Md5::digest(&attachment.data).to_vec(),
                StringFormat::Hexadecimal,
            )),
        );
        let mut stream = Dictionary::new();
        stream.insert(Name::from("Type"), Object::Name(Name::from("EmbeddedFile")));
        if let Some(mime) = &attachment.mime_type {
            stream.insert(
                Name::from("Subtype"),
                Object::Name(Name::from(mime.as_str())),
            );
        }
        stream.insert(Name::from("Params"), Object::Dictionary(params));
        stream.insert(
            Name::from("Filter"),
            Object::Name(Name::from("FlateDecode")),
        );
        let data = deflate_zlib(&attachment.data, 6);
        let stream = self.add_stream(stream, data);
        let mut files = Dictionary::new();
        files.insert(Name::from("F"), Object::Reference(stream));
        files.insert(Name::from("UF"), Object::Reference(stream));
        let mut spec = Dictionary::new();
        spec.insert(Name::from("Type"), Object::Name(Name::from("Filespec")));
        spec.insert(Name::from("F"), text_string(&attachment.filename));
        spec.insert(Name::from("UF"), text_string(&attachment.filename));
        if let Some(description) = &attachment.description {
            spec.insert(Name::from("Desc"), text_string(description));
        }
        spec.insert(Name::from("EF"), Object::Dictionary(files));
        let spec = self.add(Object::Dictionary(spec));

        let mut entries = self.name_entries("EmbeddedFiles")?;
        // The name as it is written in the tree: its bytes as `text_string` gives.
        let key = match text_string(&attachment.name) {
            Object::String(name) => name.as_bytes().to_vec(),
            _ => Vec::new(),
        };
        entries.retain(|(name, _)| *name != key);
        entries.push((key, Object::Reference(spec)));
        self.set_name_entries("EmbeddedFiles", entries)
    }

    /// Removes the attachment called `name`, and says whether there was one.
    pub fn remove_attachment(&mut self, name: &str) -> Result<bool> {
        let key = match text_string(name) {
            Object::String(name) => name.as_bytes().to_vec(),
            _ => Vec::new(),
        };
        let mut entries = self.name_entries("EmbeddedFiles")?;
        let before = entries.len();
        entries.retain(|(entry, _)| *entry != key);
        if entries.len() == before {
            return Ok(false);
        }
        self.set_name_entries("EmbeddedFiles", entries)?;
        Ok(true)
    }

    /// Crops page `index` to `area`, in default user space: its `/CropBox`.
    /// It is a `PageNotFound` error past the last page, and an `InvalidBox`
    /// error if `area` has no width or height.
    pub fn crop_page(&mut self, index: usize, area: Rectangle) -> Result<()> {
        let valid = [area.left, area.bottom, area.right, area.top]
            .iter()
            .all(|value| value.is_finite())
            && area.right > area.left
            && area.top > area.bottom;
        if !valid {
            return Err(Error::new(ErrorKind::InvalidBox, 0));
        }
        let id = self.page_object(index)?;
        let mut page = self.page_dictionary(id)?;
        page.insert(Name::from("CropBox"), rectangle(area));
        self.set(id, Object::Dictionary(page));
        Ok(())
    }

    /// Removes the pages at `indexes`; the others stay in order. What
    /// [`select_pages`](Self::select_pages) says about the outline, forms and
    /// destinations applies. It is a `PageNotFound` error if an index is past
    /// the last page, and a `NoPagesLeft` error if every page would go.
    pub fn delete_pages(&mut self, indexes: &[usize]) -> Result<()> {
        if indexes.iter().any(|&index| index >= self.pages.len()) {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        let removed: HashSet<usize> = indexes.iter().copied().collect();
        let keep: Vec<usize> = (0..self.pages.len())
            .filter(|index| !removed.contains(index))
            .collect();
        if keep.is_empty() {
            return Err(Error::new(ErrorKind::NoPagesLeft, 0));
        }
        self.select_pages(&keep)
    }

    /// Adds an empty page of `width` by `height` points before page `index`;
    /// `index` can be the number of pages, to add it at the end. It is a
    /// `PageNotFound` error past that, and an `InvalidBox` error if the size
    /// isn't positive. Page labels, which count pages by place, are removed
    /// if the page isn't added at the end.
    pub fn insert_blank_page(&mut self, index: usize, width: f64, height: f64) -> Result<()> {
        if index > self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(Error::new(ErrorKind::InvalidBox, 0));
        }
        let root = match self.page_root {
            Some(root) => root,
            None => {
                self.flatten_pages()?;
                self.page_root.expect("flattening makes a root")
            }
        };
        let mut page = Dictionary::new();
        page.insert(Name::from("Type"), Object::Name(Name::from("Page")));
        page.insert(Name::from("Parent"), Object::Reference(root));
        page.insert(
            Name::from("MediaBox"),
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                number(width),
                number(height),
            ]),
        );
        page.insert(
            Name::from("Resources"),
            Object::Dictionary(Dictionary::new()),
        );
        let id = self.add(Object::Dictionary(page));
        self.pages.insert(
            index,
            PageSlot {
                id: Some(id),
                source: None,
            },
        );
        self.write_page_tree(root)?;
        if index + 1 < self.pages.len() {
            let mut catalog = self.catalog_dictionary()?;
            if catalog.remove("PageLabels").is_some() {
                self.set(self.catalog, Object::Dictionary(catalog));
            }
        }
        Ok(())
    }
}
