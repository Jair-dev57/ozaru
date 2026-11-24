#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::fonts::outline::{Cff, TrueType, Type1};

// Reads the input as a font program of each kind and draws the glyphs it
// has, a few of them. It never panics and the work is bounded.
fuzz_target!(|data: &[u8]| {
    if let Some(font) = TrueType::parse(data) {
        for gid in 0..font.glyph_count().min(16) {
            let _ = font.outline(gid as u16);
        }
        for map in &font.maps {
            for code in map.codes().into_iter().take(8) {
                let _ = map.glyph(code);
            }
        }
        for gid in 0..font.glyph_count().min(16) {
            let _ = font.glyph_name(gid as u16);
        }
        if let Some(table) = font.cff_table()
            && let Some(cff) = Cff::parse(table)
        {
            for gid in 0..cff.glyph_count().min(16) {
                let _ = cff.outline(gid as u16, &|_| None);
            }
        }
    }
    // The tables of a font, with or without outlines, as text extraction reads them.
    if let Some(font) = TrueType::parse_tables(data, false) {
        for map in &font.maps {
            let _ = map.is_unicode();
            for code in map.codes().into_iter().take(8) {
                let _ = map.glyph(code);
            }
        }
        for gid in 0..font.glyph_count().min(64) {
            let _ = font.glyph_name(gid as u16);
        }
    }
    if let Some(cff) = Cff::parse(data) {
        for gid in 0..cff.glyph_count().min(32) {
            let _ = cff.outline(gid as u16, &|code| cff.glyph_by_name(&[code]));
            let _ = cff.glyph_name(gid as u16);
        }
    }
    if let Some(font) = Type1::parse(data) {
        let names: Vec<Vec<u8>> = font.glyph_names().take(32).map(<[u8]>::to_vec).collect();
        for name in names {
            let _ = font.outline(&name, &|_| Some("A"));
        }
    }
});
