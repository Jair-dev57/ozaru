use std::collections::BTreeMap;

use super::{LoadedXref, XrefEntry, XrefTable, parse_object_stream};
use crate::base::{Diagnostics, Error, ErrorKind, Limits, Result};
use crate::syntax::chars::{is_regular, is_whitespace};
use crate::syntax::{Dictionary, Name, Object, ObjectId, Parser, parse_indirect_object};

/// How far after `trailer`, or after the header of a catalog, its dictionary is
/// parsed. It stops a damaged file full of `trailer <<` from being parsed to the
/// end again and again.
const DICTIONARY_WINDOW: usize = 64 * 1024;

/// How many bytes after an object header are searched for `/Catalog`, when no
/// trailer gives the root.
const CATALOG_WINDOW: usize = 1024;

/// Rebuilds the cross-reference data of a damaged file by scanning it for
/// `n g obj` headers, as PDF readers do when the cross-reference data can't be
/// trusted.
///
/// - **Objects:** each header is an entry in use at its offset. When an object
///   appears several times, the last one wins, since incremental updates append
///   newer versions. Stream data is skipped up to its `endstream`, so binary data
///   that happens to look like a header isn't taken for one.
/// - **Object streams:** the objects inside them become compressed entries,
///   unless the file also has the object outside of any object stream.
/// - **Trailer:** the last `trailer` dictionary, or cross-reference stream
///   dictionary, whose `/Root` is one of the objects found. If there's none, a
///   trailer is made with the last `/Type /Catalog` object outside of object
///   streams as `/Root`.
///
/// It is an error only if the file has no objects at all. The scan reads the file
/// once, so its time grows linearly with the size of the file.
pub fn rebuild_xref(input: &[u8], limits: Limits) -> Result<LoadedXref> {
    let scan = scan(input, limits);
    if scan.entries.is_empty() {
        return Err(Error::new(ErrorKind::NoObjectsFound, 0));
    }
    let mut entries = scan.entries;
    let mut trailers = scan.trailers;

    for &offset in &scan.object_streams {
        let Ok(parsed) = parse_indirect_object(input, offset, limits) else {
            continue;
        };
        let number = parsed.object.id.number;
        let Some(stream) = parsed.object.object.as_stream() else {
            continue;
        };
        let Ok(object_stream) = parse_object_stream(stream, input, limits) else {
            continue;
        };
        let indexes = (0..object_stream.stream.len()).filter_map(|i| u32::try_from(i).ok());
        for index in indexes {
            if let Some(inner) = object_stream.stream.number(index) {
                entries.entry(inner).or_insert(XrefEntry::Compressed {
                    stream: number,
                    index,
                });
            }
        }
    }

    for &offset in &scan.xref_streams {
        if let Ok(parsed) = parse_indirect_object(input, offset, limits)
            && let Some(stream) = parsed.object.object.as_stream()
        {
            trailers.push((offset, stream.dictionary().clone()));
        }
    }
    trailers.sort_by_key(|&(offset, _)| offset);

    let trailer = trailers
        .into_iter()
        .rev()
        .map(|(_, trailer)| trailer)
        .find(|trailer| root_is_listed(trailer, &entries))
        .or_else(|| catalog_trailer(input, &entries, limits))
        .unwrap_or_default();

    Ok(LoadedXref {
        table: XrefTable::new(entries, trailer),
        sections: 0,
        diagnostics: Diagnostics::new(limits.max_diagnostics),
    })
}

struct Scan {
    entries: BTreeMap<u32, XrefEntry>,
    // Offsets of the headers of streams that look like object streams or
    // cross-reference streams.
    object_streams: Vec<usize>,
    xref_streams: Vec<usize>,
    trailers: Vec<(usize, Dictionary)>,
}

fn scan(input: &[u8], limits: Limits) -> Scan {
    let endstreams: Vec<usize> = find_all(input, b"endstream");
    let mut scan = Scan {
        entries: BTreeMap::new(),
        object_streams: Vec::new(),
        xref_streams: Vec::new(),
        trailers: Vec::new(),
    };
    // The header of the object being scanned, to know what a stream belongs to.
    let mut current: Option<usize> = None;
    let mut position = 0;

    while position < input.len() {
        let rest = &input[position..];
        if rest.starts_with(b"obj")
            && is_word_end(input, position + 3)
            && let Some((start, id)) = header_before(input, position)
        {
            scan.entries.insert(
                id.number,
                XrefEntry::InUse {
                    offset: start,
                    generation: id.generation,
                },
            );
            current = Some(start);
            position += 3;
        } else if rest.starts_with(b"stream")
            && is_word_start(input, position)
            && matches!(input.get(position + 6), Some(b'\r' | b'\n'))
        {
            if let Some(header) = current.take() {
                let dictionary = &input[header..position];
                if contains(dictionary, b"/ObjStm") {
                    scan.object_streams.push(header);
                } else if contains(dictionary, b"/XRef") && !contains(dictionary, b"/XRefStm") {
                    scan.xref_streams.push(header);
                }
            }
            // Skip the data, which can be binary.
            let after = endstreams.partition_point(|&end| end < position + 6);
            position = match endstreams.get(after) {
                Some(&end) => end + b"endstream".len(),
                None => position + 6,
            };
        } else if rest.starts_with(b"trailer")
            && is_word_start(input, position)
            && is_word_end(input, position + 7)
        {
            let window = &input[..input.len().min(position + DICTIONARY_WINDOW)];
            let mut parser = Parser::starting_at(window, position + 7, limits);
            if let Ok(Some(Object::Dictionary(trailer))) = parser.next_object() {
                scan.trailers.push((position, trailer));
            }
            position += 7;
        } else {
            position += 1;
        }
    }
    scan
}

// If `obj` at `keyword` follows `n g` and whitespace, returns where `n` starts and
// the object's number and generation.
fn header_before(input: &[u8], keyword: usize) -> Option<(usize, ObjectId)> {
    let generation_end = skip_whitespace_back(input, keyword)?;
    let generation_start = skip_digits_back(input, generation_end)?;
    let number_end = skip_whitespace_back(input, generation_start)?;
    let number_start = skip_digits_back(input, number_end)?;
    if !is_word_start(input, number_start) {
        return None;
    }
    let number = parse_decimal(&input[number_start..number_end])?;
    let generation = parse_decimal(&input[generation_start..generation_end])?;
    Some((
        number_start,
        ObjectId::new(u32::try_from(number).ok()?, u16::try_from(generation).ok()?),
    ))
}

// Returns where the whitespace before `end` starts, if there is any.
fn skip_whitespace_back(input: &[u8], end: usize) -> Option<usize> {
    let mut start = end;
    while start > 0 && is_whitespace(input[start - 1]) {
        start -= 1;
    }
    (start < end).then_some(start)
}

// Returns where the digits before `end` start, if there are any.
fn skip_digits_back(input: &[u8], end: usize) -> Option<usize> {
    let mut start = end;
    while start > 0 && input[start - 1].is_ascii_digit() {
        start -= 1;
    }
    (start < end).then_some(start)
}

fn parse_decimal(digits: &[u8]) -> Option<u64> {
    digits.iter().try_fold(0u64, |value, &digit| {
        value.checked_mul(10)?.checked_add(u64::from(digit - b'0'))
    })
}

fn is_word_start(input: &[u8], position: usize) -> bool {
    position == 0 || !is_regular(input[position - 1])
}

fn is_word_end(input: &[u8], position: usize) -> bool {
    input.get(position).is_none_or(|&byte| !is_regular(byte))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn find_all(input: &[u8], needle: &[u8]) -> Vec<usize> {
    input
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(position, _)| position)
        .collect()
}

// A trailer is only useful if its root is one of the objects found.
fn root_is_listed(trailer: &Dictionary, entries: &BTreeMap<u32, XrefEntry>) -> bool {
    trailer
        .get("Root")
        .and_then(Object::as_reference)
        .is_some_and(|root| entries.contains_key(&root.number))
}

// A trailer whose `/Root` is the last object found with `/Type /Catalog`.
fn catalog_trailer(
    input: &[u8],
    entries: &BTreeMap<u32, XrefEntry>,
    limits: Limits,
) -> Option<Dictionary> {
    let mut catalogs: Vec<(usize, ObjectId)> = entries
        .iter()
        .filter_map(|(&number, &entry)| match entry {
            XrefEntry::InUse { offset, generation } => {
                let end = input.len().min(offset + CATALOG_WINDOW);
                contains(&input[offset..end], b"/Catalog")
                    .then_some((offset, ObjectId::new(number, generation)))
            }
            _ => None,
        })
        .collect();
    catalogs.sort();

    let root = catalogs.into_iter().rev().find_map(|(offset, id)| {
        let window = &input[..input.len().min(offset + DICTIONARY_WINDOW)];
        let parsed = parse_indirect_object(window, offset, limits).ok()?;
        let kind = parsed
            .object
            .object
            .as_dictionary()?
            .get("Type")?
            .as_name()?;
        (kind.as_bytes() == b"Catalog").then_some(id)
    })?;
    let mut trailer = Dictionary::new();
    trailer.insert(Name::from("Root"), Object::Reference(root));
    Some(trailer)
}
