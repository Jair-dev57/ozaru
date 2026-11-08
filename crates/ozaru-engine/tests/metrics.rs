use ozaru_engine::fonts::{BaseEncoding, StandardFont};

#[test]
fn the_14_names_are_found() {
    for name in [
        "Courier",
        "Courier-Bold",
        "Courier-Oblique",
        "Courier-BoldOblique",
        "Helvetica",
        "Helvetica-Bold",
        "Helvetica-Oblique",
        "Helvetica-BoldOblique",
        "Times-Roman",
        "Times-Bold",
        "Times-Italic",
        "Times-BoldItalic",
        "Symbol",
        "ZapfDingbats",
    ] {
        let font = StandardFont::from_base_font(name.as_bytes()).unwrap();
        assert_eq!(font.name(), name);
    }
}

#[test]
fn common_aliases_are_found() {
    for (name, font) in [
        ("Helvetica,Bold", StandardFont::HelveticaBold),
        ("Helvetica,Italic", StandardFont::HelveticaOblique),
        ("ArialMT", StandardFont::Helvetica),
        ("Arial-BoldMT", StandardFont::HelveticaBold),
        ("Arial-ItalicMT", StandardFont::HelveticaOblique),
        ("Arial,BoldItalic", StandardFont::HelveticaBoldOblique),
        ("Times", StandardFont::TimesRoman),
        ("TimesNewRomanPSMT", StandardFont::TimesRoman),
        ("TimesNewRomanPS-BoldMT", StandardFont::TimesBold),
        ("TimesNewRoman,BoldItalic", StandardFont::TimesBoldItalic),
        ("CourierNewPSMT", StandardFont::Courier),
        ("CourierNew,Italic", StandardFont::CourierOblique),
        ("Courier,Regular", StandardFont::Courier),
        ("SymbolMT", StandardFont::Symbol),
        ("Dingbats", StandardFont::ZapfDingbats),
        ("QWERTY+Times-Bold", StandardFont::TimesBold),
    ] {
        assert_eq!(
            StandardFont::from_base_font(name.as_bytes()),
            Some(font),
            "{name}"
        );
    }
}

#[test]
fn other_fonts_are_not_standard() {
    for name in [
        "",
        "DejaVuSans",
        "Calibri",
        "Helvetica-Narrow",
        "Arial-Black",
        "Symbol,Bold",
        "abcdef+Helvetica",
        "Times-Roman-Extra",
        "\u{FF}",
    ] {
        assert_eq!(
            StandardFont::from_base_font(name.as_bytes()),
            None,
            "{name}"
        );
    }
    assert_eq!(StandardFont::from_base_font(b"\xFF\xFE"), None);
}

#[test]
fn the_built_in_encodings_are_standard_but_for_the_symbols() {
    assert_eq!(
        StandardFont::Helvetica.built_in_encoding(),
        BaseEncoding::Standard
    );
    assert_eq!(
        StandardFont::Symbol.built_in_encoding(),
        BaseEncoding::Symbol
    );
    assert_eq!(
        StandardFont::ZapfDingbats.built_in_encoding(),
        BaseEncoding::ZapfDingbats
    );
}

#[test]
fn glyph_widths_come_from_adobe_metrics() {
    assert_eq!(StandardFont::Helvetica.glyph_width(b"A"), Some(667.0));
    assert_eq!(
        StandardFont::Helvetica.glyph_width(b"Idieresis"),
        Some(278.0)
    );
    assert_eq!(StandardFont::TimesRoman.glyph_width(b"space"), Some(250.0));
    assert_eq!(
        StandardFont::CourierBoldOblique.glyph_width(b"W"),
        Some(600.0)
    );
    assert_eq!(StandardFont::Symbol.glyph_width(b"alpha"), Some(631.0));
    assert_eq!(StandardFont::ZapfDingbats.glyph_width(b"a191"), Some(918.0));
    assert_eq!(StandardFont::Helvetica.glyph_width(b"alpha"), None);
    assert_eq!(StandardFont::Helvetica.glyph_width(b"\xFF"), None);
}

#[test]
fn every_font_has_its_glyphs() {
    // The number of glyphs in each AFM file.
    for (font, count) in [
        (StandardFont::Courier, 315),
        (StandardFont::Helvetica, 315),
        (StandardFont::TimesRoman, 315),
        (StandardFont::Symbol, 190),
        (StandardFont::ZapfDingbats, 202),
    ] {
        let data = include_str!("../src/fonts/data/standard14-widths.txt");
        let section = data
            .split("font ")
            .find(|section| section.lines().next() == Some(font.name()))
            .unwrap();
        let glyphs = section
            .lines()
            .skip(1)
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .inspect(|line| {
                let (name, _) = line.split_once(' ').unwrap();
                assert!(font.glyph_width(name.as_bytes()).is_some(), "{name}");
            })
            .count();
        assert_eq!(glyphs, count, "{}", font.name());
    }
}
