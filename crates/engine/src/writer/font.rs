//! Fonts of your own: a TrueType file read for drawing text, and the subset of
//! it a page needs, as the objects of an embedded font (ISO 32000-2, section
//! 9.9 and 9.7.4).
//!
//! The font is written as a composite font with `Identity-H`: each character is
//! two bytes, the number of its glyph in the font. The subset keeps those
//! numbers and drops the outlines of the glyphs nobody drew, so the file stays
//! small and the numbers still mean what they meant.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::filters::deflate_zlib;
use crate::fonts::outline::{TrueType, table_directory};
use crate::syntax::{Dictionary, Name, Object, PdfString, StringFormat};

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn i16_at(data: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The most glyphs a font may have for the subset to number them.
const MAX_GLYPHS: usize = 65_535;

// The tables of a font a subset copies as they are, for readers that hint.
const COPIED: [&[u8; 4]; 3] = [b"cvt ", b"fpgm", b"prep"];

/// A TrueType font file read for drawing text with it.
///
/// Only fonts with `glyf` outlines can be embedded this way (OpenType fonts
/// with PostScript outlines can't), and only if their license lets them be
/// embedded and subset.
#[derive(Clone)]
pub struct EmbeddedFont {
    data: Vec<u8>,
    tables: HashMap<[u8; 4], (usize, usize)>,
    characters: HashMap<u32, u16>,
    advances: Vec<u16>,
    left_bearings: Vec<i16>,
    glyph_count: usize,
    units_per_em: u16,
    name: String,
    ascent: i16,
    descent: i16,
    cap_height: i16,
    bounding_box: [i16; 4],
    italic_angle: f64,
    bold: bool,
    fixed: bool,
    // A fingerprint of the file, for names that depend on it.
    fingerprint: u64,
}

impl std::fmt::Debug for EmbeddedFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedFont")
            .field("name", &self.name)
            .field("glyphs", &self.glyph_count)
            .finish()
    }
}

impl PartialEq for EmbeddedFont {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint && self.data.len() == other.data.len()
    }
}

// FNV-1a: a name that is the same for the same bytes, nothing more.
fn fingerprint(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl EmbeddedFont {
    /// Reads a font from the bytes of a `.ttf` file (or the first font of a
    /// `.ttc`).
    ///
    /// It is an `UnsupportedFont` error if it isn't a TrueType font with
    /// outlines of its own kind, and a `FontNotEmbeddable` one if its license
    /// forbids embedding or subsetting it.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let bad = || Error::new(ErrorKind::UnsupportedFont, 0);
        let tables = table_directory(data).ok_or_else(bad)?;
        for tag in [b"glyf", b"loca", b"head", b"hhea", b"hmtx", b"maxp"] {
            if !tables.contains_key(tag) {
                return Err(bad());
            }
        }
        let table = |tag: &[u8; 4]| -> Option<&[u8]> {
            let &(offset, length) = tables.get(tag)?;
            data.get(offset..offset + length)
        };
        let head = table(b"head").ok_or_else(bad)?;
        let hhea = table(b"hhea").ok_or_else(bad)?;
        let maxp = table(b"maxp").ok_or_else(bad)?;
        let hmtx = table(b"hmtx").ok_or_else(bad)?;

        let units_per_em = u16_at(head, 18).ok_or_else(bad)?;
        if units_per_em == 0 {
            return Err(bad());
        }
        let glyph_count = usize::from(u16_at(maxp, 4).ok_or_else(bad)?);
        if glyph_count == 0 || glyph_count > MAX_GLYPHS {
            return Err(bad());
        }
        let bounding_box = [
            i16_at(head, 36).ok_or_else(bad)?,
            i16_at(head, 38).ok_or_else(bad)?,
            i16_at(head, 40).ok_or_else(bad)?,
            i16_at(head, 42).ok_or_else(bad)?,
        ];

        // The license: restricted fonts, and those that may not be subset.
        if let Some(os2) = table(b"OS/2")
            && let Some(kind) = u16_at(os2, 8)
            && (kind & 0x0002 != 0 || kind & 0x0100 != 0)
        {
            return Err(Error::new(ErrorKind::FontNotEmbeddable, 0));
        }

        let ascent = i16_at(hhea, 4).ok_or_else(bad)?;
        let descent = i16_at(hhea, 6).ok_or_else(bad)?;
        let metric_count = usize::from(u16_at(hhea, 34).ok_or_else(bad)?).min(glyph_count);
        if metric_count == 0 {
            return Err(bad());
        }
        let mut advances = Vec::with_capacity(glyph_count);
        let mut left_bearings = Vec::with_capacity(glyph_count);
        for gid in 0..glyph_count {
            if gid < metric_count {
                advances.push(u16_at(hmtx, 4 * gid).ok_or_else(bad)?);
                left_bearings.push(i16_at(hmtx, 4 * gid + 2).ok_or_else(bad)?);
            } else {
                advances.push(*advances.last().ok_or_else(bad)?);
                let at = 4 * metric_count + 2 * (gid - metric_count);
                left_bearings.push(i16_at(hmtx, at).unwrap_or(0));
            }
        }

        let font = TrueType::parse(data).ok_or_else(bad)?;
        if font.cff.is_some() {
            return Err(bad());
        }
        // A Unicode map; the full repertoire one over the BMP one.
        let map = font
            .maps
            .iter()
            .filter(|map| map.is_unicode())
            .max_by_key(|map| (map.platform == 3 && map.encoding == 10, map.codes().len()))
            .ok_or_else(bad)?;
        let characters: HashMap<u32, u16> = map
            .codes()
            .into_iter()
            .filter_map(|code| map.glyph(code).map(|gid| (code, gid)))
            .collect();

        let os2 = table(b"OS/2");
        let cap_height = os2
            .filter(|os2| u16_at(os2, 0).is_some_and(|version| version >= 2))
            .and_then(|os2| i16_at(os2, 88))
            .filter(|&height| height > 0)
            .unwrap_or(ascent);
        let weight = os2.and_then(|os2| u16_at(os2, 4)).unwrap_or(400);
        let post = table(b"post");
        // The angle is a 16.16 fixed number.
        let italic_angle = post
            .and_then(|post| u32_at(post, 4))
            .map_or(0.0, |angle| f64::from(angle as i32) / 65536.0);
        let fixed = post
            .and_then(|post| u32_at(post, 12))
            .is_some_and(|pitch| pitch != 0);

        Ok(Self {
            name: postscript_name(table(b"name")),
            data: data.to_vec(),
            characters,
            advances,
            left_bearings,
            glyph_count,
            units_per_em,
            ascent,
            descent,
            cap_height,
            bounding_box,
            italic_angle,
            bold: weight >= 600,
            fixed,
            fingerprint: fingerprint(data),
            tables,
        })
    }

    /// The PostScript name of the font, as a name can be written.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The glyph that draws `character`, if the font has one.
    pub fn glyph(&self, character: char) -> Option<u16> {
        self.characters.get(&(character as u32)).copied()
    }

    /// How far the pen moves after glyph `gid`, in thousandths of the text size.
    pub fn advance(&self, gid: u16) -> f64 {
        self.scaled(i32::from(
            self.advances.get(usize::from(gid)).copied().unwrap_or(0),
        ))
    }

    /// The height of the letters above the baseline and the depth below it
    /// (negative), in thousandths of the text size.
    pub fn ascender_descender(&self) -> (f64, f64) {
        (
            self.scaled(i32::from(self.ascent)),
            self.scaled(i32::from(self.descent)),
        )
    }

    fn scaled(&self, units: i32) -> f64 {
        f64::from(units) * 1000.0 / f64::from(self.units_per_em)
    }

    /// The glyphs of `text`, or the first character the font doesn't have as
    /// an `UnsupportedCharacter` error.
    pub fn glyphs(&self, text: &str) -> Result<Vec<(u16, char)>> {
        text.chars()
            .map(|c| {
                self.glyph(c)
                    .map(|gid| (gid, c))
                    .ok_or_else(|| Error::new(ErrorKind::UnsupportedCharacter, 0))
            })
            .collect()
    }

    /// The width of `text` at `size` points.
    pub fn text_width(&self, size: f64, text: &str) -> Result<f64> {
        let total: f64 = self
            .glyphs(text)?
            .iter()
            .map(|&(gid, _)| self.advance(gid))
            .sum();
        Ok(total * size / 1000.0)
    }

    /// The name a font has in the resources of a page when only `glyphs` are
    /// drawn with it: it depends on both, so drawing twice shares it only when
    /// it is the same font and the same glyphs.
    pub fn resource_name(&self, glyphs: &BTreeMap<u16, char>) -> String {
        format!("OzE{}", self.tag(glyphs))
    }

    // Six capital letters that depend on the font and the glyphs of a subset
    // (the prefix a subset's name has, ISO 32000-2, 9.9.2).
    fn tag(&self, glyphs: &BTreeMap<u16, char>) -> String {
        let mut hash = self.fingerprint;
        for &gid in glyphs.keys() {
            hash ^= u64::from(gid);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        (0..6)
            .map(|i| char::from(b'A' + ((hash >> (i * 5)) % 26) as u8))
            .collect()
    }

    /// The subset of the font with `glyphs` (each with a character that draws
    /// it, for text extraction) as a font file and the dictionaries of a
    /// composite font, ready to add to a document.
    ///
    /// The objects it needs are added to `editor`.
    pub fn embed(
        &self,
        glyphs: &BTreeMap<u16, char>,
        editor: &mut Editor<'_, '_>,
    ) -> Result<Dictionary> {
        let file = self.subset(glyphs.keys().copied().collect())?;
        let base = format!("{}+{}", self.tag(glyphs), self.name);

        let mut stream = Dictionary::new();
        stream.insert(Name::from("Length1"), Object::Integer(file.len() as i64));
        stream.insert(
            Name::from("Filter"),
            Object::Name(Name::from("FlateDecode")),
        );
        let file = Object::Reference(editor.add_stream(stream, deflate_zlib(&file, 6)));

        let mut descriptor = Dictionary::new();
        descriptor.insert(
            Name::from("Type"),
            Object::Name(Name::from("FontDescriptor")),
        );
        descriptor.insert(
            Name::from("FontName"),
            Object::Name(Name::from(base.as_str())),
        );
        let mut flags = 4;
        if self.fixed {
            flags |= 1;
        }
        if self.italic_angle != 0.0 {
            flags |= 64;
        }
        descriptor.insert(Name::from("Flags"), Object::Integer(flags));
        let [x0, y0, x1, y1] = self.bounding_box.map(|v| self.scaled(i32::from(v)).round());
        descriptor.insert(
            Name::from("FontBBox"),
            Object::Array(vec![
                Object::Real(x0),
                Object::Real(y0),
                Object::Real(x1),
                Object::Real(y1),
            ]),
        );
        descriptor.insert(Name::from("ItalicAngle"), Object::Real(self.italic_angle));
        let (ascent, descent) = self.ascender_descender();
        descriptor.insert(Name::from("Ascent"), Object::Real(ascent.round()));
        descriptor.insert(Name::from("Descent"), Object::Real(descent.round()));
        descriptor.insert(
            Name::from("CapHeight"),
            Object::Real(self.scaled(i32::from(self.cap_height)).round()),
        );
        descriptor.insert(
            Name::from("StemV"),
            Object::Integer(if self.bold { 140 } else { 80 }),
        );
        descriptor.insert(Name::from("FontFile2"), file);
        let descriptor = Object::Reference(editor.add(Object::Dictionary(descriptor)));

        // The widths of the glyphs drawn, in thousandths of the text size.
        let mut widths = Vec::new();
        for &gid in glyphs.keys() {
            widths.push(Object::Integer(i64::from(gid)));
            widths.push(Object::Array(vec![Object::Integer(
                self.advance(gid).round() as i64,
            )]));
        }

        let mut system = Dictionary::new();
        system.insert(
            Name::from("Registry"),
            Object::String(PdfString::new(b"Adobe".to_vec(), StringFormat::Literal)),
        );
        system.insert(
            Name::from("Ordering"),
            Object::String(PdfString::new(b"Identity".to_vec(), StringFormat::Literal)),
        );
        system.insert(Name::from("Supplement"), Object::Integer(0));

        let mut descendant = Dictionary::new();
        descendant.insert(Name::from("Type"), Object::Name(Name::from("Font")));
        descendant.insert(
            Name::from("Subtype"),
            Object::Name(Name::from("CIDFontType2")),
        );
        descendant.insert(
            Name::from("BaseFont"),
            Object::Name(Name::from(base.as_str())),
        );
        descendant.insert(Name::from("CIDSystemInfo"), Object::Dictionary(system));
        descendant.insert(Name::from("FontDescriptor"), descriptor);
        descendant.insert(Name::from("DW"), Object::Integer(0));
        descendant.insert(Name::from("W"), Object::Array(widths));
        descendant.insert(
            Name::from("CIDToGIDMap"),
            Object::Name(Name::from("Identity")),
        );
        let descendant = Object::Reference(editor.add(Object::Dictionary(descendant)));

        let mut unicode = Dictionary::new();
        unicode.insert(
            Name::from("Filter"),
            Object::Name(Name::from("FlateDecode")),
        );
        let unicode =
            Object::Reference(editor.add_stream(unicode, deflate_zlib(&to_unicode(glyphs), 6)));

        let mut font = Dictionary::new();
        font.insert(Name::from("Type"), Object::Name(Name::from("Font")));
        font.insert(Name::from("Subtype"), Object::Name(Name::from("Type0")));
        font.insert(
            Name::from("BaseFont"),
            Object::Name(Name::from(base.as_str())),
        );
        font.insert(
            Name::from("Encoding"),
            Object::Name(Name::from("Identity-H")),
        );
        font.insert(
            Name::from("DescendantFonts"),
            Object::Array(vec![descendant]),
        );
        font.insert(Name::from("ToUnicode"), unicode);
        Ok(font)
    }

    // The font file with only the outlines of `wanted` and of the glyphs they
    // are made of, under the numbers they had.
    fn subset(&self, wanted: BTreeSet<u16>) -> Result<Vec<u8>> {
        let bad = || Error::new(ErrorKind::UnsupportedFont, 0);
        let table = |tag: &[u8; 4]| -> Option<&[u8]> {
            let &(offset, length) = self.tables.get(tag)?;
            self.data.get(offset..offset + length)
        };
        let head = table(b"head").ok_or_else(bad)?;
        let glyf = table(b"glyf").ok_or_else(bad)?;
        let loca = table(b"loca").ok_or_else(bad)?;
        let long_loca = i16_at(head, 50).ok_or_else(bad)? != 0;
        let range = |gid: usize| -> Option<(usize, usize)> {
            let (start, end) = if long_loca {
                (
                    u32_at(loca, 4 * gid)? as usize,
                    u32_at(loca, 4 * gid + 4)? as usize,
                )
            } else {
                (
                    usize::from(u16_at(loca, 2 * gid)?) * 2,
                    usize::from(u16_at(loca, 2 * gid + 2)?) * 2,
                )
            };
            (start < end && end <= glyf.len()).then_some((start, end))
        };

        // The glyphs to keep: those drawn, the missing glyph, and the parts of
        // the composite ones.
        let mut keep: BTreeSet<u16> = wanted;
        keep.insert(0);
        let mut pending: Vec<u16> = keep.iter().copied().collect();
        while let Some(gid) = pending.pop() {
            let Some((start, end)) = range(usize::from(gid)) else {
                continue;
            };
            for part in components(&glyf[start..end]) {
                if usize::from(part) < self.glyph_count && keep.insert(part) {
                    pending.push(part);
                }
            }
        }
        let count = usize::from(*keep.last().ok_or_else(bad)?) + 1;
        if count > self.glyph_count {
            return Err(bad());
        }

        let mut new_glyf = Vec::new();
        let mut new_loca = Vec::with_capacity(4 * (count + 1));
        for gid in 0..count {
            new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
            if keep.contains(&(gid as u16))
                && let Some((start, end)) = range(gid)
            {
                new_glyf.extend_from_slice(&glyf[start..end]);
                // Glyphs start on even offsets.
                while new_glyf.len() % 4 != 0 {
                    new_glyf.push(0);
                }
            }
        }
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());

        let mut new_head = head.to_vec();
        new_head.get_mut(8..12).ok_or_else(bad)?.fill(0);
        new_head
            .get_mut(50..52)
            .ok_or_else(bad)?
            .copy_from_slice(&1_i16.to_be_bytes());

        let mut hhea = table(b"hhea").ok_or_else(bad)?.to_vec();
        hhea.get_mut(34..36)
            .ok_or_else(bad)?
            .copy_from_slice(&(count as u16).to_be_bytes());

        let mut maxp = table(b"maxp").ok_or_else(bad)?.to_vec();
        maxp.get_mut(4..6)
            .ok_or_else(bad)?
            .copy_from_slice(&(count as u16).to_be_bytes());

        let mut hmtx = Vec::with_capacity(4 * count);
        for gid in 0..count {
            hmtx.extend_from_slice(&self.advances[gid].to_be_bytes());
            hmtx.extend_from_slice(&self.left_bearings[gid].to_be_bytes());
        }

        let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"glyf", new_glyf),
            (*b"head", new_head),
            (*b"hhea", hhea),
            (*b"hmtx", hmtx),
            (*b"loca", new_loca),
            (*b"maxp", maxp),
        ];
        for tag in COPIED {
            if let Some(bytes) = table(tag) {
                tables.push((*tag, bytes.to_vec()));
            }
        }
        tables.sort_by_key(|(tag, _)| *tag);
        Ok(assemble(&tables))
    }
}

// The glyphs a composite glyph is made of (the OpenType specification,
// `glyf`); none for a simple glyph or a damaged one.
fn components(glyph: &[u8]) -> Vec<u16> {
    let mut parts = Vec::new();
    if i16_at(glyph, 0).is_none_or(|contours| contours >= 0) {
        return parts;
    }
    let mut at = 10;
    while let (Some(flags), Some(part)) = (u16_at(glyph, at), u16_at(glyph, at + 2)) {
        parts.push(part);
        at += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
        at += if flags & 0x0008 != 0 {
            2
        } else if flags & 0x0040 != 0 {
            4
        } else if flags & 0x0080 != 0 {
            8
        } else {
            0
        };
        if flags & 0x0020 == 0 {
            break;
        }
    }
    parts
}

// A font file made of `tables`, sorted by tag, with its checksums.
fn assemble(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let count = tables.len();
    let mut search = 1_usize;
    let mut log = 0_u16;
    while search * 2 <= count {
        search *= 2;
        log += 1;
    }
    let mut file = Vec::new();
    file.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    file.extend_from_slice(&(count as u16).to_be_bytes());
    file.extend_from_slice(&((search * 16) as u16).to_be_bytes());
    file.extend_from_slice(&log.to_be_bytes());
    file.extend_from_slice(&((count * 16 - search * 16) as u16).to_be_bytes());
    let mut offset = 12 + 16 * count;
    let mut head_at = None;
    for (tag, bytes) in tables {
        if tag == b"head" {
            head_at = Some(offset);
        }
        file.extend_from_slice(tag);
        file.extend_from_slice(&checksum(bytes).to_be_bytes());
        file.extend_from_slice(&(offset as u32).to_be_bytes());
        file.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        offset += bytes.len().div_ceil(4) * 4;
    }
    for (_, bytes) in tables {
        file.extend_from_slice(bytes);
        while file.len() % 4 != 0 {
            file.push(0);
        }
    }
    // The adjustment that makes the whole file add up to a fixed number.
    if let Some(at) = head_at {
        let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(checksum(&file));
        file[at + 8..at + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    file
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut sum = 0_u32;
    for chunk in bytes.chunks(4) {
        let mut word = [0_u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    sum
}

// The `ToUnicode` CMap that says which character each glyph draws, so text
// extraction and copying give the text back.
fn to_unicode(glyphs: &BTreeMap<u16, char>) -> Vec<u8> {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &char)> = glyphs.iter().collect();
    for chunk in entries.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, character) in chunk {
            let mut units = [0_u16; 2];
            let hex: String = character
                .encode_utf16(&mut units)
                .iter()
                .map(|unit| format!("{unit:04X}"))
                .collect();
            cmap.push_str(&format!("<{gid:04X}> <{hex}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}

// The PostScript name (name 6) of a font, or its full name, as ASCII without
// spaces; `Font` if it has none.
fn postscript_name(table: Option<&[u8]>) -> String {
    let find = |wanted: u16| -> Option<String> {
        let table = table?;
        let count = usize::from(u16_at(table, 2)?);
        let strings = usize::from(u16_at(table, 4)?);
        for i in 0..count {
            let at = 6 + 12 * i;
            let (platform, id) = (u16_at(table, at)?, u16_at(table, at + 6)?);
            if id != wanted || !matches!(platform, 1 | 3) {
                continue;
            }
            let length = usize::from(u16_at(table, at + 8)?);
            let offset = strings + usize::from(u16_at(table, at + 10)?);
            let bytes = table.get(offset..offset + length)?;
            let text: String = if platform == 3 {
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| char::from_u32(u32::from(u16::from_be_bytes(*pair))))
                    .collect::<Option<String>>()?
            } else {
                bytes.iter().map(|&b| char::from(b)).collect()
            };
            return Some(text);
        }
        None
    };
    let name: String = find(6)
        .or_else(|| find(4))
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(60)
        .collect();
    if name.is_empty() {
        "Font".to_string()
    } else {
        name
    }
}
