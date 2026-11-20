use std::borrow::Cow;
use std::collections::HashMap;

use super::cmap::{Code, Destination, RangeDestination, parse_cmap};
use super::glyphlist::{GlyphList, glyph_to_unicode};
use crate::base::{DiagnosticKind, Diagnostics, Limits};
use crate::syntax::Object;
use crate::xref::Resolver;

/// A font's ToUnicode CMap: the text of each character code (ISO 32000-2,
/// section 9.10.3).
///
/// When a font has one, it is the most reliable way to get its text. A code can
/// map to several characters, such as `ffi` for a ligature, or to none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToUnicode {
    codespace: Vec<(Code, Code)>,
    chars: HashMap<u32, String>,
    // Sorted by their first code. Each range maps `low..=high`.
    ranges: Vec<Range>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Range {
    low: u32,
    high: u32,
    destination: RangeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RangeTarget {
    // UTF-16 code units of the text of `low`. Each next code adds one to the last.
    Start(Vec<u16>),
    List(Vec<String>),
}

impl ToUnicode {
    /// Reads the CMap in `data`. Entries that can't be read are skipped and
    /// reported with a `DamagedCMap` diagnostic at `offset`.
    pub fn parse(
        data: &[u8],
        limits: Limits,
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Self {
        let cmap = parse_cmap(data, limits);
        let mut damaged = cmap.damaged;

        let mut chars = HashMap::with_capacity(cmap.chars.len());
        for (code, destination) in &cmap.chars {
            chars.insert(code.value, text(destination));
        }
        let mut ranges = Vec::with_capacity(cmap.ranges.len());
        for (low, high, destination) in cmap.ranges {
            let destination = match destination {
                RangeDestination::Start(bytes) => {
                    let units = utf16_units(&bytes);
                    if units.is_empty() {
                        damaged = true;
                        continue;
                    }
                    RangeTarget::Start(units)
                }
                RangeDestination::List(list) => RangeTarget::List(list.iter().map(text).collect()),
            };
            ranges.push(Range {
                low: low.value,
                high: high.value,
                destination,
            });
        }
        // Stable, so that of two ranges starting at the same code, the later one
        // is found first when looking backwards.
        ranges.sort_by_key(|range| range.low);

        if damaged {
            diagnostics.push(DiagnosticKind::DamagedCMap, offset);
        }
        Self {
            codespace: cmap.codespace,
            chars,
            ranges,
        }
    }

    /// The text of `code`, or `None` if the CMap doesn't map it. A `bfchar` entry
    /// takes precedence over a `bfrange`, and of overlapping ranges, the one
    /// starting closest to `code` is used.
    ///
    /// Only the value of the code matters, not its length: some files write
    /// the codes of a single-byte font with two bytes.
    pub fn get(&self, code: u32) -> Option<Cow<'_, str>> {
        if let Some(text) = self.chars.get(&code) {
            return Some(Cow::Borrowed(text));
        }
        let candidates = self.ranges.partition_point(|range| range.low <= code);
        let range = self.ranges[..candidates]
            .iter()
            .rev()
            .find(|range| code <= range.high)?;
        let index = code - range.low;
        match &range.destination {
            RangeTarget::Start(units) => {
                let mut units = units.clone();
                let last = units.last_mut()?;
                *last = u16::try_from(u32::from(*last) + index).ok()?;
                Some(Cow::Owned(decode_utf16(&units)))
            }
            RangeTarget::List(list) => {
                let text = list.get(usize::try_from(index).ok()?)?;
                Some(Cow::Borrowed(text))
            }
        }
    }

    /// The `codespacerange` entries, as pairs of the first and last codes.
    pub fn codespace(&self) -> &[(Code, Code)] {
        &self.codespace
    }

    /// Whether the CMap maps no code at all.
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty() && self.ranges.is_empty()
    }
}

/// Reads a font's `/ToUnicode` entry, a reference to a stream. None if the font
/// has none; if the entry isn't a stream, or its data can't be decoded, it is
/// also None, with an `InvalidToUnicode` diagnostic at `offset`, the font's
/// offset.
pub fn read_to_unicode(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    limits: Limits,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> Option<ToUnicode> {
    let Object::Reference(id) = entry else {
        diagnostics.push(DiagnosticKind::InvalidToUnicode, offset);
        return None;
    };
    let offset = resolver.object_offset(id.number);
    match resolver.stream_data(*id) {
        Ok(decoded) => {
            diagnostics.append(&decoded.diagnostics);
            Some(ToUnicode::parse(&decoded.data, limits, offset, diagnostics))
        }
        Err(_) => {
            diagnostics.push(DiagnosticKind::InvalidToUnicode, offset);
            None
        }
    }
}

// The text of a destination: UTF-16BE, or a glyph name.
fn text(destination: &Destination) -> String {
    match destination {
        Destination::String(bytes) => decode_utf16(&utf16_units(bytes)),
        Destination::Name(name) => {
            glyph_to_unicode(name.as_bytes(), GlyphList::Adobe).unwrap_or_default()
        }
    }
}

// The UTF-16BE code units of `bytes`. An odd first byte is a unit on its own,
// as some writers use `<20>` for a space.
fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    let (first, rest) = match bytes.len() % 2 {
        1 => (Some(u16::from(bytes[0])), &bytes[1..]),
        _ => (None, bytes),
    };
    first
        .into_iter()
        .chain(
            rest.as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_be_bytes(pair)),
        )
        .collect()
}

fn decode_utf16(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}
