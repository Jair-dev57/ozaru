use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::path::Path;
use crate::content::Matrix;
use crate::fonts::outline::{Cff, TrueType, Type1};
use crate::fonts::{
    BaseEncoding, Code, Font, GlyphList, SimpleFontKind, StandardFont, glyph_to_unicode,
};

/// The outline of a glyph and the matrix that takes it to text space.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphShape {
    /// In the units of the font program, y up.
    pub path: Path,
    /// From those units to text space: a thousandth for most fonts.
    pub matrix: Matrix,
}

const SUBSTITUTES: [(&str, &[u8]); 13] = [
    (
        "sans",
        include_bytes!("../fonts/data/standard/OzaruStdSans-Regular.ttf"),
    ),
    (
        "sans-bold",
        include_bytes!("../fonts/data/standard/OzaruStdSans-Bold.ttf"),
    ),
    (
        "sans-italic",
        include_bytes!("../fonts/data/standard/OzaruStdSans-Italic.ttf"),
    ),
    (
        "sans-bold-italic",
        include_bytes!("../fonts/data/standard/OzaruStdSans-BoldItalic.ttf"),
    ),
    (
        "serif",
        include_bytes!("../fonts/data/standard/OzaruStdSerif-Regular.ttf"),
    ),
    (
        "serif-bold",
        include_bytes!("../fonts/data/standard/OzaruStdSerif-Bold.ttf"),
    ),
    (
        "serif-italic",
        include_bytes!("../fonts/data/standard/OzaruStdSerif-Italic.ttf"),
    ),
    (
        "serif-bold-italic",
        include_bytes!("../fonts/data/standard/OzaruStdSerif-BoldItalic.ttf"),
    ),
    (
        "mono",
        include_bytes!("../fonts/data/standard/OzaruStdMono-Regular.ttf"),
    ),
    (
        "mono-bold",
        include_bytes!("../fonts/data/standard/OzaruStdMono-Bold.ttf"),
    ),
    (
        "mono-italic",
        include_bytes!("../fonts/data/standard/OzaruStdMono-Italic.ttf"),
    ),
    (
        "mono-bold-italic",
        include_bytes!("../fonts/data/standard/OzaruStdMono-BoldItalic.ttf"),
    ),
    (
        "symbols",
        include_bytes!("../fonts/data/standard/DejaVuSans-Symbols.ttf"),
    ),
];

fn substitutes() -> &'static [Option<TrueType>] {
    static FONTS: OnceLock<Vec<Option<TrueType>>> = OnceLock::new();
    FONTS.get_or_init(|| {
        SUBSTITUTES
            .iter()
            .map(|(_, data)| TrueType::parse(data))
            .collect()
    })
}

/// Which of the bundled fonts draws a font that isn't embedded: the standard
/// 14 and the fonts metrically the same are themselves, and other fonts take
/// the family their flags and name suggest.
fn substitute_index(font: &Font) -> usize {
    let name = font
        .base_font()
        .map(|n| String::from_utf8_lossy(n.as_bytes()).to_lowercase())
        .unwrap_or_default();
    let flags = match font {
        Font::Simple(simple) => simple.flags(),
        Font::Composite(_) => 0,
    };
    if let Font::Simple(simple) = font
        && let Some(standard) = simple.standard_font()
        && matches!(standard, StandardFont::Symbol | StandardFont::ZapfDingbats)
    {
        return 12;
    }
    let bold = name.contains("bold")
        || name.contains("black")
        || name.contains("heavy")
        || flags & (1 << 18) != 0;
    let italic = name.contains("italic") || name.contains("oblique") || flags & 64 != 0;
    let family = if name.contains("courier")
        || name.contains("mono")
        || name.contains("consolas")
        || flags & 1 != 0
    {
        2
    } else if name.contains("times")
        || name.contains("serif") && !name.contains("sans")
        || name.contains("roman")
        || name.contains("georgia")
        || name.contains("garamond")
        || name.contains("palatino")
        || name.contains("book")
        || (flags & 2 != 0 && !name.contains("sans"))
    {
        1
    } else {
        0
    };
    family * 4 + usize::from(bold) + 2 * usize::from(italic)
}

/// A font program ready to give outlines.
#[allow(clippy::large_enum_variant)]
enum Program {
    TrueType(TrueType),
    Cff { cff: Cff, sfnt: Option<TrueType> },
    Type1(Type1),
    Substitute(usize),
}

/// What draws the glyphs of a font, kept for the fonts of a page.
pub struct GlyphCache {
    programs: HashMap<usize, Option<Arc<Program>>>,
    shapes: HashMap<(usize, u32, u32), Option<Arc<GlyphShape>>>,
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            programs: HashMap::new(),
            shapes: HashMap::new(),
        }
    }

    /// The outline of the glyph of `code` in `font`. `None` if the font has
    /// no program to draw it with, or it has no glyph for the code.
    pub fn shape(&mut self, font: &Arc<Font>, code: Code) -> Option<Arc<GlyphShape>> {
        let key = Arc::as_ptr(font) as usize;
        if let Some(shape) = self.shapes.get(&(key, code.value, u32::from(code.length))) {
            return shape.clone();
        }
        let program = self
            .programs
            .entry(key)
            .or_insert_with(|| load(font).map(Arc::new))
            .clone();
        let shape = program
            .and_then(|program| shape_of(&program, font, code))
            .map(Arc::new);
        self.shapes
            .insert((key, code.value, u32::from(code.length)), shape.clone());
        shape
    }
}

fn load(font: &Font) -> Option<Program> {
    if font.is_type3() {
        return None;
    }
    match font.render_program() {
        Some(embedded) => {
            let data = &embedded.data;
            if data.starts_with(&[0, 1, 0, 0])
                || data.starts_with(b"true")
                || data.starts_with(b"OTTO")
                || data.starts_with(b"ttcf")
            {
                let sfnt = TrueType::parse(data)?;
                if let Some(table) = sfnt.cff_table() {
                    let cff = Cff::parse(table)?;
                    return Some(Program::Cff {
                        cff,
                        sfnt: Some(sfnt),
                    });
                }
                Some(Program::TrueType(sfnt))
            } else if data.first() == Some(&1) && data.get(1) == Some(&0) {
                Some(Program::Cff {
                    cff: Cff::parse(data)?,
                    sfnt: None,
                })
            } else {
                Type1::parse(data).map(Program::Type1)
            }
        }
        None => match font {
            Font::Simple(_) => Some(Program::Substitute(substitute_index(font))),
            // Composite fonts without a program have no known glyphs.
            Font::Composite(_) => None,
        },
    }
}

fn standard_name(code: u8) -> Option<&'static str> {
    BaseEncoding::Standard.glyph_name(code)
}

fn shape_of(program: &Program, font: &Font, code: Code) -> Option<GlyphShape> {
    let thousandth = Matrix::new(0.001, 0.0, 0.0, 0.001, 0.0, 0.0);
    match program {
        Program::Substitute(index) => {
            let tt = substitutes().get(*index)?.as_ref()?;
            let Font::Simple(simple) = font else {
                return None;
            };
            let byte = u8::try_from(code.value).ok()?;
            // Symbol and ZapfDingbats keep their own encoding, whatever
            // `/Encoding` says, as viewers do: widgets often declare WinAnsi.
            let own = simple
                .standard_font()
                .filter(|font| matches!(font, StandardFont::Symbol | StandardFont::ZapfDingbats))
                .and_then(|font| font.built_in_encoding().glyph_name(byte));
            let name = match own {
                Some(name) => name.as_bytes(),
                None => simple.glyph_name(byte)?,
            };
            let list = if *index == 12 && simple.standard_font() == Some(StandardFont::ZapfDingbats)
            {
                GlyphList::ZapfDingbats
            } else {
                GlyphList::Adobe
            };
            let character = glyph_to_unicode(name, list)?.chars().next()?;
            // Characters the Adobe list and the fonts give to different
            // code points: Omega, Delta and micro.
            let alternate = match character as u32 {
                0x2126 => 0x03A9,
                0x03A9 => 0x2126,
                0x2206 => 0x0394,
                0x0394 => 0x2206,
                0x00B5 => 0x03BC,
                0x03BC => 0x00B5,
                other => other,
            };
            let gid = tt
                .maps
                .iter()
                .filter(|m| m.platform == 3 || m.platform == 0)
                .find_map(|m| m.glyph(character as u32).or_else(|| m.glyph(alternate)))?;
            let scale = 1.0 / f64::from(tt.units_per_em);
            Some(GlyphShape {
                path: tt.outline(gid)?,
                matrix: Matrix::new(scale, 0.0, 0.0, scale, 0.0, 0.0),
            })
        }
        Program::TrueType(tt) => {
            let gid = match font {
                Font::Simple(simple) => {
                    simple_truetype_glyph(tt, simple, u8::try_from(code.value).ok()?)?
                }
                Font::Composite(composite) => {
                    let cid = composite.cid(code)?;
                    match font.render_program().and_then(|p| p.cid_to_gid.as_ref()) {
                        Some(map) => *map.get(cid as usize)?,
                        None => u16::try_from(cid).ok()?,
                    }
                }
            };
            let scale = 1.0 / f64::from(tt.units_per_em);
            Some(GlyphShape {
                path: tt.outline(gid)?,
                matrix: Matrix::new(scale, 0.0, 0.0, scale, 0.0, 0.0),
            })
        }
        Program::Cff { cff, sfnt } => {
            let gid = match font {
                Font::Simple(simple) => {
                    let byte = u8::try_from(code.value).ok()?;
                    simple
                        .glyph_name(byte)
                        .and_then(|name| cff.glyph_by_name(name))
                        .or_else(|| cff.builtin_glyph(byte))
                        .or_else(|| {
                            let sfnt = sfnt.as_ref()?;
                            sfnt.maps.iter().find_map(|m| m.glyph(u32::from(byte)))
                        })?
                }
                Font::Composite(composite) => {
                    let cid = u16::try_from(composite.cid(code)?).ok()?;
                    if cff.is_cid {
                        cff.glyph_by_cid(cid)?
                    } else {
                        cid
                    }
                }
            };
            let standard = |c: u8| standard_name(c).and_then(|n| cff.glyph_by_name(n.as_bytes()));
            let m = cff.font_matrix;
            Some(GlyphShape {
                path: cff.outline(gid, &standard)?,
                matrix: Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5]),
            })
        }
        Program::Type1(type1) => {
            let Font::Simple(simple) = font else {
                return None;
            };
            let byte = u8::try_from(code.value).ok()?;
            let name = simple.glyph_name(byte)?;
            let path = type1.outline(name, &standard_name)?;
            let m = type1.font_matrix;
            let _ = thousandth;
            Some(GlyphShape {
                path,
                matrix: Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5]),
            })
        }
    }
}

// The glyph of a code in a simple TrueType font (ISO 32000-2, section
// 9.6.5.4): by the name the encoding gives it through the Unicode map; or
// for symbolic fonts, through the font's own maps with the code as it is, in
// the private use area, or in the Mac map.
fn simple_truetype_glyph(
    tt: &TrueType,
    simple: &crate::fonts::SimpleFont,
    code: u8,
) -> Option<u16> {
    let unicode_map = tt
        .map(3, 1)
        .or_else(|| tt.map(0, 3))
        .or_else(|| tt.map(0, 4));
    let symbol_map = tt.map(3, 0);
    let mac_map = tt.map(1, 0);
    let named = simple
        .glyph_name(code)
        .filter(|_| simple.kind() == SimpleFontKind::TrueType || !simple.is_symbolic() || true);
    if let Some(name) = named
        && !(simple.is_symbolic() && symbol_map.is_some())
    {
        if let Some(character) =
            glyph_to_unicode(name, GlyphList::Adobe).and_then(|t| t.chars().next())
            && let Some(gid) = unicode_map.and_then(|m| m.glyph(character as u32))
        {
            return Some(gid);
        }
        if let Some(gid) = tt.glyph_by_name(name) {
            return Some(gid);
        }
    }
    let code32 = u32::from(code);
    if let Some(map) = symbol_map {
        for base in [0, 0xF000, 0xF100, 0xF200] {
            if let Some(gid) = map.glyph(base + code32) {
                return Some(gid);
            }
        }
    }
    if let Some(gid) = mac_map.and_then(|m| m.glyph(code32)) {
        return Some(gid);
    }
    // A Unicode map keyed by the code itself, as some subsetters write.
    if let Some(gid) = unicode_map.and_then(|m| m.glyph(code32)) {
        return Some(gid);
    }
    if let Some(name) = simple.glyph_name(code)
        && let Some(character) =
            glyph_to_unicode(name, GlyphList::Adobe).and_then(|t| t.chars().next())
        && let Some(gid) = unicode_map.and_then(|m| m.glyph(character as u32))
    {
        return Some(gid);
    }
    None
}
