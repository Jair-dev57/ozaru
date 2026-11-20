use std::collections::HashMap;
use std::sync::OnceLock;

use super::encoding::BaseEncoding;

/// One of the standard 14 fonts, which every PDF reader knows and files can
/// use without embedding them (ISO 32000-2, section 9.6.2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardFont {
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Symbol,
    ZapfDingbats,
}

const ALL: [StandardFont; 14] = [
    StandardFont::Courier,
    StandardFont::CourierBold,
    StandardFont::CourierOblique,
    StandardFont::CourierBoldOblique,
    StandardFont::Helvetica,
    StandardFont::HelveticaBold,
    StandardFont::HelveticaOblique,
    StandardFont::HelveticaBoldOblique,
    StandardFont::TimesRoman,
    StandardFont::TimesBold,
    StandardFont::TimesItalic,
    StandardFont::TimesBoldItalic,
    StandardFont::Symbol,
    StandardFont::ZapfDingbats,
];

impl StandardFont {
    /// The PostScript name of the font, like `Helvetica-Bold`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::TimesRoman => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        }
    }

    /// The standard font that `/BaseFont` names, if any.
    ///
    /// Besides the 14 names, the names that writers commonly use for them are
    /// accepted: with a subset prefix (`ABCDEF+Helvetica`), with the style after
    /// a comma (`Helvetica,Bold`), and the Windows fonts that are metrically
    /// the same, Arial, Times New Roman and Courier New, as in `Arial-BoldMT`
    /// or `TimesNewRoman,Italic`.
    pub fn from_base_font(name: &[u8]) -> Option<Self> {
        let name = std::str::from_utf8(without_subset_prefix(name)).ok()?;
        if let Some(font) = ALL.iter().find(|font| font.name() == name) {
            return Some(*font);
        }
        let (family, style) = match name.find([',', '-']) {
            Some(split) => (&name[..split], &name[split + 1..]),
            None => (name, ""),
        };
        let family = match family {
            "Helvetica" | "Arial" | "ArialMT" => Family::Helvetica,
            "Times" | "TimesNewRoman" | "TimesNewRomanPS" | "TimesNewRomanPSMT" => Family::Times,
            "Courier" | "CourierNew" | "CourierNewPS" | "CourierNewPSMT" => Family::Courier,
            "Symbol" | "SymbolMT" => return style.is_empty().then_some(Self::Symbol),
            "ZapfDingbats" | "Dingbats" => {
                return style.is_empty().then_some(Self::ZapfDingbats);
            }
            _ => return None,
        };
        let style = style
            .strip_suffix("PSMT")
            .or_else(|| style.strip_suffix("MT"))
            .unwrap_or(style);
        let (bold, italic) = match style {
            "" | "Roman" | "Regular" | "Normal" => (false, false),
            "Bold" => (true, false),
            "Italic" | "Oblique" => (false, true),
            "BoldItalic" | "BoldOblique" => (true, true),
            _ => return None,
        };
        Some(family.font(bold, italic))
    }

    /// The `Ascender` and `Descender` of the font's metrics, in thousandths of
    /// an em. Symbol and ZapfDingbats have none, so the top and bottom of their
    /// `FontBBox` are used.
    pub fn ascender_descender(self) -> (f64, f64) {
        match self {
            Self::Courier | Self::CourierBold | Self::CourierOblique | Self::CourierBoldOblique => {
                (629.0, -157.0)
            }
            Self::Helvetica
            | Self::HelveticaBold
            | Self::HelveticaOblique
            | Self::HelveticaBoldOblique => (718.0, -207.0),
            Self::TimesRoman | Self::TimesBold | Self::TimesItalic | Self::TimesBoldItalic => {
                (683.0, -217.0)
            }
            Self::Symbol => (1010.0, -293.0),
            Self::ZapfDingbats => (820.0, -143.0),
        }
    }

    /// The font's built-in encoding: the standard one, except for Symbol and
    /// ZapfDingbats.
    pub fn built_in_encoding(self) -> BaseEncoding {
        match self {
            Self::Symbol => BaseEncoding::Symbol,
            Self::ZapfDingbats => BaseEncoding::ZapfDingbats,
            _ => BaseEncoding::Standard,
        }
    }

    /// The width of the glyph `glyph`, in thousandths of text space, from Adobe's
    /// metrics of the font. `None` if the font has no such glyph.
    pub fn glyph_width(self, glyph: &[u8]) -> Option<f64> {
        let widths = &metrics()[self as usize];
        std::str::from_utf8(glyph)
            .ok()
            .and_then(|glyph| widths.get(glyph))
            .copied()
    }
}

#[derive(Clone, Copy)]
enum Family {
    Courier,
    Helvetica,
    Times,
}

impl Family {
    fn font(self, bold: bool, italic: bool) -> StandardFont {
        use StandardFont::*;
        match (self, bold, italic) {
            (Self::Courier, false, false) => Courier,
            (Self::Courier, true, false) => CourierBold,
            (Self::Courier, false, true) => CourierOblique,
            (Self::Courier, true, true) => CourierBoldOblique,
            (Self::Helvetica, false, false) => Helvetica,
            (Self::Helvetica, true, false) => HelveticaBold,
            (Self::Helvetica, false, true) => HelveticaOblique,
            (Self::Helvetica, true, true) => HelveticaBoldOblique,
            (Self::Times, false, false) => TimesRoman,
            (Self::Times, true, false) => TimesBold,
            (Self::Times, false, true) => TimesItalic,
            (Self::Times, true, true) => TimesBoldItalic,
        }
    }
}

/// `name` without its subset prefix, six uppercase letters and a plus sign.
pub(crate) fn without_subset_prefix(name: &[u8]) -> &[u8] {
    match name.get(6) {
        Some(b'+') if name[..6].iter().all(u8::is_ascii_uppercase) => &name[7..],
        _ => name,
    }
}

// The widths of each font, in the order of `ALL`, read once from the data file.
fn metrics() -> &'static [HashMap<&'static str, f64>; 14] {
    static METRICS: OnceLock<[HashMap<&'static str, f64>; 14]> = OnceLock::new();
    METRICS.get_or_init(|| {
        let mut metrics: [HashMap<&str, f64>; 14] = Default::default();
        let mut current = None;
        for line in include_str!("data/standard14-widths.txt").lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(font) = line.strip_prefix("font ") {
                current = ALL.iter().position(|standard| standard.name() == font);
                continue;
            }
            let (Some(index), Some((glyph, width))) = (current, line.split_once(' ')) else {
                continue;
            };
            if let Ok(width) = width.parse() {
                metrics[index].insert(glyph, width);
            }
        }
        metrics
    })
}
