//! The font programs embedded in a PDF, read only for what text extraction
//! needs when the PDF itself doesn't say: the built-in encoding of a Type 1
//! font, and which characters the glyphs of a TrueType or OpenType font are.

use std::collections::HashMap;

use super::encoding::{BaseEncoding, Encoding};
use super::glyphlist::{GlyphList, glyph_to_unicode};
use super::outline::{Cff, TrueType};
use crate::base::Limits;
use crate::syntax::{Dictionary, Lexer, Name, Object, TokenKind};
use crate::xref::Resolver;

/// The kind of a font program, by the descriptor entry that holds it
/// (ISO 32000-2, section 9.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramKind {
    /// `/FontFile`: a Type 1 font.
    Type1,
    /// `/FontFile2`, or `/FontFile3` with `/Subtype /OpenType`: a TrueType or
    /// OpenType font.
    TrueType,
    /// `/FontFile3` with another subtype: CFF, which isn't read yet.
    Other,
}

/// The decoded data of the font program in `descriptor`, and its kind.
pub(crate) fn font_program(
    resolver: &mut Resolver<'_>,
    descriptor: &Dictionary,
) -> Option<(ProgramKind, Vec<u8>)> {
    let (kind, entry) = if let Some(entry) = descriptor.get("FontFile") {
        (ProgramKind::Type1, entry)
    } else if let Some(entry) = descriptor.get("FontFile2") {
        (ProgramKind::TrueType, entry)
    } else {
        let entry = descriptor.get("FontFile3")?;
        (ProgramKind::Other, entry)
    };
    let id = entry.as_reference()?;
    let stream = resolver.resolve(id).ok()?;
    let kind = match (kind, stream.as_stream()?.dictionary().get("Subtype")) {
        (ProgramKind::Other, Some(Object::Name(name))) if name.as_bytes() == b"OpenType" => {
            ProgramKind::TrueType
        }
        (kind, _) => kind,
    };
    let data = resolver.stream_data(id).ok()?.data;
    Some((kind, data))
}

/// The built-in encoding of a Type 1 font program: `/Encoding StandardEncoding
/// def`, or the `dup code /name put` entries of its `/Encoding` array, in the
/// clear text part before `eexec` (Adobe Type 1 Font Format, section 2.3).
///
/// `None` if the program doesn't say, or says `StandardEncoding`.
pub(crate) fn type1_encoding(program: &[u8], limits: Limits) -> Option<Encoding> {
    let clear_text = match find(program, b"eexec") {
        Some(end) => &program[..end],
        None => program,
    };
    let start = find(clear_text, b"/Encoding")?;
    let mut lexer = Lexer::new(&clear_text[start + b"/Encoding".len()..], limits);
    let mut encoding = Encoding::without_base();
    // The last tokens: `dup`, a code and a name come before each `put`.
    let mut window: [Option<Token>; 3] = [None, None, None];
    let mut found = false;
    while let Ok(Some(token)) = lexer.next_token() {
        let token = match token.kind {
            TokenKind::Keyword(b"StandardEncoding") if !found => return None,
            TokenKind::Keyword(b"put") => {
                if let [
                    Some(Token::Dup),
                    Some(Token::Code(code)),
                    Some(Token::Name(name)),
                ] = &window
                {
                    encoding.set(*code, name.clone());
                    found = true;
                }
                Token::Other
            }
            // The array ends with `readonly def` or `def`.
            TokenKind::Keyword(b"def") => break,
            TokenKind::Keyword(b"dup") => Token::Dup,
            TokenKind::Integer(code) => u8::try_from(code).map_or(Token::Other, Token::Code),
            TokenKind::Name(name) => Token::Name(name),
            _ => Token::Other,
        };
        window = [window[1].take(), window[2].take(), Some(token)];
    }
    found.then_some(encoding)
}

#[derive(Debug, Clone)]
enum Token {
    Dup,
    Code(u8),
    Name(Name),
    Other,
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// What a TrueType or OpenType font program says about its glyphs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TrueTypeProgram {
    /// The text of each glyph: from the font's Unicode `cmap` subtables, or else
    /// from its glyph name in the `post` table and the Adobe Glyph List.
    glyph_text: HashMap<u16, String>,
    /// The glyph of each single-byte code of a symbolic font, through the
    /// (3, 0) or (1, 0) `cmap` subtable (ISO 32000-2, section 9.6.5.4).
    symbol_glyphs: HashMap<u8, u16>,
}

impl TrueTypeProgram {
    /// Reads `program`. `None` if it isn't a TrueType or OpenType font.
    pub(crate) fn parse(program: &[u8]) -> Option<Self> {
        let font = TrueType::parse_tables(program, false)?;
        let mut glyph_text = HashMap::new();
        for map in font.maps.iter().filter(|map| map.is_unicode()) {
            for point in map.codes() {
                let (Some(glyph), Some(c)) = (map.glyph(point), char::from_u32(point)) else {
                    continue;
                };
                // A glyph can have several code points: the first one wins.
                glyph_text.entry(glyph).or_insert_with(|| c.to_string());
            }
        }
        let symbol_glyphs = symbol_code_glyphs(&font);
        // Glyphs that no character maps to are named by the `post` table or, in
        // OpenType fonts with PostScript outlines, the `CFF ` table.
        let cff = font.cff_table().and_then(Cff::parse);
        let count = u16::try_from(font.glyph_count()).unwrap_or(u16::MAX);
        for glyph in 1..count {
            if glyph_text.contains_key(&glyph) {
                continue;
            }
            let name = font
                .glyph_name(glyph)
                .or_else(|| cff.as_ref().and_then(|cff| cff.glyph_name(glyph)));
            if let Some(text) = name.and_then(|name| glyph_to_unicode(name, GlyphList::Adobe)) {
                glyph_text.insert(glyph, text);
            }
        }
        Some(Self {
            glyph_text,
            symbol_glyphs,
        })
    }

    /// The text of glyph `glyph`, if the program says.
    pub(crate) fn glyph_text(&self, glyph: u16) -> Option<&str> {
        self.glyph_text.get(&glyph).map(String::as_str)
    }

    /// The text of the single-byte `code` of a symbolic TrueType font without
    /// `/Encoding`: the text of its glyph.
    pub(crate) fn symbol_text(&self, code: u8) -> Option<&str> {
        self.glyph_text(*self.symbol_glyphs.get(&code)?)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.glyph_text.is_empty()
    }
}

// In a symbolic font, a code c is looked up in the (3, 0) subtable as c, or as
// 0xF000 + c, 0xF100 + c or 0xF200 + c, where symbol fonts put their glyphs, or
// else in the (1, 0) subtable as c. Fonts that only have a Unicode subtable are
// looked up with c as a code point.
fn symbol_code_glyphs(font: &TrueType) -> HashMap<u8, u16> {
    let unicode = font.maps.iter().find(|map| map.is_unicode());
    let mut glyphs = HashMap::new();
    for code in 0..=255u8 {
        let c = u32::from(code);
        let glyph = if let Some(symbol) = font.map(3, 0) {
            [c, 0xF000 + c, 0xF100 + c, 0xF200 + c]
                .into_iter()
                .find_map(|point| symbol.glyph(point))
        } else if let Some(mac) = font.map(1, 0) {
            mac.glyph(c)
        } else {
            unicode.and_then(|map| map.glyph(c))
        };
        if let Some(glyph) = glyph {
            glyphs.insert(code, glyph);
        }
    }
    glyphs
}

/// A CIDFont's `/CIDToGIDMap`: `/Identity`, which is the default, or a stream
/// of two-byte glyph numbers, one per CID (ISO 32000-2, table 115). `None` for
/// the identity.
pub(crate) fn cid_to_gid_map(
    resolver: &mut Resolver<'_>,
    descendant: &Dictionary,
) -> Option<Vec<u16>> {
    let id = descendant.get("CIDToGIDMap")?.as_reference()?;
    let data = resolver.stream_data(id).ok()?.data;
    let (pairs, _) = data.as_chunks::<2>();
    Some(pairs.iter().map(|&pair| u16::from_be_bytes(pair)).collect())
}

impl Encoding {
    /// An encoding with no base: only the codes it is given have glyph names,
    /// as in the built-in encoding of a font program.
    pub(crate) fn without_base() -> Self {
        let mut encoding = Self::new(BaseEncoding::Standard);
        encoding.base_used = false;
        encoding
    }
}
