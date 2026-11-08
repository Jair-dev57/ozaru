use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use super::cmap::{CMapData, Code, parse_cmap};
use super::resolved;
use super::tounicode::{ToUnicode, read_to_unicode};
use crate::base::{DiagnosticKind, Diagnostics, Limits};
use crate::syntax::{Dictionary, Name, Object};
use crate::xref::Resolver;

/// How a CMap reads character codes and finds their CIDs (ISO 32000-2, section
/// 9.7.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidCMap {
    name: Option<Name>,
    form: Form,
    vertical: bool,
    codespace: Vec<(Code, Code)>,
    chars: HashMap<Code, u32>,
    // Sorted by their first code.
    ranges: Vec<(Code, Code, u32)>,
    // Codes that the mappings don't have are their own CID, as with Identity-H:
    // an embedded CMap that uses Identity-H adds to it.
    identity_fallback: bool,
}

// The kind of CMap, which decides how codes are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    // Two-byte codes that are their own CIDs.
    Identity,
    // Codes that are Unicode text, in a predefined `Uni...` CMap. Their CIDs
    // need that CMap's data, which the engine doesn't have.
    Ucs2,
    Utf16,
    Utf8,
    Utf32,
    // A CMap stream of the file, read with its codespace ranges.
    Embedded,
}

impl CidCMap {
    /// `Identity-H`, or `Identity-V` if `vertical`: two-byte codes, each its own
    /// CID.
    pub fn identity(vertical: bool) -> Self {
        let name = if vertical { "Identity-V" } else { "Identity-H" };
        Self::with_form(Some(Name::new(name)), Form::Identity, vertical)
    }

    /// A predefined CMap by its name. Only `Identity-H`, `Identity-V` and the
    /// Unicode CMaps, those whose codes are UCS-2, UTF-16, UTF-8 or UTF-32 text
    /// like `UniGB-UCS2-H` or `UniJIS-UTF16-V`, are known. The Unicode ones give
    /// the text of each code, but not its CID.
    pub fn predefined(name: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(name).ok()?;
        let (base, vertical) = match text.as_bytes().last()? {
            b'H' => (text.strip_suffix("-H")?, false),
            b'V' => (text.strip_suffix("-V")?, true),
            _ => return None,
        };
        if base == "Identity" {
            return Some(Self::identity(vertical));
        }
        if !base.starts_with("Uni") {
            return None;
        }
        let form = match base.rsplit('-').next()? {
            "UCS2" => Form::Ucs2,
            "UTF16" => Form::Utf16,
            "UTF8" => Form::Utf8,
            "UTF32" => Form::Utf32,
            _ => return None,
        };
        Some(Self::with_form(Some(Name::new(name)), form, vertical))
    }

    fn with_form(name: Option<Name>, form: Form, vertical: bool) -> Self {
        Self {
            name,
            form,
            vertical,
            codespace: Vec::new(),
            chars: HashMap::new(),
            ranges: Vec::new(),
            identity_fallback: form == Form::Identity,
        }
    }

    /// An embedded CMap, from its mappings. `parent` is the CMap it uses, with
    /// `usecmap` or `/UseCMap`, whose mappings come first.
    pub fn from_data(data: CMapData, name: Option<Name>, parent: Option<CidCMap>) -> Self {
        let mut cmap = Self::with_form(name, Form::Embedded, data.vertical);
        if let Some(parent) = parent {
            cmap.vertical |= parent.vertical;
            cmap.identity_fallback = parent.identity_fallback;
            cmap.codespace = parent.codespace;
            cmap.chars = parent.chars;
            cmap.ranges = parent.ranges;
            if parent.form == Form::Identity {
                cmap.codespace.push((
                    Code {
                        value: 0,
                        length: 2,
                    },
                    Code {
                        value: 0xFFFF,
                        length: 2,
                    },
                ));
            }
        }
        cmap.codespace.extend(data.codespace);
        cmap.chars.extend(data.cid_chars);
        cmap.ranges.extend(data.cid_ranges);
        cmap.ranges
            .sort_by_key(|(low, _, _)| (low.length, low.value));
        cmap
    }

    pub fn name(&self) -> Option<&Name> {
        self.name.as_ref()
    }

    /// Whether the CMap is for vertical writing, like `Identity-V`.
    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    /// The first code of `bytes`, and how many bytes it takes: at least one, if
    /// `bytes` isn't empty.
    ///
    /// An embedded CMap reads the shortest code that one of its codespace ranges
    /// contains. If none does, the code has the length of the shortest range
    /// whose first byte matches, or one byte, as section 9.7.6.3 says.
    pub fn next_code(&self, bytes: &[u8]) -> Option<(Code, usize)> {
        let first = *bytes.first()?;
        let length = match self.form {
            Form::Identity | Form::Ucs2 => 2,
            Form::Utf32 => 4,
            Form::Utf16 => match first {
                0xD8..=0xDB => 4,
                _ => 2,
            },
            Form::Utf8 => match first {
                0xF0..=0xF7 => 4,
                0xE0..=0xEF => 3,
                0xC0..=0xDF => 2,
                _ => 1,
            },
            Form::Embedded => self.embedded_length(bytes),
        };
        let length = length.min(bytes.len());
        Some((code(&bytes[..length]), length))
    }

    fn embedded_length(&self, bytes: &[u8]) -> usize {
        if self.codespace.is_empty() {
            return 2;
        }
        for length in 1..=4usize.min(bytes.len()) {
            let candidate = &bytes[..length];
            if self.codespace.iter().any(|(low, high)| {
                usize::from(low.length) == length && contains(*low, *high, candidate)
            }) {
                return length;
            }
        }
        self.codespace
            .iter()
            .filter(|(low, high)| contains(*low, *high, &bytes[..1]))
            .map(|(low, _)| usize::from(low.length))
            .min()
            .unwrap_or(1)
    }

    /// Splits `bytes`, a string of a content stream, into codes.
    pub fn codes(&self, bytes: &[u8]) -> Vec<Code> {
        let mut codes = Vec::new();
        let mut rest = bytes;
        while let Some((code, length)) = self.next_code(rest) {
            codes.push(code);
            rest = &rest[length..];
        }
        codes
    }

    /// The CID of `code`, or `None` if the CMap doesn't map it.
    pub fn cid(&self, code: Code) -> Option<u32> {
        if let Some(cid) = self.chars.get(&code) {
            return Some(*cid);
        }
        let candidates = self
            .ranges
            .partition_point(|(low, _, _)| (low.length, low.value) <= (code.length, code.value));
        let found = self.ranges[..candidates]
            .iter()
            .rev()
            .find(|(low, high, _)| low.length == code.length && code.value <= high.value);
        match found {
            Some((low, _, start)) => start.checked_add(code.value - low.value),
            None => (self.identity_fallback && code.length == 2).then_some(code.value),
        }
    }

    /// The text of `code` in a Unicode CMap, whose codes are text. `None` for
    /// other CMaps.
    pub fn unicode(&self, code: Code) -> Option<String> {
        let bytes = code.value.to_be_bytes();
        let bytes = &bytes[4 - usize::from(code.length)..];
        let text = match self.form {
            Form::Ucs2 | Form::Utf16 => {
                let units = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&pair| u16::from_be_bytes(pair));
                char::decode_utf16(units)
                    .collect::<Result<String, _>>()
                    .ok()?
            }
            Form::Utf8 => std::str::from_utf8(bytes).ok()?.to_string(),
            Form::Utf32 => char::from_u32(code.value)?.to_string(),
            Form::Identity | Form::Embedded => return None,
        };
        Some(text)
    }
}

// Whether `bytes` is inside the codespace range from `low` to `high`, byte by
// byte, as codespace ranges are (Adobe Technical Note #5014, section 4.4).
fn contains(low: Code, high: Code, bytes: &[u8]) -> bool {
    let length = usize::from(low.length);
    if bytes.len() > length {
        return false;
    }
    let low = low.value.to_be_bytes();
    let high = high.value.to_be_bytes();
    bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| (low[4 - length + index]..=high[4 - length + index]).contains(byte))
}

fn code(bytes: &[u8]) -> Code {
    Code {
        value: bytes
            .iter()
            .fold(0, |value, &byte| value << 8 | u32::from(byte)),
        length: bytes.len() as u8,
    }
}

/// The kind of the descendant font of a composite font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CidFontKind {
    /// `CIDFontType0`, with glyphs in CFF or Type 1 format.
    Type0,
    /// `CIDFontType2`, with glyphs in TrueType format.
    Type2,
}

/// A composite font, `/Subtype /Type0`: codes of one to four bytes, read with a
/// CMap, select the glyphs of a CIDFont by their CIDs (ISO 32000-2, section
/// 9.7).
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeFont {
    base_font: Option<Name>,
    cmap: CidCMap,
    to_unicode: Option<ToUnicode>,
    kind: CidFontKind,
    system_info: Option<(String, String, i64)>,
    default_width: f64,
    widths: HashMap<u32, f64>,
    // Sorted by their first CID.
    width_ranges: Vec<WidthRange>,
    embedded: bool,
}

impl CompositeFont {
    /// The PostScript name of the font, `/BaseFont`.
    pub fn base_font(&self) -> Option<&Name> {
        self.base_font.as_ref()
    }

    pub fn cmap(&self) -> &CidCMap {
        &self.cmap
    }

    pub fn to_unicode(&self) -> Option<&ToUnicode> {
        self.to_unicode.as_ref()
    }

    pub fn kind(&self) -> CidFontKind {
        self.kind
    }

    /// The descendant font's `/CIDSystemInfo`: its registry, ordering and
    /// supplement, like `Adobe`, `Japan1` and 6.
    pub fn system_info(&self) -> Option<(&str, &str, i64)> {
        let (registry, ordering, supplement) = self.system_info.as_ref()?;
        Some((registry, ordering, *supplement))
    }

    /// Whether the font program is in the file.
    pub fn is_embedded(&self) -> bool {
        self.embedded
    }

    /// Splits `bytes`, a string of a content stream, into codes.
    pub fn codes(&self, bytes: &[u8]) -> Vec<Code> {
        self.cmap.codes(bytes)
    }

    /// The CID of `code`.
    pub fn cid(&self, code: Code) -> Option<u32> {
        self.cmap.cid(code)
    }

    /// The text of `code`: from the ToUnicode CMap, or else from the CMap if its
    /// codes are Unicode text. `None` if neither gives any.
    pub fn text(&self, code: Code) -> Option<Cow<'_, str>> {
        if let Some(text) = self.to_unicode.as_ref().and_then(|map| map.get(code.value)) {
            return Some(text);
        }
        self.cmap.unicode(code).map(Cow::Owned)
    }

    /// The horizontal width of `code`, in text space units: the width of its CID
    /// in `/W`, or `/DW`.
    pub fn width(&self, code: Code) -> f64 {
        let width = self
            .cid(code)
            .and_then(|cid| self.cid_width(cid))
            .unwrap_or(self.default_width);
        width / 1000.0
    }

    /// The width of `cid` in `/W`, in thousandths of text space, if it has one.
    pub fn cid_width(&self, cid: u32) -> Option<f64> {
        if let Some(width) = self.widths.get(&cid) {
            return Some(*width);
        }
        let candidates = self.width_ranges.partition_point(|(low, _, _)| *low <= cid);
        self.width_ranges[..candidates]
            .iter()
            .rev()
            .find(|(_, high, _)| cid <= *high)
            .map(|(_, _, width)| *width)
    }
}

/// Reads a composite font from its font dictionary, `font`, found at `offset`.
///
/// `None` if `font` isn't `/Subtype /Type0`. Problems are worked around and
/// reported: an encoding that can't be read is `Identity-H`, a descendant font
/// that can't be read has the default widths, and invalid items of `/W` are
/// skipped.
pub fn read_composite_font(
    resolver: &mut Resolver<'_>,
    font: &Dictionary,
    limits: Limits,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> Option<CompositeFont> {
    let subtype = font.get("Subtype").map(|entry| resolved(resolver, entry));
    if subtype
        .as_deref()
        .and_then(Object::as_name)
        .map(Name::as_bytes)
        != Some(b"Type0")
    {
        return None;
    }
    let base_font = font
        .get("BaseFont")
        .map(|entry| resolved(resolver, entry))
        .and_then(|entry| entry.as_name().cloned());
    let cmap = match font.get("Encoding") {
        Some(entry) => read_cid_cmap(resolver, entry, limits, offset, diagnostics, 0),
        None => {
            diagnostics.push(DiagnosticKind::InvalidCMap, offset);
            None
        }
    };
    let cmap = cmap.unwrap_or_else(|| CidCMap::identity(false));
    let to_unicode = font
        .get("ToUnicode")
        .and_then(|entry| read_to_unicode(resolver, entry, limits, offset, diagnostics));

    let descendant = font
        .get("DescendantFonts")
        .map(|entry| resolved(resolver, entry))
        .and_then(|array| array.as_array()?.first().cloned())
        .map(|entry| resolved(resolver, &entry));
    let descendant = descendant.as_deref().and_then(Object::as_dictionary);
    let Some(descendant) = descendant else {
        diagnostics.push(DiagnosticKind::InvalidDescendantFont, offset);
        return Some(CompositeFont {
            base_font,
            cmap,
            to_unicode,
            kind: CidFontKind::Type2,
            system_info: None,
            default_width: 1000.0,
            widths: HashMap::new(),
            width_ranges: Vec::new(),
            embedded: false,
        });
    };

    let subtype = descendant
        .get("Subtype")
        .map(|entry| resolved(resolver, entry));
    let kind = match subtype
        .as_deref()
        .and_then(Object::as_name)
        .map(Name::as_bytes)
    {
        Some(b"CIDFontType0") => CidFontKind::Type0,
        Some(b"CIDFontType2") => CidFontKind::Type2,
        _ => {
            diagnostics.push(DiagnosticKind::InvalidDescendantFont, offset);
            CidFontKind::Type2
        }
    };
    let system_info = read_system_info(resolver, descendant);
    let default_width = descendant
        .get("DW")
        .and_then(|entry| resolved(resolver, entry).as_number())
        .filter(|width| width.is_finite())
        .unwrap_or(1000.0);
    let (widths, width_ranges) = match descendant.get("W") {
        Some(entry) => read_cid_widths(resolver, entry, offset, diagnostics),
        None => (HashMap::new(), Vec::new()),
    };
    let descriptor = descendant
        .get("FontDescriptor")
        .map(|entry| resolved(resolver, entry));
    let embedded = descriptor
        .as_deref()
        .and_then(Object::as_dictionary)
        .is_some_and(|descriptor| {
            ["FontFile", "FontFile2", "FontFile3"]
                .iter()
                .any(|key| descriptor.contains_key(key))
        });

    Some(CompositeFont {
        base_font,
        cmap,
        to_unicode,
        kind,
        system_info,
        default_width,
        widths,
        width_ranges,
        embedded,
    })
}

// CIDs from the first to the last, with the same width.
type WidthRange = (u32, u32, f64);

// How many CMaps can use each other, through `usecmap` or `/UseCMap`.
const MAX_CMAP_CHAIN: usize = 8;

// A font's `/Encoding`, or a CMap's `/UseCMap`: a predefined CMap's name, or an
// embedded CMap stream. None, with a diagnostic, if it can't be used.
fn read_cid_cmap(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    limits: Limits,
    offset: usize,
    diagnostics: &mut Diagnostics,
    depth: usize,
) -> Option<CidCMap> {
    if depth > MAX_CMAP_CHAIN {
        diagnostics.push(DiagnosticKind::InvalidCMap, offset);
        return None;
    }
    let id = match entry {
        Object::Reference(id) => *id,
        Object::Name(name) => return predefined(name, offset, diagnostics),
        _ => {
            diagnostics.push(DiagnosticKind::InvalidCMap, offset);
            return None;
        }
    };
    let object = resolver.resolve(id).ok();
    if let Some(Object::Name(name)) = object.as_deref() {
        return predefined(name, offset, diagnostics);
    }
    let stream_offset = resolver.object_offset(id.number);
    let (dictionary, data) = match (object.as_deref(), resolver.stream_data(id)) {
        (Some(Object::Stream(stream)), Ok(decoded)) => {
            diagnostics.append(&decoded.diagnostics);
            (stream.dictionary().clone(), decoded.data)
        }
        _ => {
            diagnostics.push(DiagnosticKind::InvalidCMap, stream_offset);
            return None;
        }
    };
    let data = parse_cmap(&data, limits);
    if data.damaged {
        diagnostics.push(DiagnosticKind::DamagedCMap, stream_offset);
    }
    let parent = match (dictionary.get("UseCMap"), &data.use_cmap) {
        (Some(parent), _) => {
            read_cid_cmap(resolver, parent, limits, offset, diagnostics, depth + 1)
        }
        (None, Some(name)) => predefined(name, stream_offset, diagnostics),
        (None, None) => None,
    };
    let name = dictionary
        .get("CMapName")
        .map(|entry| resolved(resolver, entry))
        .and_then(|name| name.as_name().cloned())
        .or_else(|| data.name.clone());
    let vertical = dictionary
        .get("WMode")
        .map(|entry| resolved(resolver, entry))
        .and_then(|mode| mode.as_integer())
        == Some(1);
    let mut cmap = CidCMap::from_data(data, name, parent);
    cmap.vertical |= vertical;
    Some(cmap)
}

fn predefined(name: &Name, offset: usize, diagnostics: &mut Diagnostics) -> Option<CidCMap> {
    let cmap = CidCMap::predefined(name.as_bytes());
    if cmap.is_none() {
        diagnostics.push(DiagnosticKind::UnsupportedCMap, offset);
    }
    cmap
}

fn read_system_info(
    resolver: &mut Resolver<'_>,
    descendant: &Dictionary,
) -> Option<(String, String, i64)> {
    let info = resolved(resolver, descendant.get("CIDSystemInfo")?);
    let info = info.as_dictionary()?;
    let mut text = |key: &str| -> Option<String> {
        let entry = resolved(resolver, info.get(key)?);
        let string = entry.as_string()?;
        Some(String::from_utf8_lossy(string.as_bytes()).into_owned())
    };
    let registry = text("Registry")?;
    let ordering = text("Ordering")?;
    let supplement = info
        .get("Supplement")
        .and_then(|entry| resolved(resolver, entry).as_integer())
        .unwrap_or(0);
    Some((registry, ordering, supplement))
}

// `/W`: `c [w1 w2 ...]` gives CIDs from c on their widths, and `c_first c_last
// w` gives all the CIDs from c_first to c_last the width w (ISO 32000-2,
// section 9.7.4.3). Reading stops at the first item that breaks that pattern.
fn read_cid_widths(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> (HashMap<u32, f64>, Vec<WidthRange>) {
    let mut widths = HashMap::new();
    let mut ranges = Vec::new();
    let array = resolved(resolver, entry);
    let Some(items) = array.as_array() else {
        diagnostics.push(DiagnosticKind::InvalidCidWidths, offset);
        return (widths, ranges);
    };
    let items: Vec<Arc<Object>> = items.iter().map(|item| resolved(resolver, item)).collect();
    let cid = |item: &Object| item.as_integer().and_then(|cid| u32::try_from(cid).ok());
    let width = |item: &Object| item.as_number().filter(|width| width.is_finite());
    let mut index = 0;
    let mut invalid = false;
    while index < items.len() {
        let Some(first) = cid(&items[index]) else {
            invalid = true;
            break;
        };
        match items.get(index + 1).map(|item| item.as_ref()) {
            Some(Object::Array(list)) => {
                for (offset, item) in list.iter().enumerate() {
                    let item = resolved(resolver, item);
                    match (
                        width(&item),
                        u32::try_from(offset)
                            .ok()
                            .and_then(|o| first.checked_add(o)),
                    ) {
                        (Some(width), Some(cid)) => {
                            widths.insert(cid, width);
                        }
                        _ => invalid = true,
                    }
                }
                index += 2;
            }
            Some(last) => {
                let range = cid(last)
                    .zip(items.get(index + 2).and_then(|item| width(item)))
                    .filter(|(last, _)| first <= *last);
                let Some((last, width)) = range else {
                    invalid = true;
                    break;
                };
                ranges.push((first, last, width));
                index += 3;
            }
            None => {
                invalid = true;
                break;
            }
        }
    }
    if invalid {
        diagnostics.push(DiagnosticKind::InvalidCidWidths, offset);
    }
    ranges.sort_by_key(|(first, _, _)| *first);
    (widths, ranges)
}
