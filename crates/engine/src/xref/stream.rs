use std::collections::{BTreeMap, btree_map};

use super::{ParsedXrefTable, XrefEntry, XrefTable};
use crate::base::{DiagnosticKind, Error, ErrorKind, Limits, Result};
use crate::filters::decode_stream;
use crate::syntax::{Object, parse_indirect_object};

/// Parses the cross-reference stream at `offset` (ISO 32000-2, section 7.5.8):
/// an indirect object like
///
/// ```text
/// 20 0 obj
/// << /Type /XRef /Size 21 /W [1 2 2] /Filter /FlateDecode /Length 81 >>
/// stream
/// ...
/// endstream
/// endobj
/// ```
///
/// Its data is a list of binary entries. `/W` gives the width in bytes of their
/// three fields: the type, then two values that depend on it. `/Index` lists the
/// object numbers they describe, as pairs of first number and count, and is
/// `[0 Size]` by default. The stream dictionary is the trailer.
pub fn parse_xref_stream(input: &[u8], offset: usize, limits: Limits) -> Result<ParsedXrefTable> {
    let parsed = parse_indirect_object(input, offset, limits)?;
    let mut diagnostics = parsed.diagnostics;
    let not_a_stream = || Error::new(ErrorKind::NotAnXrefStream, offset);
    let Object::Stream(stream) = parsed.object.object else {
        return Err(not_a_stream());
    };
    let dictionary = stream.dictionary();
    let kind = dictionary.get("Type").and_then(Object::as_name);
    if kind.is_none_or(|kind| kind.as_bytes() != b"XRef") {
        return Err(not_a_stream());
    }

    let invalid = || Error::new(ErrorKind::InvalidXrefStream, offset);
    let widths = widths(dictionary.get("W")).ok_or_else(invalid)?;
    let size = dictionary
        .get("Size")
        .and_then(Object::as_integer)
        .and_then(|size| u64::try_from(size).ok())
        .ok_or_else(invalid)?;
    let subsections = subsections(dictionary.get("Index"), size).ok_or_else(invalid)?;

    let decoded = decode_stream(&stream, input, limits)?;
    diagnostics.append(&decoded.diagnostics);

    let invalid_entry = || Error::new(ErrorKind::InvalidXrefEntry, offset);
    let mut rows = decoded.data.chunks_exact(widths.iter().sum());
    let mut entries = BTreeMap::new();
    let mut reported_unknown = false;
    'subsections: for (first, count) in subsections {
        for index in 0..count {
            let Some(row) = rows.next() else {
                diagnostics.push(DiagnosticKind::XrefStreamTooShort, offset);
                break 'subsections;
            };
            let [kind, second, third] = fields(row, widths);
            let entry = match kind {
                0 => XrefEntry::Free {
                    // The generation of a free entry is only a hint, so a value too
                    // large to be one isn't worth an error.
                    generation: u16::try_from(third).unwrap_or(u16::MAX),
                },
                1 => XrefEntry::InUse {
                    offset: usize::try_from(second).map_err(|_| invalid_entry())?,
                    generation: u16::try_from(third).map_err(|_| invalid_entry())?,
                },
                2 => XrefEntry::Compressed {
                    stream: u32::try_from(second).map_err(|_| invalid_entry())?,
                    index: u32::try_from(third).map_err(|_| invalid_entry())?,
                },
                // The standard says to treat other types as references to the null
                // object, which is the same as having no entry.
                _ => {
                    if !reported_unknown {
                        reported_unknown = true;
                        diagnostics.push(DiagnosticKind::UnknownXrefEntryType, offset);
                    }
                    continue;
                }
            };
            // `subsections` checked that the numbers fit in 32 bits.
            let number = first + index as u32;
            match entries.entry(number) {
                btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(entry);
                }
                btree_map::Entry::Occupied(_) => {
                    diagnostics.push(DiagnosticKind::DuplicateXrefEntry, offset);
                }
            }
        }
    }

    Ok(ParsedXrefTable {
        table: XrefTable::new(entries, dictionary.clone()),
        end: parsed.end,
        diagnostics,
    })
}

// `/W`: three widths of at most 8 bytes, so each field fits in a u64, and not all
// zero.
fn widths(value: Option<&Object>) -> Option<[usize; 3]> {
    let [first, second, third] = value?.as_array()? else {
        return None;
    };
    let width = |value: &Object| {
        let width = usize::try_from(value.as_integer()?).ok()?;
        (width <= 8).then_some(width)
    };
    let widths = [width(first)?, width(second)?, width(third)?];
    (widths.iter().sum::<usize>() > 0).then_some(widths)
}

// `/Index`: pairs of first object number and count, `[0 Size]` by default. Every
// object number must fit in 32 bits.
fn subsections(value: Option<&Object>, size: u64) -> Option<Vec<(u32, u64)>> {
    let pairs: Vec<(u64, u64)> = match value {
        None => vec![(0, size)],
        Some(value) => {
            let (pairs, []) = value.as_array()?.as_chunks::<2>() else {
                return None;
            };
            let integer = |item: &Object| u64::try_from(item.as_integer()?).ok();
            pairs
                .iter()
                .map(|[first, count]| Some((integer(first)?, integer(count)?)))
                .collect::<Option<_>>()?
        }
    };
    pairs
        .into_iter()
        .map(|(first, count)| {
            (first.checked_add(count)? <= u64::from(u32::MAX) + 1)
                .then_some((u32::try_from(first).ok()?, count))
        })
        .collect()
}

// Reads the three big-endian fields of an entry. A field of width 0 takes its
// default: type 1 for the first, 0 for the others.
fn fields(row: &[u8], widths: [usize; 3]) -> [u64; 3] {
    let mut values = [1, 0, 0];
    let mut start = 0;
    for (value, width) in values.iter_mut().zip(widths) {
        if width > 0 {
            let bytes = &row[start..start + width];
            *value = bytes
                .iter()
                .fold(0, |value, &byte| value << 8 | u64::from(byte));
        }
        start += width;
    }
    values
}
