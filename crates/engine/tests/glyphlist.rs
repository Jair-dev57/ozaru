use ozaru_engine::fonts::{BaseEncoding, GlyphList, glyph_to_unicode};

fn adobe(name: &str) -> Option<String> {
    glyph_to_unicode(name.as_bytes(), GlyphList::Adobe)
}

#[test]
fn names_in_the_adobe_glyph_list_map_to_their_characters() {
    assert_eq!(adobe("A").as_deref(), Some("A"));
    assert_eq!(adobe("ntilde").as_deref(), Some("ñ"));
    assert_eq!(adobe("Euro").as_deref(), Some("€"));
    assert_eq!(adobe("quoteright").as_deref(), Some("’"));
    assert_eq!(adobe("fi").as_deref(), Some("ﬁ"));
    assert_eq!(adobe("alpha").as_deref(), Some("α"));
    assert_eq!(adobe("space").as_deref(), Some(" "));
}

#[test]
fn some_names_map_to_several_characters() {
    // A Hebrew letter with its point.
    assert_eq!(
        adobe("dalethatafpatah").as_deref(),
        Some("\u{05D3}\u{05B2}")
    );
}

#[test]
fn uni_names_give_their_code_points() {
    assert_eq!(adobe("uni00F1").as_deref(), Some("ñ"));
    assert_eq!(adobe("uni00660069").as_deref(), Some("fi"));
    assert_eq!(adobe("uni00f1").as_deref(), Some("ñ"));
    // Not a multiple of four digits, not hexadecimal, or a surrogate.
    assert_eq!(adobe("uni00F"), None);
    assert_eq!(adobe("uni00G1"), None);
    assert_eq!(adobe("uniD800"), None);
    assert_eq!(adobe("uni"), None);
}

#[test]
fn u_names_give_one_code_point_of_four_to_six_digits() {
    assert_eq!(adobe("u00F1").as_deref(), Some("ñ"));
    assert_eq!(adobe("u1F600").as_deref(), Some("😀"));
    assert_eq!(adobe("u10FFFF").as_deref(), Some("\u{10FFFF}"));
    assert_eq!(adobe("u110000"), None);
    assert_eq!(adobe("u0F1"), None);
    assert_eq!(adobe("u0000F1F"), None);
    assert_eq!(adobe("uDFFF"), None);
}

#[test]
fn a_suffix_after_a_period_is_dropped() {
    assert_eq!(adobe("a.sc").as_deref(), Some("a"));
    assert_eq!(adobe("uni00F1.alt.2").as_deref(), Some("ñ"));
    assert_eq!(adobe(".notdef"), None);
}

#[test]
fn ligatures_are_their_components_in_order() {
    assert_eq!(adobe("f_f_i").as_deref(), Some("ffi"));
    assert_eq!(adobe("T_h.liga").as_deref(), Some("Th"));
    // Components that give nothing are skipped.
    assert_eq!(adobe("f_g123_l").as_deref(), Some("fl"));
}

#[test]
fn names_that_map_to_nothing_give_none() {
    assert_eq!(adobe(""), None);
    assert_eq!(adobe("g123"), None);
    assert_eq!(adobe("glyph42"), None);
    assert_eq!(glyph_to_unicode(b"\xFF\xFE", GlyphList::Adobe), None);
}

#[test]
fn zapf_dingbats_names_use_their_own_list() {
    let zapf = |name: &str| glyph_to_unicode(name.as_bytes(), GlyphList::ZapfDingbats);
    assert_eq!(zapf("a1").as_deref(), Some("✁"));
    assert_eq!(zapf("a191").as_deref(), Some("➾"));
    assert_eq!(zapf("space").as_deref(), Some(" "));
    // `a1` isn't in the Adobe list. Names that the Zapf Dingbats list doesn't
    // have, like `space` and `A`, are looked up in the Adobe one.
    assert_eq!(adobe("a1"), None);
    assert_eq!(zapf("A").as_deref(), Some("A"));
    assert_eq!(zapf("uni2701").as_deref(), Some("✁"));
}

#[test]
fn every_glyph_of_the_standard_encodings_maps_to_unicode() {
    for (base, list) in [
        (BaseEncoding::Standard, GlyphList::Adobe),
        (BaseEncoding::MacRoman, GlyphList::Adobe),
        (BaseEncoding::WinAnsi, GlyphList::Adobe),
        (BaseEncoding::Symbol, GlyphList::Adobe),
        (BaseEncoding::ZapfDingbats, GlyphList::ZapfDingbats),
    ] {
        for code in 0..=255 {
            if let Some(name) = base.glyph_name(code) {
                let text = glyph_to_unicode(name.as_bytes(), list);
                assert!(text.is_some(), "{base:?} {code} {name}");
            }
        }
    }
}

#[test]
fn every_entry_of_both_lists_is_read() {
    for (file, list, count) in [
        ("src/fonts/data/glyphlist.txt", GlyphList::Adobe, 4281),
        (
            "src/fonts/data/zapfdingbats.txt",
            GlyphList::ZapfDingbats,
            201,
        ),
    ] {
        let text = std::fs::read_to_string(file).unwrap();
        let mut entries = 0;
        for line in text.lines().filter(|line| !line.starts_with('#')) {
            let (name, codes) = line.split_once(';').unwrap();
            let expected: String = codes
                .split(' ')
                .map(|code| char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap())
                .collect();
            assert_eq!(
                glyph_to_unicode(name.as_bytes(), list).as_deref(),
                Some(expected.as_str()),
                "{name}"
            );
            entries += 1;
        }
        assert_eq!(entries, count, "{file}");
    }
}
