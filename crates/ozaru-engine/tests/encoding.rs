use ozaru_engine::base::{DiagnosticKind, Diagnostics, Limits};
use ozaru_engine::fonts::{BaseEncoding, Encoding, read_encoding};
use ozaru_engine::syntax::{Name, parse_object};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

// The base encodings

#[test]
fn the_standard_encoding_has_the_adobe_latin_glyphs() {
    let standard = BaseEncoding::Standard;
    assert_eq!(standard.glyph_name(b'A'), Some("A"));
    assert_eq!(standard.glyph_name(b'\''), Some("quoteright"));
    assert_eq!(standard.glyph_name(b'`'), Some("quoteleft"));
    assert_eq!(standard.glyph_name(0xE1), Some("AE"));
    assert_eq!(standard.glyph_name(0xAE), Some("fi"));
    assert_eq!(standard.glyph_name(0xF5), Some("dotlessi"));
    assert_eq!(standard.glyph_name(0xE9), Some("Oslash"));
    // It has no precomposed accented letters, and leaves 0xE0 undefined.
    assert_eq!(standard.glyph_name(0xE0), None);
}

#[test]
fn win_ansi_is_code_page_1252() {
    let win_ansi = BaseEncoding::WinAnsi;
    assert_eq!(win_ansi.glyph_name(b'\''), Some("quotesingle"));
    assert_eq!(win_ansi.glyph_name(0x80), Some("Euro"));
    assert_eq!(win_ansi.glyph_name(0x93), Some("quotedblleft"));
    assert_eq!(win_ansi.glyph_name(0xE9), Some("eacute"));
    assert_eq!(win_ansi.glyph_name(0xF1), Some("ntilde"));
    // Code page 1252 leaves 0x81 undefined.
    assert_eq!(win_ansi.glyph_name(0x81), None);
}

#[test]
fn win_ansi_repeats_the_space_and_the_hyphen() {
    let win_ansi = BaseEncoding::WinAnsi;
    assert_eq!(win_ansi.glyph_name(0xA0), Some("space"));
    assert_eq!(win_ansi.glyph_name(0xAD), Some("hyphen"));
}

#[test]
fn mac_roman_is_the_mac_os_latin_encoding() {
    let mac_roman = BaseEncoding::MacRoman;
    assert_eq!(mac_roman.glyph_name(0x80), Some("Adieresis"));
    assert_eq!(mac_roman.glyph_name(0x8E), Some("eacute"));
    assert_eq!(mac_roman.glyph_name(0x96), Some("ntilde"));
    assert_eq!(mac_roman.glyph_name(0xDB), Some("currency"));
    assert_eq!(mac_roman.glyph_name(0xCA), Some("space"));
    // The mathematical symbols of Mac OS Roman are in the Symbol font instead.
    assert_eq!(mac_roman.glyph_name(0xAD), None);
}

#[test]
fn symbol_and_zapf_dingbats_have_their_own_glyphs() {
    assert_eq!(BaseEncoding::Symbol.glyph_name(b'a'), Some("alpha"));
    assert_eq!(BaseEncoding::Symbol.glyph_name(0xA0), Some("Euro"));
    assert_eq!(BaseEncoding::Symbol.glyph_name(0xE5), Some("summation"));
    assert_eq!(BaseEncoding::ZapfDingbats.glyph_name(b'!'), Some("a1"));
    assert_eq!(BaseEncoding::ZapfDingbats.glyph_name(0xFE), Some("a191"));
    assert_eq!(BaseEncoding::ZapfDingbats.glyph_name(0xF0), None);
}

#[test]
fn no_encoding_defines_the_control_codes() {
    for base in [
        BaseEncoding::Standard,
        BaseEncoding::MacRoman,
        BaseEncoding::WinAnsi,
        BaseEncoding::Symbol,
        BaseEncoding::ZapfDingbats,
    ] {
        for code in 0..0x20 {
            assert_eq!(base.glyph_name(code), None, "{base:?} {code}");
        }
        assert_eq!(base.glyph_name(b' '), Some("space"), "{base:?}");
    }
}

#[test]
fn only_the_named_encodings_are_found_by_name() {
    assert_eq!(
        BaseEncoding::from_name(b"WinAnsiEncoding"),
        Some(BaseEncoding::WinAnsi)
    );
    assert_eq!(
        BaseEncoding::from_name(b"MacRomanEncoding"),
        Some(BaseEncoding::MacRoman)
    );
    assert_eq!(
        BaseEncoding::from_name(b"StandardEncoding"),
        Some(BaseEncoding::Standard)
    );
    assert_eq!(BaseEncoding::from_name(b"PDFDocEncoding"), None);
    assert_eq!(BaseEncoding::from_name(b"Symbol"), None);
}

#[test]
fn differences_take_precedence_over_the_base_encoding() {
    let mut encoding = Encoding::new(BaseEncoding::WinAnsi);
    encoding.set(b'A', Name::new("Alpha"));
    assert_eq!(encoding.glyph_name(b'A'), Some(&b"Alpha"[..]));
    assert_eq!(encoding.glyph_name(b'B'), Some(&b"B"[..]));
    assert_eq!(encoding.glyph_name(0x81), None);
}

// Reading /Encoding

// The encoding read from `entry`, with `objects` in the file, for a font whose
// built-in encoding is the standard one.
fn read(objects: &[(u32, &str)], entry: &str) -> (Encoding, Vec<DiagnosticKind>) {
    let mut pdf = Pdf::new();
    let mut table = Vec::new();
    for &(number, body) in [(1, "<< /Type /Catalog /Pages 2 0 R >>")]
        .iter()
        .chain(objects)
    {
        let offset = pdf.object(number, body);
        table.push((
            number,
            XrefEntry::InUse {
                offset,
                generation: 0,
            },
        ));
    }
    let start = pdf.table(&table, "/Size 20 /Root 1 0 R");
    pdf.end(start);

    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();
    let entry = parse_object(entry.as_bytes(), Limits::default())
        .unwrap()
        .object;
    let mut diagnostics = Diagnostics::new(10);
    let encoding = read_encoding(
        &mut resolver,
        &entry,
        BaseEncoding::Standard,
        0,
        &mut diagnostics,
    );
    (encoding, diagnostics.iter().map(|d| d.kind()).collect())
}

#[test]
fn a_name_selects_a_base_encoding() {
    let (encoding, diagnostics) = read(&[], "/WinAnsiEncoding");
    assert_eq!(encoding, Encoding::new(BaseEncoding::WinAnsi));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_dictionary_adds_differences_to_its_base_encoding() {
    let (encoding, diagnostics) = read(
        &[],
        "<< /Type /Encoding /BaseEncoding /MacRomanEncoding \
         /Differences [32 /space 65 /Agrave /Aacute (x) /Acircumflex 200 /Euro] >>",
    );
    assert_eq!(encoding.base(), BaseEncoding::MacRoman);
    assert_eq!(encoding.glyph_name(32), Some(&b"space"[..]));
    assert_eq!(encoding.glyph_name(65), Some(&b"Agrave"[..]));
    assert_eq!(encoding.glyph_name(66), Some(&b"Aacute"[..]));
    assert_eq!(encoding.glyph_name(200), Some(&b"Euro"[..]));
    assert_eq!(encoding.glyph_name(68), Some(&b"D"[..]));
    // The string is skipped, and the names after it go on from where they were.
    assert_eq!(encoding.glyph_name(67), Some(&b"Acircumflex"[..]));
    assert_eq!(encoding.differences().len(), 5);
    assert_eq!(diagnostics, [DiagnosticKind::InvalidDifferences]);
}

#[test]
fn without_a_base_encoding_the_differences_change_the_built_in_one() {
    let (encoding, diagnostics) = read(&[], "<< /Differences [39 /quotesingle] >>");
    assert_eq!(encoding.base(), BaseEncoding::Standard);
    assert_eq!(encoding.glyph_name(39), Some(&b"quotesingle"[..]));
    assert_eq!(encoding.glyph_name(96), Some(&b"quoteleft"[..]));
    assert!(diagnostics.is_empty());
}

#[test]
fn references_are_resolved() {
    let (encoding, diagnostics) = read(
        &[
            (3, "<< /BaseEncoding /WinAnsiEncoding /Differences 4 0 R >>"),
            (4, "[1 /a /b]"),
        ],
        "3 0 R",
    );
    assert_eq!(encoding.base(), BaseEncoding::WinAnsi);
    assert_eq!(encoding.glyph_name(1), Some(&b"a"[..]));
    assert_eq!(encoding.glyph_name(2), Some(&b"b"[..]));
    assert!(diagnostics.is_empty());
}

#[test]
fn names_without_a_valid_code_are_skipped() {
    let (encoding, diagnostics) = read(
        &[],
        "<< /Differences [/orphan 255 /ydieresis /past 256 /big -1 /negative] >>",
    );
    assert_eq!(encoding.glyph_name(255), Some(&b"ydieresis"[..]));
    assert_eq!(encoding.differences().len(), 1);
    assert_eq!(diagnostics, [DiagnosticKind::InvalidDifferences]);
}

#[test]
fn an_unknown_encoding_name_falls_back_to_the_built_in_one() {
    let (encoding, diagnostics) = read(&[], "/PDFDocEncoding");
    assert_eq!(encoding, Encoding::new(BaseEncoding::Standard));
    assert_eq!(diagnostics, [DiagnosticKind::UnknownEncoding]);

    let (encoding, diagnostics) = read(&[], "<< /BaseEncoding /Klingon >>");
    assert_eq!(encoding, Encoding::new(BaseEncoding::Standard));
    assert_eq!(diagnostics, [DiagnosticKind::UnknownEncoding]);
}

#[test]
fn an_invalid_entry_falls_back_to_the_built_in_encoding() {
    for entry in ["42", "(WinAnsiEncoding)", "9 0 R", "<< /BaseEncoding 1 >>"] {
        let (encoding, diagnostics) = read(&[], entry);
        assert_eq!(encoding, Encoding::new(BaseEncoding::Standard), "{entry}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidEncoding], "{entry}");
    }
}

#[test]
fn differences_that_are_not_an_array_are_ignored() {
    let (encoding, diagnostics) = read(&[], "<< /Differences /space >>");
    assert_eq!(encoding, Encoding::new(BaseEncoding::Standard));
    assert_eq!(diagnostics, [DiagnosticKind::InvalidDifferences]);
}
