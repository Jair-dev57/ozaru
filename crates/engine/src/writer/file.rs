use std::collections::BTreeMap;
use std::io::Write as _;

use miniz_oxide::deflate::compress_to_vec_zlib;

use super::serialize::{write_dictionary, write_indirect, write_object};
use crate::syntax::{Dictionary, Name, Object, ObjectId};

/// How a [`FileWriter`] stores the objects and their cross-reference data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// Every object on its own, and a classic cross-reference table, which
    /// every reader understands (ISO 32000-2, section 7.5.4).
    #[default]
    Classic,
    /// Objects other than streams packed into compressed object streams, and a
    /// compressed cross-reference stream (sections 7.5.7 and 7.5.8), as PDF 1.5
    /// allows: much smaller files when there are many small objects.
    Compressed,
}

// Objects in each object stream, as other writers use.
const OBJECTS_PER_STREAM: usize = 100;

// Where an object of the file being written is.
#[derive(Debug, Clone, Copy)]
enum Entry {
    InUse { offset: usize, generation: u16 },
    Compressed { stream: u32, index: u32 },
}

/// Builds a PDF file object by object, then ends it with its cross-reference
/// data and trailer (ISO 32000-2, section 7.5).
#[derive(Debug)]
pub struct FileWriter {
    out: Vec<u8>,
    layout: Layout,
    entries: BTreeMap<u32, Entry>,
    // The objects waiting to go into object streams, with the compressed
    // layout: their numbers and their syntax.
    packed: Vec<(u32, Vec<u8>)>,
    // For an incremental update: the offset of the original's last
    // cross-reference section, and its `/Size`.
    update: Option<(usize, u32)>,
}

impl FileWriter {
    /// A new file, with the header of PDF `version`, such as `"1.7"`, and a
    /// comment of four bytes above 127, which tells transfer programs that the
    /// file is binary.
    pub fn new(version: &str, layout: Layout) -> Self {
        let mut out = Vec::new();
        let _ = writeln!(out, "%PDF-{version}");
        out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");
        Self {
            out,
            layout,
            entries: BTreeMap::new(),
            packed: Vec::new(),
            update: None,
        }
    }

    /// An incremental update of `file` (ISO 32000-2, section 7.5.6): the
    /// objects added are written after its bytes, which stay as they are, and
    /// [`finish`](Self::finish) writes a cross-reference section with only
    /// them, whose `/Prev` is `previous`, the offset of the file's last
    /// section, and whose `/Size` is at least `size`, the file's.
    ///
    /// With the classic layout the section is a table; with the compressed
    /// one, a cross-reference stream, but the objects aren't packed into
    /// object streams.
    pub fn update(file: &[u8], layout: Layout, previous: usize, size: u32) -> Self {
        let mut out = file.to_vec();
        if !matches!(out.last(), Some(b'\n' | b'\r')) {
            out.push(b'\n');
        }
        Self {
            out,
            layout,
            entries: BTreeMap::new(),
            packed: Vec::new(),
            update: Some((previous, size)),
        }
    }

    /// Adds indirect object `id`; with `data`, as a stream with that data,
    /// already encoded. See [`write_indirect`].
    ///
    /// With the compressed layout, an object that isn't a stream and has
    /// generation 0 waits to go into an object stream, written by
    /// [`finish`](Self::finish).
    pub fn add(&mut self, id: ObjectId, object: &Object, data: Option<&[u8]>) {
        let packs = self.layout == Layout::Compressed && self.update.is_none();
        if packs && data.is_none() && id.generation == 0 {
            let mut syntax = Vec::new();
            write_object(&mut syntax, object);
            self.packed.push((id.number, syntax));
            return;
        }
        let offset = write_indirect(&mut self.out, id, object, data);
        let generation = id.generation;
        self.entries
            .insert(id.number, Entry::InUse { offset, generation });
    }

    /// The bytes written so far.
    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// The bytes written so far, then the objects waiting for an object
    /// stream.
    pub fn bytes(&self) -> impl Iterator<Item = &[u8]> {
        std::iter::once(&self.out[..]).chain(self.packed.iter().map(|(_, syntax)| &syntax[..]))
    }

    /// Ends the file: the object streams, if any, then the cross-reference
    /// data with `trailer`, its `/Size` set, then `startxref` and `%%EOF`.
    ///
    /// Object numbers not used are free entries, linked in a list from entry
    /// 0, as the standard asks.
    pub fn finish(mut self, trailer: Dictionary) -> Vec<u8> {
        match self.layout {
            Layout::Classic => self.finish_table(trailer),
            Layout::Compressed => {
                self.write_object_streams();
                self.finish_stream(trailer)
            }
        }
    }

    fn next_number(&self) -> u32 {
        let written = self.entries.keys().next_back().copied().unwrap_or(0);
        let packed = self
            .packed
            .iter()
            .map(|&(number, _)| number)
            .max()
            .unwrap_or(0);
        let next = written.max(packed).saturating_add(1);
        match self.update {
            Some((_, size)) => next.max(size),
            None => next,
        }
    }

    // The runs of consecutive object numbers written by an update, each a
    // subsection of its cross-reference section: its first number and length.
    fn runs(&self) -> Vec<(u32, u32)> {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for &number in self.entries.keys() {
            match runs.last_mut() {
                Some((first, length)) if *first + *length == number => *length += 1,
                _ => runs.push((number, 1)),
            }
        }
        runs
    }

    // Packs the waiting objects into object streams (section 7.5.7): a header
    // of number and offset pairs, then the objects, compressed with Flate.
    fn write_object_streams(&mut self) {
        let mut number = self.next_number();
        let packed = std::mem::take(&mut self.packed);
        for group in packed.chunks(OBJECTS_PER_STREAM) {
            let mut header = Vec::new();
            let mut body = Vec::new();
            for (index, (object, syntax)) in group.iter().enumerate() {
                let _ = write!(header, "{object} {} ", body.len());
                body.extend_from_slice(syntax);
                body.push(b'\n');
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                self.entries.insert(
                    *object,
                    Entry::Compressed {
                        stream: number,
                        index,
                    },
                );
            }
            header.push(b'\n');
            let first = header.len();
            header.extend_from_slice(&body);
            let data = compress_to_vec_zlib(&header, 6);

            let mut dictionary = Dictionary::new();
            dictionary.insert(Name::from("Type"), Object::Name(Name::from("ObjStm")));
            dictionary.insert(Name::from("N"), integer(group.len()));
            dictionary.insert(Name::from("First"), integer(first));
            dictionary.insert(
                Name::from("Filter"),
                Object::Name(Name::from("FlateDecode")),
            );
            let id = ObjectId::new(number, 0);
            let dictionary = Object::Dictionary(dictionary);
            let offset = write_indirect(&mut self.out, id, &dictionary, Some(&data));
            self.entries.insert(
                number,
                Entry::InUse {
                    offset,
                    generation: 0,
                },
            );
            number = number.saturating_add(1);
        }
    }

    // A classic cross-reference table, with an end of line of two bytes in
    // each entry, so each is exactly 20 bytes (section 7.5.4).
    fn finish_table(mut self, mut trailer: Dictionary) -> Vec<u8> {
        let size = self.next_number();
        let start = self.out.len();
        if let Some((previous, _)) = self.update {
            self.out.extend_from_slice(b"xref\n");
            for (first, length) in self.runs() {
                let _ = writeln!(self.out, "{first} {length}");
                for number in first..first + length {
                    if let Some(Entry::InUse { offset, generation }) = self.entries.get(&number) {
                        let _ = write!(self.out, "{offset:010} {generation:05} n\r\n");
                    }
                }
            }
            trailer.insert(Name::from("Size"), Object::Integer(i64::from(size)));
            trailer.insert(Name::from("Prev"), integer(previous));
            self.out.extend_from_slice(b"trailer\n");
            write_dictionary(&mut self.out, &trailer);
            let _ = write!(self.out, "\nstartxref\n{start}\n%%EOF\n");
            return self.out;
        }
        let _ = write!(self.out, "xref\n0 {size}\n");
        let free = FreeList::new(&self.entries, size);
        let _ = write!(self.out, "{:010} 65535 f\r\n", free.after(0));
        for number in 1..size {
            match self.entries.get(&number) {
                Some(Entry::InUse { offset, generation }) => {
                    let _ = write!(self.out, "{offset:010} {generation:05} n\r\n");
                }
                // A classic table has no compressed entries.
                Some(Entry::Compressed { .. }) | None => {
                    let _ = write!(self.out, "{:010} 00001 f\r\n", free.after(number));
                }
            }
        }
        trailer.insert(Name::from("Size"), Object::Integer(i64::from(size)));
        self.out.extend_from_slice(b"trailer\n");
        write_dictionary(&mut self.out, &trailer);
        let _ = write!(self.out, "\nstartxref\n{start}\n%%EOF\n");
        self.out
    }

    // A cross-reference stream (section 7.5.8): its dictionary is the trailer,
    // and it has an entry of its own.
    fn finish_stream(mut self, mut trailer: Dictionary) -> Vec<u8> {
        let number = self.next_number();
        let size = number.saturating_add(1);
        let start = self.out.len();
        self.entries.insert(
            number,
            Entry::InUse {
                offset: start,
                generation: 0,
            },
        );

        // The widths of the fields: the type, then the offset or object
        // stream, in as few bytes as they need, then the generation or index.
        let largest = self
            .entries
            .values()
            .map(|entry| match *entry {
                Entry::InUse { offset, .. } => offset as u64,
                Entry::Compressed { stream, .. } => u64::from(stream),
            })
            .chain([u64::from(size)])
            .max()
            .unwrap_or(0);
        let width = (1..8)
            .find(|&bytes| largest >> (8 * bytes) == 0)
            .unwrap_or(8);
        let free = FreeList::new(&self.entries, size);
        let mut data = Vec::with_capacity(size as usize * (width + 3));
        let mut row = |kind: u8, value: u64, last: u16| {
            data.push(kind);
            data.extend_from_slice(&value.to_be_bytes()[8 - width..]);
            data.extend_from_slice(&last.to_be_bytes());
        };
        if let Some((previous, _)) = self.update {
            let runs = self.runs();
            for &(first, length) in &runs {
                for number in first..first + length {
                    if let Some(&Entry::InUse { offset, generation }) = self.entries.get(&number) {
                        row(1, offset as u64, generation);
                    }
                }
            }
            let index = runs
                .iter()
                .flat_map(|&(first, length)| [first, length])
                .map(|value| Object::Integer(i64::from(value)))
                .collect();
            trailer.insert(Name::from("Index"), Object::Array(index));
            trailer.insert(Name::from("Prev"), integer(previous));
        } else {
            row(0, u64::from(free.after(0)), 65535);
        }
        for number in (1..size).filter(|_| self.update.is_none()) {
            match self.entries.get(&number) {
                Some(&Entry::InUse { offset, generation }) => row(1, offset as u64, generation),
                Some(&Entry::Compressed { stream, index }) => {
                    row(
                        2,
                        u64::from(stream),
                        u16::try_from(index).unwrap_or(u16::MAX),
                    );
                }
                None => row(0, u64::from(free.after(number)), 1),
            }
        }

        trailer.insert(Name::from("Type"), Object::Name(Name::from("XRef")));
        trailer.insert(Name::from("Size"), Object::Integer(i64::from(size)));
        trailer.insert(
            Name::from("W"),
            Object::Array(vec![integer(1), integer(width), integer(2)]),
        );
        trailer.insert(
            Name::from("Filter"),
            Object::Name(Name::from("FlateDecode")),
        );
        let compressed = compress_to_vec_zlib(&data, 6);
        let id = ObjectId::new(number, 0);
        let trailer = Object::Dictionary(trailer);
        write_indirect(&mut self.out, id, &trailer, Some(&compressed));
        let _ = write!(self.out, "startxref\n{start}\n%%EOF\n");
        self.out
    }
}

// The free object numbers below `size`, each pointing at the next one.
struct FreeList(Vec<u32>);

impl FreeList {
    fn new(entries: &BTreeMap<u32, Entry>, size: u32) -> Self {
        Self(
            (1..size)
                .filter(|number| !entries.contains_key(number))
                .collect(),
        )
    }

    // The free number after `number`, or 0 after the last one.
    fn after(&self, number: u32) -> u32 {
        let next = self.0.partition_point(|&free| free <= number);
        self.0.get(next).copied().unwrap_or(0)
    }
}

fn integer(value: usize) -> Object {
    Object::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}
