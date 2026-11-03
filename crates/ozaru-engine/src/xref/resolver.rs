use std::collections::HashMap;
use std::sync::Arc;

use super::{ObjectStream, XrefEntry, XrefTable, load_xref, parse_object_stream, rebuild_xref};
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object, ObjectId, parse_indirect_object_with_lengths};

/// Finds the object a reference points at, wherever it is in the file
/// (ISO 32000-2, section 7.3.10).
///
/// Streams whose `/Length` is a reference get their real length, so their data
/// doesn't depend on finding `endstream`.
///
/// It reads each object, and decodes each object stream, only once: the results
/// are cached, errors included, so a broken object can't be decoded again and
/// again. It holds no global or per-thread state, so several files can be read at
/// the same time, each with its own resolver.
#[derive(Debug)]
pub struct Resolver<'a> {
    file: &'a [u8],
    table: XrefTable,
    limits: Limits,
    objects: HashMap<ObjectId, Result<Arc<Object>>>,
    object_streams: HashMap<u32, Result<Arc<ObjectStream>>>,
    // Object numbers being resolved right now, to detect cycles.
    in_progress: Vec<u32>,
    // The table rebuilt by scanning the file, built the first time an object isn't
    // where `table` says. `Some(None)` if that failed. Not used when `table` was
    // rebuilt already.
    rebuilt: Option<Option<XrefTable>>,
    diagnostics: Diagnostics,
}

impl<'a> Resolver<'a> {
    /// A resolver for `file`, whose cross-reference data is `table`.
    pub fn new(file: &'a [u8], table: XrefTable, limits: Limits) -> Self {
        Self {
            file,
            table,
            limits,
            objects: HashMap::new(),
            object_streams: HashMap::new(),
            in_progress: Vec::new(),
            rebuilt: None,
            diagnostics: Diagnostics::new(limits.max_diagnostics),
        }
    }

    /// Loads the cross-reference data of `file` with [`load_xref`] and returns a
    /// resolver for it. Its diagnostics are the first of the resolver's.
    ///
    /// If the data can't be loaded, it is rebuilt with [`rebuild_xref`], with an
    /// `XrefRebuilt` diagnostic. It is only an error if that fails too, and then
    /// the error is the one of the original data.
    pub fn open(file: &'a [u8], limits: Limits) -> Result<Self> {
        let (loaded, rebuilt_at) = match load_xref(file, limits) {
            Ok(loaded) => (loaded, None),
            Err(error) => match rebuild_xref(file, limits) {
                Ok(rebuilt) => (rebuilt, Some(error.offset())),
                Err(_) => return Err(error),
            },
        };
        let mut resolver = Self::new(file, loaded.table, limits);
        resolver.diagnostics = loaded.diagnostics;
        if let Some(offset) = rebuilt_at {
            resolver
                .diagnostics
                .push(DiagnosticKind::XrefRebuilt, offset);
            resolver.rebuilt = Some(None);
        }
        Ok(resolver)
    }

    /// The newest trailer, with `/Root`, the entry point of the document.
    pub fn trailer(&self) -> &Dictionary {
        self.table.trailer()
    }

    /// The cross-reference data used to find objects.
    pub fn table(&self) -> &XrefTable {
        &self.table
    }

    /// Problems found and worked around so far, while loading the
    /// cross-reference data and resolving objects.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// The object that `id` refers to.
    ///
    /// As the standard says, a reference to an object that doesn't exist, is
    /// free, or has another generation, resolves to the null object. If the object
    /// isn't where the cross-reference data says, the file is scanned for it, with
    /// an `ObjectRelocated` diagnostic. It is an error if it can't be found or
    /// read, or depends on itself.
    pub fn resolve(&mut self, id: ObjectId) -> Result<Arc<Object>> {
        if let Some(cached) = self.objects.get(&id) {
            return cached.clone();
        }
        let result = self.read(id);
        // A cycle is only an error for the objects involved while it is being
        // resolved, so it isn't cached.
        if !matches!(&result, Err(error) if error.kind() == ErrorKind::ReferenceCycle) {
            self.objects.insert(id, result.clone());
        }
        result
    }

    fn read(&mut self, id: ObjectId) -> Result<Arc<Object>> {
        let null = || Ok(Arc::new(Object::Null));
        let Some(entry) = self.table.get(id.number) else {
            return null();
        };
        let generation = match entry {
            XrefEntry::Free { .. } => return null(),
            XrefEntry::InUse { generation, .. } => generation,
            XrefEntry::Compressed { .. } => 0,
        };
        if generation != id.generation {
            let offset = self.offset_of(entry);
            self.diagnostics
                .push(DiagnosticKind::GenerationMismatch, offset);
            return null();
        }

        if self.in_progress.contains(&id.number) {
            return Err(Error::new(ErrorKind::ReferenceCycle, self.offset_of(entry)));
        }
        if self.in_progress.len() >= self.limits.max_resolve_depth {
            return Err(Error::new(
                ErrorKind::LimitExceeded(Limit::ResolveDepth),
                self.offset_of(entry),
            ));
        }
        self.in_progress.push(id.number);
        let result = match self.read_entry(id, entry) {
            Err(error) if is_worth_relocating(&error) => {
                self.relocate(id, entry).unwrap_or(Err(error))
            }
            result => result,
        };
        self.in_progress.pop();
        result
    }

    fn read_entry(&mut self, id: ObjectId, entry: XrefEntry) -> Result<Arc<Object>> {
        match entry {
            XrefEntry::InUse { offset, .. } => self.read_at(id, offset),
            XrefEntry::Compressed { stream, index } => self.read_compressed(id, stream, index),
            XrefEntry::Free { .. } => unreachable!("free entries return early"),
        }
    }

    // Reads `id` where the rebuilt table says, if that's somewhere else than
    // `entry`. `None` if it isn't found there either.
    fn relocate(&mut self, id: ObjectId, entry: XrefEntry) -> Option<Result<Arc<Object>>> {
        if self.rebuilt.is_none() {
            let rebuilt = rebuild_xref(self.file, self.limits).ok();
            self.rebuilt = Some(rebuilt.map(|loaded| loaded.table));
        }
        let found = self.rebuilt.as_ref()?.as_ref()?.get(id.number)?;
        let same_generation = match found {
            XrefEntry::InUse { generation, .. } => generation == id.generation,
            XrefEntry::Compressed { .. } => id.generation == 0,
            XrefEntry::Free { .. } => false,
        };
        if found == entry || !same_generation {
            return None;
        }
        let object = self.read_entry(id, found).ok()?;
        let offset = self.offset_of(entry);
        self.diagnostics
            .push(DiagnosticKind::ObjectRelocated, offset);
        Some(Ok(object))
    }

    // An indirect object at `offset`, which must have the number and generation of
    // `id`. A stream's `/Length` that is a reference is resolved too; if that
    // fails, for example because it refers to the stream itself, the stream ends
    // at `endstream`.
    fn read_at(&mut self, id: ObjectId, offset: usize) -> Result<Arc<Object>> {
        let (file, limits) = (self.file, self.limits);
        let parsed = parse_indirect_object_with_lengths(file, offset, limits, &mut |length| {
            self.resolve(length).ok()?.as_integer()
        })?;
        if parsed.object.id != id {
            return Err(Error::new(ErrorKind::ObjectMismatch, offset));
        }
        self.diagnostics.append(&parsed.diagnostics);
        Ok(Arc::new(parsed.object.object))
    }

    // Object `id`, stored at `index` in the object stream number `stream`.
    fn read_compressed(&mut self, id: ObjectId, stream: u32, index: u32) -> Result<Arc<Object>> {
        let object_stream = self.object_stream(stream)?;
        let offset = self.offset_of(XrefEntry::Compressed { stream, index });
        let index = if object_stream.number(index) == Some(id.number) {
            index
        } else {
            let found = object_stream
                .index_of(id.number)
                .ok_or_else(|| Error::new(ErrorKind::ObjectMismatch, offset))?;
            self.diagnostics
                .push(DiagnosticKind::WrongObjectStreamIndex, offset);
            found
        };
        let compressed = object_stream.object(index)?;
        self.diagnostics.append(&compressed.diagnostics);
        Ok(Arc::new(compressed.object.object))
    }

    fn object_stream(&mut self, number: u32) -> Result<Arc<ObjectStream>> {
        if let Some(cached) = self.object_streams.get(&number) {
            return cached.clone();
        }
        let result = self.read_object_stream(number);
        if !matches!(&result, Err(error) if error.kind() == ErrorKind::ReferenceCycle) {
            self.object_streams.insert(number, result.clone());
        }
        result
    }

    fn read_object_stream(&mut self, number: u32) -> Result<Arc<ObjectStream>> {
        let offset = self
            .table
            .get(number)
            .map_or(0, |entry| self.offset_of(entry));
        let container = self.resolve(ObjectId::new(number, 0))?;
        let Some(stream) = container.as_stream() else {
            return Err(Error::new(ErrorKind::NotAnObjectStream, offset));
        };
        let parsed = parse_object_stream(stream, self.file, self.limits)?;
        self.diagnostics.append(&parsed.diagnostics);
        Ok(Arc::new(parsed.stream))
    }

    // Where an entry's object is in the file, for errors: its own offset, or the
    // offset of the object stream that holds it.
    fn offset_of(&self, entry: XrefEntry) -> usize {
        match entry {
            XrefEntry::InUse { offset, .. } => offset,
            XrefEntry::Compressed { stream, .. } => match self.table.get(stream) {
                Some(XrefEntry::InUse { offset, .. }) => offset,
                _ => 0,
            },
            XrefEntry::Free { .. } => 0,
        }
    }
}

// A cycle or an exceeded limit wouldn't go away by reading the object elsewhere.
fn is_worth_relocating(error: &Error) -> bool {
    !matches!(
        error.kind(),
        ErrorKind::ReferenceCycle | ErrorKind::LimitExceeded(_)
    )
}
