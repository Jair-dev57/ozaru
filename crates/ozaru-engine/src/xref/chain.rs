use std::collections::{BTreeMap, HashSet};

use super::{
    ParsedXrefTable, XrefEntry, XrefTable, find_startxref, parse_xref_stream, parse_xref_table,
};
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object};

/// The cross-reference data of a whole file, loaded by [`load_xref`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedXref {
    /// The entries of every section, the newest winning, and the newest trailer.
    pub table: XrefTable,
    /// Number of sections read, counting the `/XRefStm` of hybrid files.
    pub sections: usize,
    /// Problems found and worked around while loading it.
    pub diagnostics: Diagnostics,
}

/// Parses the cross-reference section at `offset`: a classic table if it starts
/// with `xref`, a cross-reference stream otherwise.
pub fn parse_xref_section(input: &[u8], offset: usize, limits: Limits) -> Result<ParsedXrefTable> {
    match parse_xref_table(input, offset, limits) {
        Err(error) if error.kind() == ErrorKind::NotAnXrefTable => {
            parse_xref_stream(input, offset, limits)
        }
        result => result,
    }
}

/// Loads the cross-reference data of a file (ISO 32000-2, sections 7.5.5 to
/// 7.5.8).
///
/// It starts at the section that `startxref` points at and follows the `/Prev`
/// of each trailer back to the original one. Each incremental update appends a
/// section, so for an object listed in several of them, the newest entry wins.
///
/// A hybrid file has a classic table whose trailer also has `/XRefStm`: a
/// cross-reference stream with the objects that older readers don't know about.
/// Its entries come after those in use in the table, and before `/Prev`.
///
/// Only the section that `startxref` points at must be readable. If an older one
/// is broken or the chain loops, the entries read so far are kept, with a
/// diagnostic. A `/Prev` or `/XRefStm` past the end of the file is reported at
/// the end of the file.
pub fn load_xref(input: &[u8], limits: Limits) -> Result<LoadedXref> {
    let startxref = find_startxref(input, limits)?;
    let mut diagnostics = startxref.diagnostics;
    let mut entries = BTreeMap::new();
    let mut trailer: Option<Dictionary> = None;
    let mut visited = HashSet::new();
    let mut sections = 0;
    let mut next = Some(startxref.offset);

    while let Some(offset) = next.take() {
        if !visited.insert(offset) {
            diagnostics.push(DiagnosticKind::XrefLoop, offset);
            break;
        }
        if sections >= limits.max_xref_sections {
            return Err(Error::new(
                ErrorKind::LimitExceeded(Limit::XrefSections),
                offset,
            ));
        }
        let section = match parse_xref_section(input, offset, limits) {
            Ok(section) => section,
            Err(error) if trailer.is_none() => return Err(error),
            Err(_) => {
                diagnostics.push(DiagnosticKind::BrokenXrefSection, offset.min(input.len()));
                break;
            }
        };
        sections += 1;
        diagnostics.append(&section.diagnostics);
        let (mut section_entries, section_trailer) = section.table.into_parts();

        if let Some(stream_offset) = offset_entry(&section_trailer, "XRefStm") {
            match parse_xref_stream(input, stream_offset, limits) {
                Ok(stream) => {
                    sections += 1;
                    diagnostics.append(&stream.diagnostics);
                    for (number, entry) in stream.table.iter() {
                        let in_use_in_table = matches!(
                            section_entries.get(&number),
                            Some(XrefEntry::InUse { .. } | XrefEntry::Compressed { .. })
                        );
                        if !in_use_in_table {
                            section_entries.insert(number, entry);
                        }
                    }
                }
                Err(_) => diagnostics.push(
                    DiagnosticKind::BrokenXrefSection,
                    stream_offset.min(input.len()),
                ),
            }
        }

        for (number, entry) in section_entries {
            entries.entry(number).or_insert(entry);
        }
        if section_trailer.get("Prev").is_some() {
            next = offset_entry(&section_trailer, "Prev");
            if next.is_none() {
                diagnostics.push(DiagnosticKind::BrokenXrefSection, offset.min(input.len()));
            }
        }
        trailer.get_or_insert(section_trailer);
    }

    Ok(LoadedXref {
        table: XrefTable::new(entries, trailer.unwrap_or_default()),
        sections,
        diagnostics,
    })
}

// A trailer entry holding a file offset.
fn offset_entry(trailer: &Dictionary, key: &str) -> Option<usize> {
    trailer
        .get(key)
        .and_then(Object::as_integer)
        .and_then(|offset| usize::try_from(offset).ok())
}
