use std::collections::HashMap;
use std::sync::Arc;

use super::{
    ObjectStream, XrefEntry, XrefTable, load_xref, parse_object_stream_data, rebuild_xref,
};
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::filters::{Decoded, decode_data};
use crate::security::{FileKey, StandardSecurity};
use crate::syntax::{Dictionary, Object, ObjectId, Stream, parse_indirect_object_with_lengths};

/// Finds the object a reference points at, wherever it is in the file
/// (ISO 32000-2, section 7.3.10).
///
/// Streams whose `/Length` is a reference get their real length, so their data
/// doesn't depend on finding `endstream`.
///
/// An encrypted file is decrypted on the fly when its user password is empty, or
/// after [`unlock`](Self::unlock) with the right one: the strings of each object
/// when it is resolved, and stream data in [`stream_data`](Self::stream_data).
/// Until then, numbers, names and references still resolve, so the page tree
/// can be read, but strings stay encrypted and stream data is a
/// `PasswordRequired` error.
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
    encryption: Option<Encryption>,
    diagnostics: Diagnostics,
}

// How an encrypted file is decrypted.
#[derive(Debug)]
struct Encryption {
    // The security handler, or why it can't be used, such as an unsupported one.
    security: std::result::Result<StandardSecurity, Error>,
    // The file key, once a password worked.
    key: Option<FileKey>,
    // The /Encrypt dictionary's own object, which is never encrypted.
    dictionary: Option<ObjectId>,
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
            encryption: None,
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
        resolver.encryption = resolver.read_encryption();
        Ok(resolver)
    }

    // The trailer's /Encrypt, tried with the empty user password.
    fn read_encryption(&mut self) -> Option<Encryption> {
        let entry = self.trailer().get("Encrypt")?.clone();
        let (dictionary, object) = match entry {
            Object::Reference(id) => (Some(id), self.resolve(id)),
            object => (None, Ok(Arc::new(object))),
        };
        let file_id = self
            .trailer()
            .get("ID")
            .and_then(Object::as_array)
            .and_then(|id| id.first())
            .and_then(Object::as_string)
            .map_or(&[][..], |id| id.as_bytes())
            .to_vec();
        let offset = dictionary
            .and_then(|id| self.table.get(id.number))
            .map_or(0, |entry| self.offset_of(entry));
        let security = object.and_then(|object| match object.as_dictionary() {
            Some(encrypt) => StandardSecurity::new(encrypt, &file_id, offset),
            None => Err(Error::new(ErrorKind::InvalidEncryption, offset)),
        });
        let key = security.as_ref().ok().and_then(|s| s.authenticate(b""));
        // /Encrypt was read before the key was known, so nothing is cached
        // encrypted except it.
        self.objects.retain(|id, _| Some(*id) == dictionary);
        Some(Encryption {
            security,
            key,
            dictionary,
        })
    }

    /// Whether the file is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.encryption.is_some()
    }

    /// Whether the file is encrypted and can't be decrypted yet: its user password
    /// isn't empty, or its encryption isn't supported.
    pub fn needs_password(&self) -> bool {
        self.encryption.as_ref().is_some_and(|e| e.key.is_none())
    }

    /// Tries `password` as the user password, and returns whether it was right.
    /// Objects resolved before are read again, decrypted.
    pub fn unlock(&mut self, password: &[u8]) -> bool {
        let Some(encryption) = &mut self.encryption else {
            return true;
        };
        let Ok(security) = &encryption.security else {
            return false;
        };
        let Some(key) = security.authenticate(password) else {
            return false;
        };
        encryption.key = Some(key);
        let dictionary = encryption.dictionary;
        self.objects.retain(|id, _| Some(*id) == dictionary);
        self.object_streams.clear();
        true
    }

    /// The decrypted and decoded data of the stream that `id` refers to.
    ///
    /// It is an error if `id` isn't a stream, if its filters can't decode it, or,
    /// with a `PasswordRequired` error, if it is encrypted and the file is locked.
    pub fn stream_data(&mut self, id: ObjectId) -> Result<Decoded> {
        let object = self.resolve(id)?;
        let offset = self.table.get(id.number).map_or(0, |e| self.offset_of(e));
        let Some(stream) = object.as_stream() else {
            return Err(Error::new(ErrorKind::NotAStream, offset));
        };
        let raw = self.decrypted_data(id, stream)?;
        let decoded = decode_data(
            stream.dictionary(),
            &raw,
            stream.data_range().start,
            self.limits,
        )?;
        Ok(decoded)
    }

    // The raw data of `stream`, object `id`, decrypted if the file is encrypted.
    fn decrypted_data(&mut self, id: ObjectId, stream: &Stream) -> Result<Vec<u8>> {
        let offset = stream.data_range().start;
        let raw = stream
            .raw_data(self.file)
            .ok_or_else(|| Error::new(ErrorKind::UnexpectedEnd, self.file.len()))?;
        let Some(encryption) = &self.encryption else {
            return Ok(raw.to_vec());
        };
        match (&encryption.security, &encryption.key) {
            (Ok(security), _) if !security.decrypts_stream(stream.dictionary()) => Ok(raw.to_vec()),
            (Ok(security), Some(key)) => {
                security.decrypt_stream_data(key, id, raw, offset, &mut self.diagnostics)
            }
            _ => Err(Error::new(ErrorKind::PasswordRequired, offset)),
        }
    }

    // Decrypts the strings of object `id`, read outside of any object stream. The
    // /Encrypt dictionary, cross-reference streams and unencrypted metadata streams
    // are left as they are.
    fn decrypt_strings(&mut self, id: ObjectId, object: &mut Object, offset: usize) -> Result<()> {
        let Some(encryption) = &self.encryption else {
            return Ok(());
        };
        let (Ok(security), Some(key)) = (&encryption.security, &encryption.key) else {
            return Ok(());
        };
        let unencrypted_stream = object
            .as_stream()
            .is_some_and(|stream| !security.decrypts_stream(stream.dictionary()));
        if Some(id) == encryption.dictionary || unencrypted_stream {
            return Ok(());
        }
        security.decrypt_object(key, id, object, offset, &mut self.diagnostics)
    }

    /// The newest trailer, with `/Root`, the entry point of the document.
    pub fn trailer(&self) -> &Dictionary {
        self.table.trailer()
    }

    /// The cross-reference data used to find objects.
    pub fn table(&self) -> &XrefTable {
        &self.table
    }

    /// Where object `number` is in the file, in bytes from the start: its own
    /// offset, or the offset of the object stream holding it. 0 if the
    /// cross-reference data doesn't say. It is meant for errors and diagnostics.
    pub fn object_offset(&self, number: u32) -> usize {
        self.table
            .get(number)
            .map_or(0, |entry| self.offset_of(entry))
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
        let mut object = parsed.object.object;
        self.decrypt_strings(id, &mut object, offset)?;
        Ok(Arc::new(object))
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
        let data = self.decrypted_data(ObjectId::new(number, 0), stream)?;
        let parsed = parse_object_stream_data(stream, &data, self.limits)?;
        self.diagnostics.append(&parsed.diagnostics);
        Ok(Arc::new(parsed.stream))
    }

    // Where an entry's object is in the file, for errors and diagnostics: its own
    // offset, or the offset of the object stream that holds it. A damaged entry can
    // point past the end of the file, and then the end of the file is used.
    fn offset_of(&self, entry: XrefEntry) -> usize {
        let offset = match entry {
            XrefEntry::InUse { offset, .. } => offset,
            XrefEntry::Compressed { stream, .. } => match self.table.get(stream) {
                Some(XrefEntry::InUse { offset, .. }) => offset,
                _ => 0,
            },
            XrefEntry::Free { .. } => 0,
        };
        offset.min(self.file.len())
    }
}

// A cycle, an exceeded limit or a missing password wouldn't go away by reading the
// object elsewhere.
fn is_worth_relocating(error: &Error) -> bool {
    !matches!(
        error.kind(),
        ErrorKind::ReferenceCycle | ErrorKind::LimitExceeded(_) | ErrorKind::PasswordRequired
    )
}
