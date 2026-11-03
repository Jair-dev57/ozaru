use std::collections::{BTreeMap, btree_map};

use super::skip_whitespace;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limits, Result};
use crate::syntax::chars::is_regular;
use crate::syntax::{Dictionary, Object, Parser};

/// Where one object is, according to a cross-reference table or stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefEntry {
    /// The object is in use and starts at `offset`, in bytes from the start of the
    /// file.
    InUse { offset: usize, generation: u16 },
    /// The object number isn't used. A later object with this number would get
    /// `generation`.
    Free { generation: u16 },
    /// The object is the `index`-th one inside the object stream number `stream`.
    /// Only cross-reference streams have these entries. Its generation is 0.
    Compressed { stream: u32, index: u32 },
}

/// The entries of one cross-reference section and its trailer: a classic table
/// (ISO 32000-2, sections 7.5.4 and 7.5.5) or a cross-reference stream, whose
/// dictionary is its trailer (section 7.5.8).
#[derive(Debug, Clone, PartialEq)]
pub struct XrefTable {
    entries: BTreeMap<u32, XrefEntry>,
    trailer: Dictionary,
}

impl XrefTable {
    pub(super) fn new(entries: BTreeMap<u32, XrefEntry>, trailer: Dictionary) -> Self {
        Self { entries, trailer }
    }

    /// The entry for object `number`, if the table has one.
    pub fn get(&self, number: u32) -> Option<XrefEntry> {
        self.entries.get(&number).copied()
    }

    /// Number of entries, in use or free.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries in order of object number.
    pub fn iter(&self) -> impl Iterator<Item = (u32, XrefEntry)> + '_ {
        self.entries.iter().map(|(&number, &entry)| (number, entry))
    }

    /// The trailer dictionary, with `/Size`, `/Root` and, after an incremental
    /// update, `/Prev`.
    pub fn trailer(&self) -> &Dictionary {
        &self.trailer
    }
}

/// A section parsed by [`parse_xref_table`] or
/// [`parse_xref_stream`](super::parse_xref_stream).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedXrefTable {
    pub table: XrefTable,
    /// Offset just past the trailer dictionary, or past `endobj` for a stream.
    pub end: usize,
    /// Problems found and worked around while parsing it.
    pub diagnostics: Diagnostics,
}

/// Parses the cross-reference table that starts at `offset`, and the trailer
/// after it:
///
/// ```text
/// xref
/// 0 2
/// 0000000000 65535 f
/// 0000000017 00000 n
/// trailer
/// << /Size 2 /Root 1 0 R >>
/// ```
///
/// Each subsection starts with the first object number and the number of entries.
/// Entries that don't follow the exact 20-byte layout are read anyway, with a
/// diagnostic, because many writers get it slightly wrong.
pub fn parse_xref_table(input: &[u8], offset: usize, limits: Limits) -> Result<ParsedXrefTable> {
    let mut reader = Reader {
        input,
        position: skip_whitespace(input, offset),
        diagnostics: Diagnostics::new(limits.max_diagnostics),
        reported_non_standard: false,
    };
    if !reader.at_keyword(b"xref") {
        return Err(Error::new(ErrorKind::NotAnXrefTable, reader.position));
    }
    reader.position += b"xref".len();

    let mut entries = BTreeMap::new();
    loop {
        reader.position = skip_whitespace(input, reader.position);
        if reader.at_keyword(b"trailer") {
            break;
        }
        match input.get(reader.position) {
            Some(byte) if byte.is_ascii_digit() => reader.subsection(&mut entries)?,
            _ => return Err(Error::new(ErrorKind::MissingTrailer, reader.position)),
        }
    }

    let trailer_start = reader.position;
    let mut parser = Parser::starting_at(input, trailer_start + b"trailer".len(), limits);
    let Some(Object::Dictionary(trailer)) = parser.next_object()? else {
        return Err(Error::new(ErrorKind::InvalidTrailer, trailer_start));
    };
    for diagnostic in parser.diagnostics().iter() {
        reader
            .diagnostics
            .push(diagnostic.kind(), diagnostic.offset());
    }

    Ok(ParsedXrefTable {
        table: XrefTable::new(entries, trailer),
        end: parser.position(),
        diagnostics: reader.diagnostics,
    })
}

struct Reader<'a> {
    input: &'a [u8],
    position: usize,
    diagnostics: Diagnostics,
    reported_non_standard: bool,
}

// One field of an entry or a subsection header, and how many digits it had.
struct Number {
    value: u64,
    digits: usize,
}

impl Reader<'_> {
    // Reads `first count` and the entries that follow.
    fn subsection(&mut self, entries: &mut BTreeMap<u32, XrefEntry>) -> Result<()> {
        let header = self.position;
        let invalid = || Error::new(ErrorKind::InvalidXrefSubsection, header);
        let first = self.number().ok_or_else(invalid)?.value;
        if self.spaces() == 0 {
            return Err(invalid());
        }
        let count = self.number().ok_or_else(invalid)?.value;
        // Object numbers must fit in 32 bits. `first + count` itself may be one past
        // the last.
        let first = u32::try_from(first).map_err(|_| invalid())?;
        if count > u64::from(u32::MAX - first) + 1 {
            return Err(invalid());
        }

        for index in 0..count {
            self.position = skip_whitespace(self.input, self.position);
            if self.at_keyword(b"trailer") {
                self.diagnostics
                    .push(DiagnosticKind::XrefSubsectionTooShort, header);
                return Ok(());
            }
            let entry_start = self.position;
            let entry = self.entry()?;
            // `count` was checked, so this can't overflow.
            let number = first + index as u32;
            match entries.entry(number) {
                btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(entry);
                }
                btree_map::Entry::Occupied(_) => self
                    .diagnostics
                    .push(DiagnosticKind::DuplicateXrefEntry, entry_start),
            }
        }
        Ok(())
    }

    // Reads `offset generation n` or `offset generation f`.
    fn entry(&mut self) -> Result<XrefEntry> {
        let start = self.position;
        let invalid = || Error::new(ErrorKind::InvalidXrefEntry, start);

        let offset = self.number().ok_or_else(invalid)?;
        let first_gap = self.spaces();
        let generation = self.number().ok_or_else(invalid)?;
        let second_gap = self.spaces();
        let kind = self.input.get(self.position).copied();
        let after = self.input.get(self.position + 1).copied();
        if first_gap == 0 || second_gap == 0 || after.is_some_and(is_regular) {
            return Err(invalid());
        }
        let generation_digits = generation.digits;
        let generation = u16::try_from(generation.value).map_err(|_| invalid())?;
        let entry = match kind {
            Some(b'n') => XrefEntry::InUse {
                offset: usize::try_from(offset.value).map_err(|_| invalid())?,
                generation,
            },
            Some(b'f') => XrefEntry::Free { generation },
            _ => return Err(invalid()),
        };
        self.position += 1;

        let line_end = self.input.get(self.position..self.position + 2);
        let standard = offset.digits == 10
            && first_gap == 1
            && generation_digits == 5
            && second_gap == 1
            && matches!(line_end, Some(b" \r" | b" \n" | b"\r\n"));
        if !standard && !self.reported_non_standard {
            self.reported_non_standard = true;
            self.diagnostics
                .push(DiagnosticKind::NonStandardXrefEntry, start);
        }
        Ok(entry)
    }

    // Reads decimal digits. `None` if there are none or the value is too large.
    fn number(&mut self) -> Option<Number> {
        let mut value: u64 = 0;
        let mut digits = 0;
        while let Some(&byte) = self.input.get(self.position).filter(|b| b.is_ascii_digit()) {
            value = value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))?;
            digits += 1;
            self.position += 1;
        }
        (digits > 0).then_some(Number { value, digits })
    }

    // Skips spaces and tabs inside a line, and returns how many there were.
    fn spaces(&mut self) -> usize {
        let start = self.position;
        while matches!(self.input.get(self.position), Some(b' ' | b'\t')) {
            self.position += 1;
        }
        self.position - start
    }

    fn at_keyword(&self, word: &[u8]) -> bool {
        self.input
            .get(self.position..)
            .is_some_and(|rest| rest.starts_with(word))
            && self
                .input
                .get(self.position + word.len())
                .is_none_or(|&byte| !is_regular(byte))
    }
}
