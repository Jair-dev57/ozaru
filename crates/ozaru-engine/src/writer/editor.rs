use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use md5::{Digest, Md5};

use super::file::{FileWriter, Layout};
use crate::base::{DiagnosticKind, Error, ErrorKind, Result};
use crate::document::{Document, Rectangle};
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, Stream, StringFormat};
use crate::xref::{XrefEntry, find_startxref};

/// Changes to an open [`Document`], kept apart from it until they are saved.
///
/// The document isn't changed: the editor holds the objects that were changed
/// or added, and reads the others from the document as they are. Saving writes
/// a new file with both.
///
/// Pages are numbered from 0, in the order the editor has them, which is the
/// document's until pages are moved.
///
/// An encrypted document is saved decrypted: its password was needed to open
/// it, and the file written has no `/Encrypt`.
#[derive(Debug)]
pub struct Editor<'d, 'a> {
    document: &'d mut Document<'a>,
    edits: HashMap<ObjectId, Edit>,
    next_number: u32,
    catalog: ObjectId,
    info: Option<ObjectId>,
    pages: Vec<PageSlot>,
    // The root of the page tree the editor made, once it made one.
    page_root: Option<ObjectId>,
    // Pages left out of the document: references to them are written as null.
    dropped: HashSet<ObjectId>,
    // The newest PDF version of the documents pages were imported from, and
    // whether any of them had object streams.
    imported_version: Option<(u8, u8)>,
    imported_object_streams: bool,
}

// An object changed or added by the editor.
#[derive(Debug, Clone)]
enum Edit {
    Object(Object),
    // A stream: its dictionary and its data, encoded by its filters.
    Stream(Dictionary, Arc<Vec<u8>>),
}

// An object ready to write: a stream as its dictionary and its data.
type Loaded = (Object, Option<Arc<Vec<u8>>>);

// A page of the editor: its object, and the page of the document it comes from.
#[derive(Debug, Clone, Copy)]
struct PageSlot {
    id: Option<ObjectId>,
    source: Option<usize>,
}

// The entries a page inherits from its ancestors (ISO 32000-2, section 7.7.3.4).
const INHERITED: [&str; 4] = ["MediaBox", "CropBox", "Rotate", "Resources"];

impl<'d, 'a> Editor<'d, 'a> {
    /// An editor of `document`, with no changes yet.
    ///
    /// If the page tree is damaged, the editor starts with a new one, made from
    /// the pages the engine read, so the file saved has the same pages, in a
    /// tree any reader can follow. It is a `PasswordRequired` error if the
    /// document is still locked.
    pub fn new(document: &'d mut Document<'a>) -> Result<Self> {
        if document.needs_password() {
            return Err(Error::new(ErrorKind::PasswordRequired, 0));
        }
        let resolver = document.resolver();
        let highest = resolver.table().iter().map(|(number, _)| number).max();
        let size = resolver
            .trailer()
            .get("Size")
            .and_then(Object::as_integer)
            .and_then(|size| u32::try_from(size).ok())
            .unwrap_or(0);
        let next_number = highest.map_or(1, |n| n.saturating_add(1)).max(size).max(1);
        let pages = (0..document.page_count())
            .map(|index| PageSlot {
                id: document.page(index).and_then(|page| page.id()),
                source: Some(index),
            })
            .collect();
        let damaged = tree_is_damaged(document);
        let catalog_id = document.catalog().id();
        let catalog_dictionary = document.catalog().dictionary().clone();
        let info = document.resolver().trailer().get("Info").cloned();

        let mut editor = Self {
            document,
            edits: HashMap::new(),
            next_number,
            catalog: ObjectId::new(0, 0),
            info: None,
            pages,
            page_root: None,
            dropped: HashSet::new(),
            imported_version: None,
            imported_object_streams: false,
        };
        editor.catalog = match catalog_id {
            Some(id) => id,
            None => editor.add(Object::Dictionary(catalog_dictionary)),
        };
        // The information dictionary, an object of its own even if the
        // trailer holds it directly.
        editor.info = match info {
            Some(Object::Reference(id)) => match editor.object(id) {
                Ok(Object::Dictionary(_)) => Some(id),
                _ => None,
            },
            Some(Object::Dictionary(info)) => Some(editor.add(Object::Dictionary(info))),
            _ => None,
        };
        if damaged {
            editor.flatten_pages()?;
        } else {
            editor.complete_pages()?;
        }
        Ok(editor)
    }

    /// The number of pages.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The catalog's object.
    pub fn catalog(&self) -> ObjectId {
        self.catalog
    }

    /// The information dictionary's object, if the document has one.
    pub fn info(&self) -> Option<ObjectId> {
        self.info
    }

    /// The object `id` refers to, with the editor's changes. A stream the
    /// editor changed or added has its dictionary, but no data in the file:
    /// its data is in [`stream_data`](Self::stream_data).
    pub fn object(&mut self, id: ObjectId) -> Result<Object> {
        match self.edits.get(&id) {
            Some(Edit::Object(object)) => Ok(object.clone()),
            Some(Edit::Stream(dictionary, _)) => {
                Ok(Object::Stream(Stream::new(dictionary.clone(), 0..0)))
            }
            None => Ok(self.document.resolver().resolve(id)?.as_ref().clone()),
        }
    }

    /// The data of stream `id`, as its filters encoded it, decrypted.
    pub fn stream_data(&mut self, id: ObjectId) -> Result<Vec<u8>> {
        match self.edits.get(&id) {
            Some(Edit::Stream(_, data)) => Ok(data.as_ref().clone()),
            Some(Edit::Object(_)) => Err(Error::new(ErrorKind::NotAStream, 0)),
            None => Ok(self.document.resolver().decrypted_stream(id)?.1),
        }
    }

    /// Replaces object `id` with `object`, which must not be a stream: see
    /// [`set_stream`](Self::set_stream).
    pub fn set(&mut self, id: ObjectId, object: Object) {
        self.edits.insert(id, Edit::Object(object));
    }

    /// Replaces object `id` with a stream of `dictionary` and `data`, already
    /// encoded by the filters `dictionary` names. Its `/Length` is set when it
    /// is written.
    pub fn set_stream(&mut self, id: ObjectId, dictionary: Dictionary, data: Vec<u8>) {
        self.edits
            .insert(id, Edit::Stream(dictionary, Arc::new(data)));
    }

    /// Adds `object` as a new indirect object, and returns its reference.
    pub fn add(&mut self, object: Object) -> ObjectId {
        let id = self.allocate();
        self.set(id, object);
        id
    }

    /// Adds a new stream, as [`set_stream`](Self::set_stream) does.
    pub fn add_stream(&mut self, dictionary: Dictionary, data: Vec<u8>) -> ObjectId {
        let id = self.allocate();
        self.set_stream(id, dictionary, data);
        id
    }

    fn allocate(&mut self) -> ObjectId {
        let id = ObjectId::new(self.next_number, 0);
        self.next_number = self.next_number.saturating_add(1);
        id
    }

    /// Turns page `index` clockwise by `degrees`, which may be negative: its
    /// `/Rotate` becomes its rotation plus `degrees`, from 0 to 270.
    ///
    /// It is a `PageNotFound` error past the last page, and an
    /// `InvalidRotation` error if `degrees` isn't a multiple of 90.
    pub fn rotate_page(&mut self, index: usize, degrees: i64) -> Result<()> {
        if degrees % 90 != 0 {
            return Err(Error::new(ErrorKind::InvalidRotation, 0));
        }
        let slot = *self
            .pages
            .get(index)
            .ok_or_else(|| Error::new(ErrorKind::PageNotFound, 0))?;
        let id = match slot.id {
            Some(id) => id,
            // A page held directly by its parent can't change alone.
            None => {
                self.flatten_pages()?;
                self.pages[index].id.expect("flattened pages are objects")
            }
        };
        let mut dictionary = self.page_dictionary(id)?;
        let current = if self.edits.contains_key(&id) {
            dictionary
                .get("Rotate")
                .and_then(Object::as_integer)
                .unwrap_or(0)
        } else {
            slot.source
                .and_then(|source| self.document.page(source))
                .map_or(0, |page| i64::from(page.rotation()))
        };
        let rotation = (current + degrees % 360).rem_euclid(360);
        dictionary.insert(Name::from("Rotate"), Object::Integer(rotation));
        self.set(id, Object::Dictionary(dictionary));
        Ok(())
    }

    // Gives the pages that lack them the entries every page needs (ISO
    // 32000-2, section 7.7.3.3): the media box the engine used, US Letter if
    // none was found, and empty resources.
    fn complete_pages(&mut self) -> Result<()> {
        for index in 0..self.pages.len() {
            let slot = self.pages[index];
            let (Some(id), Some(page)) = (slot.id, slot.source.and_then(|s| self.document.page(s)))
            else {
                continue;
            };
            let media_box = page
                .attribute("MediaBox")
                .is_none()
                .then(|| rectangle(page.media_box()));
            let resources = page.attribute("Resources").is_none();
            if media_box.is_none() && !resources {
                continue;
            }
            let mut dictionary = self.page_dictionary(id)?;
            if let Some(media_box) = media_box {
                dictionary.insert(Name::from("MediaBox"), media_box);
            }
            if resources {
                dictionary.insert(
                    Name::from("Resources"),
                    Object::Dictionary(Dictionary::new()),
                );
            }
            self.set(id, Object::Dictionary(dictionary));
        }
        Ok(())
    }

    // The dictionary of page `id`.
    fn page_dictionary(&mut self, id: ObjectId) -> Result<Dictionary> {
        let object = self.object(id)?;
        object
            .as_dictionary()
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::InvalidPageTree, 0))
    }

    /// Replaces the page tree with a new one, with every page as a kid of its
    /// root, in the editor's order.
    ///
    /// Each page gets the attributes it inherited as its own entries, so it
    /// looks the same without its old ancestors, and becomes an indirect
    /// object if its parent held it directly. A damaged page becomes an empty
    /// page of its size.
    fn flatten_pages(&mut self) -> Result<()> {
        let root = self.allocate();
        for index in 0..self.pages.len() {
            let slot = self.pages[index];
            let source = slot
                .source
                .and_then(|source| self.document.page(source))
                .cloned();
            let damaged = source.as_ref().is_some_and(|page| page.is_damaged());
            let mut dictionary = match (slot.id, &source) {
                (Some(id), _) if !damaged || self.edits.contains_key(&id) => {
                    self.page_dictionary(id)?
                }
                (_, Some(page)) => page.dictionary().clone(),
                (_, None) => Dictionary::new(),
            };
            if let Some(page) = &source {
                for key in INHERITED {
                    if !dictionary.contains_key(key)
                        && let Some(value) = page.attribute(key)
                    {
                        dictionary.insert(Name::from(key), value.clone());
                    }
                }
                if !dictionary.contains_key("MediaBox") {
                    let media_box = rectangle(page.media_box());
                    dictionary.insert(Name::from("MediaBox"), media_box);
                }
            }
            if !dictionary.contains_key("Resources") {
                dictionary.insert(
                    Name::from("Resources"),
                    Object::Dictionary(Dictionary::new()),
                );
            }
            dictionary.insert(Name::from("Type"), Object::Name(Name::from("Page")));
            dictionary.insert(Name::from("Parent"), Object::Reference(root));
            let id = match slot.id {
                Some(id) if !damaged => id,
                _ => self.allocate(),
            };
            self.set(id, Object::Dictionary(dictionary));
            self.pages[index].id = Some(id);
        }
        self.page_root = Some(root);
        self.write_page_tree(root)?;

        let mut catalog = self.catalog_dictionary()?;
        catalog.insert(Name::from("Pages"), Object::Reference(root));
        self.set(self.catalog, Object::Dictionary(catalog));
        Ok(())
    }

    // Writes the root of a flat page tree: the editor's pages, in order.
    fn write_page_tree(&mut self, root: ObjectId) -> Result<()> {
        let kids: Vec<Object> = self
            .pages
            .iter()
            .filter_map(|slot| slot.id.map(Object::Reference))
            .collect();
        let count = i64::try_from(kids.len()).unwrap_or(i64::MAX);
        let mut tree = Dictionary::new();
        tree.insert(Name::from("Type"), Object::Name(Name::from("Pages")));
        tree.insert(Name::from("Kids"), Object::Array(kids));
        tree.insert(Name::from("Count"), Object::Integer(count));
        self.set(root, Object::Dictionary(tree));
        Ok(())
    }

    fn catalog_dictionary(&mut self) -> Result<Dictionary> {
        Ok(match self.object(self.catalog)? {
            Object::Dictionary(catalog) => catalog,
            _ => self.document.catalog().dictionary().clone(),
        })
    }

    /// Keeps only the pages at `indexes`, in that order: pages can be left
    /// out, moved and repeated. A page repeated is copied, sharing its
    /// content and resources with the first.
    ///
    /// Pages left out aren't written when saving, nor what only they use, and
    /// references to them become null. The document-wide entries of the
    /// catalog that point at pages are removed, as they would point at the
    /// wrong ones: the outline, the forms, the named destinations, the
    /// structure tree, the page labels, the article threads and an opening
    /// destination.
    ///
    /// It is a `PageNotFound` error if an index is past the last page; the
    /// pages are then left as they were.
    pub fn select_pages(&mut self, indexes: &[usize]) -> Result<()> {
        if indexes.iter().any(|&index| index >= self.pages.len()) {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        let root = match self.page_root {
            Some(root) => root,
            None => {
                self.flatten_pages()?;
                self.page_root.expect("flattening makes a root")
            }
        };
        let old = std::mem::take(&mut self.pages);
        let mut used = HashSet::new();
        for &index in indexes {
            let slot = old[index];
            let id = slot.id.expect("flattened pages are objects");
            let id = if used.insert(id) {
                id
            } else {
                let copy = self.page_dictionary(id)?;
                self.add(Object::Dictionary(copy))
            };
            self.dropped.remove(&id);
            self.pages.push(PageSlot {
                id: Some(id),
                source: slot.source,
            });
        }
        for slot in old {
            let id = slot.id.expect("flattened pages are objects");
            if !used.contains(&id) {
                self.dropped.insert(id);
            }
        }
        self.write_page_tree(root)?;
        self.remove_page_references()
    }

    /// Adds the pages at `indexes` of `other`, another document, after the
    /// editor's pages, in that order, with everything they use: their
    /// contents, fonts, images and annotations, copied as new objects. An
    /// object several of them use is copied once.
    ///
    /// Each page gets the attributes it inherited as its own entries.
    /// References to pages of `other` that aren't imported become null, and
    /// the entries of the catalog that point at pages are removed, as
    /// [`select_pages`](Self::select_pages) does. A damaged page becomes an
    /// empty page of its size. Pages imported twice are copied twice.
    ///
    /// It is a `PageNotFound` error if an index is past the last page of
    /// `other`, and a `PasswordRequired` error if `other` is locked; the
    /// editor is then left as it was.
    pub fn import_pages(&mut self, other: &mut Document<'_>, indexes: &[usize]) -> Result<()> {
        if other.needs_password() {
            return Err(Error::new(ErrorKind::PasswordRequired, 0));
        }
        if indexes.iter().any(|&index| index >= other.page_count()) {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        if self.page_root.is_none() {
            self.flatten_pages()?;
        }
        let root = self.page_root.expect("flattening makes a root");

        // The pages of `other` that are imported, each the first time, get
        // their new objects first, so references between them follow.
        let mut copies: HashMap<ObjectId, ObjectId> = HashMap::new();
        let other_pages: HashSet<ObjectId> =
            other.pages().iter().filter_map(|page| page.id()).collect();
        let mut new_pages = Vec::with_capacity(indexes.len());
        for &index in indexes {
            let page = other.page(index).expect("checked above").clone();
            let id = self.allocate();
            if let Some(original) = page.id()
                && !page.is_damaged()
            {
                copies.entry(original).or_insert(id);
            }
            new_pages.push((id, page));
        }

        let mut pending: Vec<(ObjectId, ObjectId)> = Vec::new();
        let mut imported = Vec::with_capacity(new_pages.len());
        for (id, page) in new_pages {
            let mut dictionary = if page.is_damaged() {
                Dictionary::new()
            } else {
                page.dictionary().clone()
            };
            for key in INHERITED {
                if !dictionary.contains_key(key)
                    && let Some(value) = page.attribute(key)
                {
                    dictionary.insert(Name::from(key), value.clone());
                }
            }
            if !dictionary.contains_key("MediaBox") {
                dictionary.insert(Name::from("MediaBox"), rectangle(page.media_box()));
            }
            if !dictionary.contains_key("Resources") {
                dictionary.insert(
                    Name::from("Resources"),
                    Object::Dictionary(Dictionary::new()),
                );
            }
            dictionary.remove("Parent");
            dictionary.remove("B");
            let mut object = Object::Dictionary(dictionary);
            self.copy_references(&mut object, &mut copies, &other_pages, &mut pending);
            let Object::Dictionary(mut dictionary) = object else {
                unreachable!("a page stays a dictionary");
            };
            dictionary.insert(Name::from("Type"), Object::Name(Name::from("Page")));
            dictionary.insert(Name::from("Parent"), Object::Reference(root));
            self.set(id, Object::Dictionary(dictionary));
            imported.push(PageSlot {
                id: Some(id),
                source: None,
            });
        }
        // Then everything they use, each object once. A page tree node isn't
        // copied: its kids are pages of the other document.
        while let Some((original, id)) = pending.pop() {
            let object = match other.resolver().resolve(original) {
                Ok(object) => object,
                Err(error) if error.kind() == ErrorKind::PasswordRequired => return Err(error),
                Err(_) => Arc::new(Object::Null),
            };
            let is_tree_node = object.as_dictionary().is_some_and(|dictionary| {
                dictionary
                    .get("Type")
                    .and_then(Object::as_name)
                    .is_some_and(|kind| kind.as_bytes() == b"Pages")
            });
            if is_tree_node {
                self.set(id, Object::Null);
                continue;
            }
            match object.as_ref() {
                Object::Stream(stream) => {
                    let mut dictionary = Object::Dictionary(stream.dictionary().clone());
                    self.copy_references(&mut dictionary, &mut copies, &other_pages, &mut pending);
                    let Object::Dictionary(mut dictionary) = dictionary else {
                        unreachable!("a stream dictionary stays a dictionary");
                    };
                    dictionary.remove("Length");
                    let data = match other.resolver().decrypted_stream(original) {
                        Ok((_, data)) => data,
                        Err(error) if error.kind() == ErrorKind::PasswordRequired => {
                            return Err(error);
                        }
                        Err(_) => {
                            for key in ["Filter", "DecodeParms", "DL"] {
                                dictionary.remove(key);
                            }
                            Vec::new()
                        }
                    };
                    self.set_stream(id, dictionary, data);
                }
                object => {
                    let mut object = object.clone();
                    self.copy_references(&mut object, &mut copies, &other_pages, &mut pending);
                    self.set(id, object);
                }
            }
        }

        self.imported_version = self.imported_version.max(file_version(other));
        self.imported_object_streams |= uses_object_streams(other);
        self.pages.extend(imported);
        self.write_page_tree(root)?;
        self.remove_page_references()
    }

    // Replaces each reference of `object`, to an object of another document,
    // with a reference to its copy. An object not copied yet gets its new
    // object now, in `copies`, and waits in `pending` to be copied, so it is
    // copied once even if it refers back to itself. References to pages not
    // imported become null.
    fn copy_references(
        &mut self,
        object: &mut Object,
        copies: &mut HashMap<ObjectId, ObjectId>,
        other_pages: &HashSet<ObjectId>,
        pending: &mut Vec<(ObjectId, ObjectId)>,
    ) {
        match object {
            Object::Reference(original) => {
                *object = match copies.get(original) {
                    Some(&copy) => Object::Reference(copy),
                    None if other_pages.contains(original) => Object::Null,
                    None => {
                        let id = self.allocate();
                        copies.insert(*original, id);
                        pending.push((*original, id));
                        Object::Reference(id)
                    }
                };
            }
            Object::Array(items) => {
                for item in items {
                    self.copy_references(item, copies, other_pages, pending);
                }
            }
            Object::Dictionary(dictionary) => {
                for value in dictionary.values_mut() {
                    self.copy_references(value, copies, other_pages, pending);
                }
            }
            Object::Stream(stream) => {
                for value in stream.dictionary_mut().values_mut() {
                    self.copy_references(value, copies, other_pages, pending);
                }
            }
            _ => {}
        }
    }

    // Removes the entries that point at pages by their place or object, which
    // would point at the wrong pages or at none.
    fn remove_page_references(&mut self) -> Result<()> {
        let mut catalog = self.catalog_dictionary()?;
        for key in [
            "Outlines",
            "AcroForm",
            "Dests",
            "StructTreeRoot",
            "MarkInfo",
            "PageLabels",
            "Threads",
        ] {
            catalog.remove(key);
        }
        // An opening action that isn't a destination, such as JavaScript,
        // stays.
        let opens_a_page = match catalog.get("OpenAction") {
            Some(Object::Array(_)) => true,
            Some(Object::Dictionary(action)) => action
                .get("S")
                .and_then(Object::as_name)
                .is_some_and(|kind| kind.as_bytes() == b"GoTo"),
            _ => false,
        };
        if opens_a_page {
            catalog.remove("OpenAction");
        }
        if catalog
            .get("PageMode")
            .and_then(Object::as_name)
            .is_some_and(|mode| mode.as_bytes() == b"UseOutlines")
        {
            catalog.remove("PageMode");
        }
        // The name tree of destinations; embedded files and scripts stay.
        let names = match catalog.get("Names") {
            Some(Object::Reference(id)) => self.object(*id)?.as_dictionary().cloned(),
            Some(Object::Dictionary(names)) => Some(names.clone()),
            _ => None,
        };
        if let Some(mut names) = names
            && names.remove("Dests").is_some()
        {
            catalog.insert(Name::from("Names"), Object::Dictionary(names));
        }
        self.set(self.catalog, Object::Dictionary(catalog));

        // The beads of article threads.
        for index in 0..self.pages.len() {
            let Some(id) = self.pages[index].id else {
                continue;
            };
            let mut page = self.page_dictionary(id)?;
            if page.remove("B").is_some() {
                self.set(id, Object::Dictionary(page));
            }
        }
        Ok(())
    }

    /// Writes the document, with the editor's changes, as a new, complete
    /// file (ISO 32000-2, section 7.5).
    ///
    /// Only the objects the document uses are written, the ones reached from
    /// its catalog and information dictionary, numbered again from 1. Streams
    /// keep their data as it is, still compressed. If the original packed its
    /// objects into object streams, the new file does too, with a
    /// cross-reference stream; otherwise it has a classic table. An object that can't be read
    /// is written as null, and a stream whose data can't be read, empty.
    ///
    /// It is a `PasswordRequired` error if an object can't be decrypted.
    pub fn save(&mut self) -> Result<Vec<u8>> {
        // The file is written as the original was: with object streams if it
        // had them, which needs PDF 1.5.
        let layout = if uses_object_streams(self.document) || self.imported_object_streams {
            Layout::Compressed
        } else {
            Layout::Classic
        };
        let mut version = file_version(self.document)
            .max(self.imported_version)
            .unwrap_or((1, 7));
        if layout == Layout::Compressed {
            version = version.max((1, 5));
        }
        let version = format!("{}.{}", version.0, version.1);
        let info = self.info;

        // Every object reached from the roots, in the order found.
        let mut numbers: HashMap<ObjectId, u32> = HashMap::new();
        let mut loaded: Vec<Loaded> = Vec::new();
        let mut seen: HashSet<ObjectId> = HashSet::new();
        let mut queue: VecDeque<ObjectId> = VecDeque::new();
        for root in std::iter::once(self.catalog).chain(info) {
            if seen.insert(root) {
                queue.push_back(root);
            }
        }
        while let Some(id) = queue.pop_front() {
            let Some((object, data)) = self.load(id)? else {
                continue;
            };
            let mut found = Vec::new();
            references(&object, &mut found);
            for reference in found {
                if !self.dropped.contains(&reference) && seen.insert(reference) {
                    queue.push_back(reference);
                }
            }
            let number = u32::try_from(loaded.len() + 1).unwrap_or(u32::MAX);
            numbers.insert(id, number);
            loaded.push((object, data));
        }

        let mut file = FileWriter::new(&version, layout);
        for (index, (mut object, data)) in loaded.into_iter().enumerate() {
            renumber(&mut object, &numbers);
            let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
            file.add(
                ObjectId::new(number, 0),
                &object,
                data.as_deref().map(Vec::as_slice),
            );
        }

        let mut trailer = Dictionary::new();
        trailer.insert(
            Name::from("Root"),
            Object::Reference(ObjectId::new(numbers[&self.catalog], 0)),
        );
        if let Some(info) = info.filter(|info| numbers.contains_key(info)) {
            trailer.insert(
                Name::from("Info"),
                Object::Reference(ObjectId::new(numbers[&info], 0)),
            );
        }
        let id = match self.document.resolver().trailer().get("ID") {
            Some(Object::Array(id))
                if id.len() == 2 && id.iter().all(|i| i.as_string().is_some()) =>
            {
                Object::Array(id.clone())
            }
            // A new file gets an identifier made from its content (section
            // 14.4).
            _ => {
                let mut digest = Md5::new();
                file.bytes().for_each(|bytes| digest.update(bytes));
                let hash = PdfString::new(digest.finalize().to_vec(), StringFormat::Hexadecimal);
                Object::Array(vec![Object::String(hash.clone()), Object::String(hash)])
            }
        };
        trailer.insert(Name::from("ID"), id);
        Ok(file.finish(trailer))
    }

    /// Writes the editor's changes as an incremental update of the original
    /// file (ISO 32000-2, section 7.5.6): the file's bytes, as they are, then
    /// the objects changed or added, and a cross-reference section that
    /// points back at the file's.
    ///
    /// The file keeps every byte it had, so digital signatures stay valid,
    /// and earlier versions of the document can still be recovered from it. Its last cross-reference section
    /// decides the new one's form: a table, or a stream. Pages left out by
    /// [`select_pages`](Self::select_pages) are still in the file, unused.
    ///
    /// It is an `EncryptedUpdate` error for an encrypted file, and a
    /// `DamagedUpdate` error if the file's cross-reference data had to be
    /// repaired.
    pub fn save_incremental(&mut self) -> Result<Vec<u8>> {
        if self.document.is_encrypted() {
            return Err(Error::new(ErrorKind::EncryptedUpdate, 0));
        }
        let diagnostics = self.document.diagnostics();
        let damaged = diagnostics.dropped() > 0
            || diagnostics.iter().any(|diagnostic| {
                matches!(
                    diagnostic.kind(),
                    DiagnosticKind::XrefRebuilt
                        | DiagnosticKind::ObjectRelocated
                        | DiagnosticKind::BrokenXrefSection
                        | DiagnosticKind::XrefLoop
                )
            });
        let file = self.document.resolver().file();
        let limits = self.document.limits();
        let (Ok(startxref), false) = (find_startxref(file, limits), damaged) else {
            return Err(Error::new(ErrorKind::DamagedUpdate, 0));
        };
        let original = self.document.resolver().trailer().clone();
        let is_stream = original
            .get("Type")
            .and_then(Object::as_name)
            .is_some_and(|kind| kind.as_bytes() == b"XRef");
        let layout = if is_stream {
            Layout::Compressed
        } else {
            Layout::Classic
        };
        let size = original
            .get("Size")
            .and_then(Object::as_integer)
            .and_then(|size| u32::try_from(size).ok())
            .unwrap_or(0);

        let mut writer = FileWriter::update(file, layout, startxref.offset, size);
        let mut ids: Vec<ObjectId> = self.edits.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let (object, data) = match &self.edits[&id] {
                Edit::Object(object) => (object.clone(), None),
                Edit::Stream(dictionary, data) => {
                    let mut dictionary = dictionary.clone();
                    dictionary.remove("Length");
                    (Object::Dictionary(dictionary), Some(data.clone()))
                }
            };
            writer.add(id, &object, data.as_deref().map(Vec::as_slice));
        }

        // The trailer keeps the original's entries, but those of its own
        // cross-reference section.
        let mut trailer = Dictionary::new();
        for (key, value) in original.iter() {
            let own = matches!(
                key.as_bytes(),
                b"Size"
                    | b"Prev"
                    | b"XRefStm"
                    | b"Type"
                    | b"W"
                    | b"Index"
                    | b"Filter"
                    | b"DecodeParms"
                    | b"Length"
                    | b"DL"
                    | b"F"
                    | b"FFilter"
                    | b"FDecodeParms"
            );
            if !own {
                trailer.insert(key.clone(), value.clone());
            }
        }
        trailer.insert(Name::from("Root"), Object::Reference(self.catalog));
        if let Some(info) = self.info {
            trailer.insert(Name::from("Info"), Object::Reference(info));
        }
        // The first identifier stays; the second changes with the content.
        let mut digest = Md5::new();
        writer.bytes().for_each(|bytes| digest.update(bytes));
        let changed = Object::String(PdfString::new(
            digest.finalize().to_vec(),
            StringFormat::Hexadecimal,
        ));
        let first = match original.get("ID") {
            Some(Object::Array(id)) if id.first().is_some_and(|i| i.as_string().is_some()) => {
                id[0].clone()
            }
            _ => changed.clone(),
        };
        trailer.insert(Name::from("ID"), Object::Array(vec![first, changed]));
        Ok(writer.finish(trailer))
    }

    // The object `id` refers to, ready to write: a stream as its dictionary,
    // without `/Length`, and its data. `None` for an object that is null or
    // can't be read, and for the cross-reference and object streams of the
    // file, which the new file doesn't use.
    fn load(&mut self, id: ObjectId) -> Result<Option<Loaded>> {
        match self.edits.get(&id) {
            // A null object, such as one that couldn't be copied, is null
            // where it is referred to.
            Some(Edit::Object(Object::Null)) => return Ok(None),
            Some(Edit::Object(object)) => return Ok(Some((object.clone(), None))),
            Some(Edit::Stream(dictionary, data)) => {
                let mut dictionary = dictionary.clone();
                dictionary.remove("Length");
                return Ok(Some((Object::Dictionary(dictionary), Some(data.clone()))));
            }
            None => {}
        }
        let resolver = self.document.resolver();
        let object = match resolver.resolve(id) {
            Ok(object) => object,
            Err(error) if error.kind() == ErrorKind::PasswordRequired => return Err(error),
            Err(_) => return Ok(None),
        };
        match object.as_ref() {
            Object::Null => Ok(None),
            Object::Stream(stream) => {
                let mut dictionary = stream.dictionary().clone();
                let kind = dictionary.get("Type").and_then(Object::as_name);
                if kind.is_some_and(|kind| matches!(kind.as_bytes(), b"XRef" | b"ObjStm")) {
                    return Ok(None);
                }
                dictionary.remove("Length");
                let data = match resolver.decrypted_stream(id) {
                    Ok((_, data)) => data,
                    Err(error) if error.kind() == ErrorKind::PasswordRequired => {
                        return Err(error);
                    }
                    // Without data, the filters would have nothing to decode.
                    Err(_) => {
                        for key in ["Filter", "DecodeParms", "DL"] {
                            dictionary.remove(key);
                        }
                        Vec::new()
                    }
                };
                Ok(Some((Object::Dictionary(dictionary), Some(Arc::new(data)))))
            }
            object => Ok(Some((object.clone(), None))),
        }
    }
}

// The PDF version of `document`: the newer of its header and its catalog's
// `/Version`, if either says.
fn file_version(document: &mut Document<'_>) -> Option<(u8, u8)> {
    let catalog = document
        .catalog()
        .dictionary()
        .get("Version")
        .and_then(Object::as_name)
        .and_then(|name| parse_version(name.as_bytes()));
    let file = document.resolver().file();
    let header = file.get(..file.len().min(1024)).and_then(|start| {
        let at = start.windows(5).position(|window| window == b"%PDF-")?;
        parse_version(&start[at + 5..])
    });
    header.max(catalog)
}

// Whether `document` packs objects into object streams.
fn uses_object_streams(document: &mut Document<'_>) -> bool {
    document
        .resolver()
        .table()
        .iter()
        .any(|(_, entry)| matches!(entry, XrefEntry::Compressed { .. }))
}

// A version such as `1.7` at the start of `bytes`.
fn parse_version(bytes: &[u8]) -> Option<(u8, u8)> {
    match bytes {
        [major @ b'1'..=b'2', b'.', minor @ b'0'..=b'9', ..] => Some((major - b'0', minor - b'0')),
        _ => None,
    }
}

// Whether the engine had to work around a damaged page tree, so saving it as
// it is would give other readers other pages.
fn tree_is_damaged(document: &Document<'_>) -> bool {
    let diagnostics = document.diagnostics();
    document.catalog().id().is_none()
        || document
            .pages()
            .iter()
            .any(|page| page.id().is_none() || page.is_damaged())
        || diagnostics.dropped() > 0
        || diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind(),
                DiagnosticKind::PageTreeCycle
                    | DiagnosticKind::InvalidPageTreeNode
                    | DiagnosticKind::DamagedPage
                    | DiagnosticKind::WrongPageCount
                    | DiagnosticKind::PageTreeRebuilt
                    | DiagnosticKind::PageTreeFound
            )
        })
}

// The references inside `object`, in the order they appear.
fn references(object: &Object, found: &mut Vec<ObjectId>) {
    match object {
        Object::Reference(id) => found.push(*id),
        Object::Array(items) => items.iter().for_each(|item| references(item, found)),
        Object::Dictionary(dictionary) => {
            dictionary
                .iter()
                .for_each(|(_, value)| references(value, found));
        }
        Object::Stream(stream) => {
            stream
                .dictionary()
                .iter()
                .for_each(|(_, value)| references(value, found));
        }
        _ => {}
    }
}

// Replaces each reference in `object` with the new number of its object, or
// with null if it wasn't written.
fn renumber(object: &mut Object, numbers: &HashMap<ObjectId, u32>) {
    match object {
        Object::Reference(id) => {
            *object = match numbers.get(id) {
                Some(&number) => Object::Reference(ObjectId::new(number, 0)),
                None => Object::Null,
            };
        }
        Object::Array(items) => items.iter_mut().for_each(|item| renumber(item, numbers)),
        Object::Dictionary(dictionary) => {
            dictionary
                .values_mut()
                .for_each(|value| renumber(value, numbers));
        }
        Object::Stream(stream) => {
            stream
                .dictionary_mut()
                .values_mut()
                .for_each(|value| renumber(value, numbers));
        }
        _ => {}
    }
}

fn rectangle(rectangle: Rectangle) -> Object {
    let corners = [
        rectangle.left,
        rectangle.bottom,
        rectangle.right,
        rectangle.top,
    ];
    Object::Array(corners.into_iter().map(number).collect())
}

// An integer when the number is one, as writers usually have page sizes.
fn number(value: f64) -> Object {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        Object::Integer(value as i64)
    } else {
        Object::Real(value)
    }
}
