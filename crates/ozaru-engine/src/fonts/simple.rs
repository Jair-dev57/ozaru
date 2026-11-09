use std::borrow::Cow;

use super::embedded::{ProgramKind, TrueTypeProgram, font_program, type1_encoding};
use super::encoding::{BaseEncoding, Encoding, read_encoding};
use super::extent::{Extent, read_extent};
use super::glyphlist::{GlyphList, glyph_to_unicode};
use super::metrics::StandardFont;
use super::resolved;
use super::tounicode::{ToUnicode, read_to_unicode};
use crate::base::{DiagnosticKind, Diagnostics, Limits};
use crate::syntax::{Dictionary, Name, Object};
use crate::xref::Resolver;

/// The kind of a simple font, its `/Subtype`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimpleFontKind {
    Type1,
    /// A multiple master Type 1 font, read as a Type 1 font.
    MMType1,
    TrueType,
    /// A font whose glyphs are content streams. Its widths are in its own glyph
    /// space, scaled by its `/FontMatrix`.
    Type3,
}

/// A simple font: one byte per character code, up to 256 glyphs (ISO 32000-2,
/// sections 9.6 and 9.7).
///
/// It knows the text and the width of each code. The width is in text space
/// units, so a glyph 500 units wide in a Type 1 font is 0.5.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleFont {
    kind: SimpleFontKind,
    base_font: Option<Name>,
    standard: Option<StandardFont>,
    encoding: Option<Encoding>,
    glyph_list: GlyphList,
    to_unicode: Option<ToUnicode>,
    first_char: u8,
    // In glyph space: thousandths of text space, or Type 3 glyph space units.
    widths: Vec<f64>,
    missing_width: f64,
    font_matrix: [f64; 6],
    flags: u32,
    embedded: bool,
    // The embedded TrueType program, for symbolic fonts without `/Encoding`.
    program: Option<TrueTypeProgram>,
    extent: Extent,
}

impl SimpleFont {
    pub fn kind(&self) -> SimpleFontKind {
        self.kind
    }

    /// The PostScript name of the font, `/BaseFont`, with the subset prefix if it
    /// has one, like `ABCDEF+DejaVuSans`.
    pub fn base_font(&self) -> Option<&Name> {
        self.base_font.as_ref()
    }

    /// The encoding of codes to glyph names. `None` for a symbolic TrueType
    /// font without `/Encoding`: its codes select glyphs through the font
    /// program's own tables instead (ISO 32000-2, section 9.6.5.4).
    /// The standard 14 font that `/BaseFont` names, if any, also under the names
    /// that writers commonly use for them, like `Arial`.
    pub fn standard_font(&self) -> Option<StandardFont> {
        self.standard
    }

    pub fn encoding(&self) -> Option<&Encoding> {
        self.encoding.as_ref()
    }

    pub fn to_unicode(&self) -> Option<&ToUnicode> {
        self.to_unicode.as_ref()
    }

    /// The font descriptor's `/Flags` (ISO 32000-2, table 121). 0 if there is
    /// none.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Whether the font is symbolic: its glyphs aren't all in the standard Latin
    /// character set. Flag 3 of `/Flags`.
    pub fn is_symbolic(&self) -> bool {
        self.flags & 4 != 0
    }

    /// Whether the font program is in the file, in `/FontFile`, `/FontFile2` or
    /// `/FontFile3` of the font descriptor.
    pub fn is_embedded(&self) -> bool {
        self.embedded
    }

    /// The glyph name of `code`, from the encoding.
    pub fn glyph_name(&self, code: u8) -> Option<&[u8]> {
        self.encoding.as_ref()?.glyph_name(code)
    }

    /// The text of `code`: from the ToUnicode CMap if it maps the code, and from
    /// its glyph name and the Adobe Glyph List otherwise (ISO 32000-2, section
    /// 9.10.2). A symbolic TrueType font without `/Encoding` has no glyph names,
    /// so its embedded program says which character the code's glyph is. `None`
    /// if none of them gives any.
    pub fn text(&self, code: u8) -> Option<Cow<'_, str>> {
        if let Some(text) = self
            .to_unicode
            .as_ref()
            .and_then(|map| map.get(u32::from(code)))
        {
            return Some(text);
        }
        if self.encoding.is_none() {
            let text = self.program.as_ref()?.symbol_text(code)?;
            return Some(Cow::Borrowed(text));
        }
        let name = self.glyph_name(code)?;
        glyph_to_unicode(name, self.glyph_list).map(Cow::Owned)
    }

    /// The horizontal width of `code`, in text space units, scaled by the font
    /// matrix: its entry in `/Widths`; for a standard 14 font without `/Widths`,
    /// the width of its glyph in Adobe's metrics; and otherwise the descriptor's
    /// `/MissingWidth`.
    pub fn width(&self, code: u8) -> f64 {
        let width = self
            .listed_width(code)
            .or_else(|| self.standard_width(code))
            .unwrap_or(self.missing_width);
        // Dividing by 1000 rounds better than multiplying by 0.001: 700 gives 0.7,
        // not 0.7000000000000001.
        if self.font_matrix[0] == DEFAULT_MATRIX[0] {
            width / 1000.0
        } else {
            width * self.font_matrix[0]
        }
    }

    /// Whether `/Widths`, or for a standard 14 font without it Adobe's metrics,
    /// give the width of `code`. When neither does, `width` uses `/MissingWidth`.
    pub fn has_width(&self, code: u8) -> bool {
        self.listed_width(code).is_some() || self.standard_width(code).is_some()
    }

    fn listed_width(&self, code: u8) -> Option<f64> {
        let index = usize::from(code).checked_sub(usize::from(self.first_char))?;
        self.widths.get(index).copied()
    }

    // The metrics of a standard font only count when the font has no widths.
    fn standard_width(&self, code: u8) -> Option<f64> {
        if !self.widths.is_empty() {
            return None;
        }
        self.standard?.glyph_width(self.glyph_name(code)?)
    }

    /// The font matrix, from glyph space to text space. `[0.001 0 0 0.001 0 0]`
    /// for every kind but Type 3.
    pub fn font_matrix(&self) -> [f64; 6] {
        self.font_matrix
    }

    /// How far its glyphs reach above and below the baseline.
    pub fn extent(&self) -> Extent {
        self.extent
    }
}

const DEFAULT_MATRIX: [f64; 6] = [0.001, 0.0, 0.0, 0.001, 0.0, 0.0];

/// Reads a simple font from its font dictionary, `font`, found at `offset`.
///
/// `None` if `font` is a composite font, `/Subtype /Type0`. Problems are worked
/// around and reported: a missing or unknown `/Subtype` is read as Type 1, and
/// invalid widths or font matrix are replaced by the defaults.
pub fn read_simple_font(
    resolver: &mut Resolver<'_>,
    font: &Dictionary,
    limits: Limits,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> Option<SimpleFont> {
    let subtype = font.get("Subtype").map(|entry| resolved(resolver, entry));
    let kind = match subtype
        .as_deref()
        .and_then(Object::as_name)
        .map(Name::as_bytes)
    {
        Some(b"Type0") => return None,
        Some(b"Type1") => SimpleFontKind::Type1,
        Some(b"MMType1") => SimpleFontKind::MMType1,
        Some(b"TrueType") => SimpleFontKind::TrueType,
        Some(b"Type3") => SimpleFontKind::Type3,
        _ => {
            diagnostics.push(DiagnosticKind::UnknownFontType, offset);
            SimpleFontKind::Type1
        }
    };
    let base_font = font
        .get("BaseFont")
        .map(|entry| resolved(resolver, entry))
        .and_then(|entry| entry.as_name().cloned());

    let descriptor = font
        .get("FontDescriptor")
        .map(|entry| resolved(resolver, entry));
    let descriptor = descriptor.as_deref().and_then(Object::as_dictionary);
    let mut number = |dictionary: Option<&Dictionary>, key: &str| {
        let entry = dictionary?.get(key)?;
        resolved(resolver, entry).as_number()
    };
    let flags = number(descriptor, "Flags")
        .filter(|flags| (0.0..=f64::from(u32::MAX)).contains(flags))
        .map_or(0, |flags| flags as u32);
    let missing_width = number(descriptor, "MissingWidth")
        .filter(|width| width.is_finite())
        .unwrap_or(0.0);
    let embedded = descriptor.is_some_and(|descriptor| {
        ["FontFile", "FontFile2", "FontFile3"]
            .iter()
            .any(|key| descriptor.contains_key(key))
    });

    let standard = base_font
        .as_ref()
        .and_then(|name| StandardFont::from_base_font(name.as_bytes()));
    let base = standard.map_or(BaseEncoding::Standard, StandardFont::built_in_encoding);
    let symbolic = flags & 4 != 0;
    let encoding_entry = font.get("Encoding").map(|entry| resolved(resolver, entry));
    let names_base = matches!(encoding_entry.as_deref(), Some(Object::Name(_)))
        || matches!(encoding_entry.as_deref(), Some(Object::Dictionary(d)) if d.contains_key("BaseEncoding"));
    let unencoded_truetype =
        kind == SimpleFontKind::TrueType && symbolic && encoding_entry.is_none();

    // The embedded program is only read when the font's own entries don't say
    // enough: a Type 1 font without a base encoding uses the program's, and a
    // symbolic TrueType font without an encoding uses its tables.
    let program = match descriptor {
        Some(descriptor) if !names_base => font_program(resolver, descriptor),
        _ => None,
    };
    let mut built_in = Encoding::new(base);
    let mut truetype = None;
    match program {
        Some((ProgramKind::Type1, data))
            if matches!(kind, SimpleFontKind::Type1 | SimpleFontKind::MMType1) =>
        {
            if let Some(encoding) = type1_encoding(&data, limits) {
                built_in = encoding;
            }
        }
        Some((ProgramKind::TrueType, data)) if unencoded_truetype => {
            truetype = TrueTypeProgram::parse(&data).filter(|program| !program.is_empty());
        }
        _ => {}
    }
    let encoding = match &encoding_entry {
        Some(entry) => Some(read_encoding(
            resolver,
            entry,
            &built_in,
            offset,
            diagnostics,
        )),
        None if unencoded_truetype => None,
        None => Some(built_in),
    };
    let glyph_list = match standard {
        Some(StandardFont::ZapfDingbats) => GlyphList::ZapfDingbats,
        _ => GlyphList::Adobe,
    };
    let to_unicode = font
        .get("ToUnicode")
        .and_then(|entry| read_to_unicode(resolver, entry, limits, offset, diagnostics));

    let (first_char, widths) = read_widths(resolver, font, offset, diagnostics);
    let font_matrix = match kind {
        SimpleFontKind::Type3 => read_font_matrix(resolver, font, offset, diagnostics),
        _ => DEFAULT_MATRIX,
    };
    let extent = read_extent(resolver, font, descriptor, standard, font_matrix[3]);

    Some(SimpleFont {
        kind,
        base_font,
        standard,
        encoding,
        glyph_list,
        to_unicode,
        first_char,
        widths,
        missing_width,
        font_matrix,
        flags,
        embedded,
        program: truetype,
        extent,
    })
}

// `/FirstChar` and `/Widths`, as numbers. Widths past code 255 are dropped. If
// either is invalid, there are no widths, and the problem is reported.
fn read_widths(
    resolver: &mut Resolver<'_>,
    font: &Dictionary,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> (u8, Vec<f64>) {
    let Some(entry) = font.get("Widths") else {
        return (0, Vec::new());
    };
    let array = resolved(resolver, entry);
    let first = font
        .get("FirstChar")
        .map(|entry| resolved(resolver, entry))
        .and_then(|entry| entry.as_integer())
        .and_then(|first| u8::try_from(first).ok());
    let (Some(items), Some(first)) = (array.as_array(), first) else {
        diagnostics.push(DiagnosticKind::InvalidWidths, offset);
        return (0, Vec::new());
    };
    let count = items.len().min(256 - usize::from(first));
    let mut widths = Vec::with_capacity(count);
    let mut invalid = items.len() > count;
    for item in &items[..count] {
        let width = resolved(resolver, item)
            .as_number()
            .filter(|width| width.is_finite());
        invalid |= width.is_none();
        widths.push(width.unwrap_or(0.0));
    }
    if invalid {
        diagnostics.push(DiagnosticKind::InvalidWidths, offset);
    }
    (first, widths)
}

// A Type 3 font's `/FontMatrix`: six finite numbers, the first not zero, since
// it scales the widths.
fn read_font_matrix(
    resolver: &mut Resolver<'_>,
    font: &Dictionary,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> [f64; 6] {
    let matrix = font.get("FontMatrix").and_then(|entry| {
        let array = resolved(resolver, entry);
        let items = array.as_array()?;
        let numbers: Vec<f64> = items
            .iter()
            .map(|item| {
                resolved(resolver, item)
                    .as_number()
                    .filter(|n| n.is_finite())
            })
            .collect::<Option<_>>()?;
        let matrix: [f64; 6] = numbers.try_into().ok()?;
        (matrix[0] != 0.0).then_some(matrix)
    });
    matrix.unwrap_or_else(|| {
        diagnostics.push(DiagnosticKind::InvalidFontMatrix, offset);
        DEFAULT_MATRIX
    })
}
