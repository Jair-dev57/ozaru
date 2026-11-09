use ozaru_engine::base::{DiagnosticKind, Diagnostics, Limits};
use ozaru_engine::fonts::{
    BaseEncoding, Extent, SimpleFont, SimpleFontKind, StandardFont, read_simple_font,
};
use ozaru_engine::syntax::ObjectId;
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

// Reads object 3 of a file with `objects` as a simple font.
fn font(objects: &[(u32, &str)]) -> (Option<SimpleFont>, Vec<DiagnosticKind>) {
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
    let id = ObjectId {
        number: 3,
        generation: 0,
    };
    let dictionary = resolver.resolve(id).unwrap();
    let mut diagnostics = Diagnostics::new(10);
    let font = read_simple_font(
        &mut resolver,
        dictionary.as_dictionary().unwrap(),
        Limits::default(),
        0,
        &mut diagnostics,
    );
    (font, diagnostics.iter().map(|d| d.kind()).collect())
}

fn text(font: &SimpleFont, code: u8) -> Option<String> {
    font.text(code).map(|text| text.into_owned())
}

#[test]
fn a_standard_font_without_an_encoding_uses_the_standard_one() {
    let (font, diagnostics) =
        font(&[(3, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")]);
    let font = font.unwrap();
    assert_eq!(font.kind(), SimpleFontKind::Type1);
    assert_eq!(font.base_font().unwrap().as_bytes(), b"Helvetica");
    assert_eq!(font.encoding().unwrap().base(), BaseEncoding::Standard);
    assert_eq!(text(&font, b'A').as_deref(), Some("A"));
    // In the standard encoding, the apostrophe is a right quotation mark.
    assert_eq!(text(&font, b'\'').as_deref(), Some("’"));
    assert_eq!(text(&font, 0xE9).as_deref(), Some("Ø"));
    assert_eq!(text(&font, 0x80), None);
    assert!(!font.is_embedded());
    assert!(diagnostics.is_empty());
}

#[test]
fn widths_start_at_first_char_and_scale_to_text_space() {
    let (font, diagnostics) = font(&[
        (
            3,
            "<< /Type /Font /Subtype /TrueType /BaseFont /Arial /Encoding /WinAnsiEncoding \
             /FirstChar 32 /LastChar 34 /Widths [278 278 355] /FontDescriptor 4 0 R >>",
        ),
        (4, "<< /Type /FontDescriptor /Flags 32 /MissingWidth 250 >>"),
    ]);
    let font = font.unwrap();
    assert_eq!(font.width(32), 0.278);
    assert_eq!(font.width(34), 0.355);
    assert!(font.has_width(33));
    // Outside of /Widths, the missing width.
    assert_eq!(font.width(35), 0.25);
    assert_eq!(font.width(0), 0.25);
    assert!(!font.has_width(35));
    assert_eq!(text(&font, 0xE9).as_deref(), Some("é"));
    assert_eq!(font.flags(), 32);
    assert!(!font.is_symbolic());
    assert!(diagnostics.is_empty());
}

#[test]
fn without_widths_or_missing_width_a_glyph_is_zero_wide() {
    let (font, _) = font(&[(3, "<< /Type /Font /Subtype /Type1 /BaseFont /Futura >>")]);
    assert_eq!(font.unwrap().width(b'A'), 0.0);
}

#[test]
fn to_unicode_takes_precedence_over_the_encoding() {
    let cmap = "1 beginbfchar <41> <0058> endbfchar";
    let stream = format!("<< /Length {} >>\nstream\n{cmap}\nendstream", cmap.len());
    let (font, diagnostics) = font(&[
        (
            3,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman \
             /Encoding << /Differences [66 /eacute] >> /ToUnicode 4 0 R >>",
        ),
        (4, &stream),
    ]);
    let font = font.unwrap();
    assert!(font.to_unicode().is_some());
    assert_eq!(text(&font, b'A').as_deref(), Some("X"));
    // Codes that the CMap doesn't map use the encoding.
    assert_eq!(text(&font, b'B').as_deref(), Some("é"));
    assert_eq!(font.glyph_name(b'B'), Some(&b"eacute"[..]));
    assert_eq!(text(&font, b'C').as_deref(), Some("C"));
    assert!(diagnostics.is_empty());
}

#[test]
fn symbol_and_zapf_dingbats_have_their_built_in_encodings() {
    let (symbol, _) = font(&[(3, "<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>")]);
    let symbol = symbol.unwrap();
    assert_eq!(symbol.encoding().unwrap().base(), BaseEncoding::Symbol);
    assert_eq!(text(&symbol, b'a').as_deref(), Some("α"));
    assert_eq!(text(&symbol, 0xB3).as_deref(), Some("≥"));

    let (zapf, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /ZapfDingbats >>",
    )]);
    let zapf = zapf.unwrap();
    assert_eq!(text(&zapf, b'!').as_deref(), Some("✁"));
    assert_eq!(text(&zapf, b' ').as_deref(), Some(" "));

    let (subset, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+Symbol >>",
    )]);
    assert_eq!(
        subset.unwrap().encoding().unwrap().base(),
        BaseEncoding::Symbol
    );

    // Only a real subset prefix is removed.
    let (other, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /abcdef+Symbol >>",
    )]);
    assert_eq!(
        other.unwrap().encoding().unwrap().base(),
        BaseEncoding::Standard
    );
}

#[test]
fn type_3_widths_are_scaled_by_the_font_matrix() {
    let (font, diagnostics) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 100 100] \
         /FontMatrix [0.01 0 0 0.01 0 0] /CharProcs << >> \
         /Encoding << /Type /Encoding /Differences [65 /uni25A0 /g7] >> \
         /FirstChar 65 /LastChar 66 /Widths [50 80] >>",
    )]);
    let font = font.unwrap();
    assert_eq!(font.kind(), SimpleFontKind::Type3);
    assert_eq!(font.width(b'A'), 0.5);
    assert_eq!(font.width(b'B'), 0.8);
    assert_eq!(font.font_matrix(), [0.01, 0.0, 0.0, 0.01, 0.0, 0.0]);
    assert_eq!(text(&font, b'A').as_deref(), Some("■"));
    // A glyph name that means nothing gives no text.
    assert_eq!(text(&font, b'B'), None);
    assert!(diagnostics.is_empty());
}

#[test]
fn an_invalid_type_3_font_matrix_is_replaced() {
    for matrix in [
        "",
        "/FontMatrix [0 0 0 0.01 0 0]",
        "/FontMatrix [1 2 3]",
        "/FontMatrix (x)",
    ] {
        let body =
            format!("<< /Type /Font /Subtype /Type3 {matrix} /FirstChar 0 /Widths [1000] >>");
        let (font, diagnostics) = font(&[(3, &body)]);
        let font = font.unwrap();
        assert_eq!(font.width(0), 1.0, "{matrix}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidFontMatrix], "{matrix}");
    }
}

#[test]
fn a_composite_font_isnt_a_simple_font() {
    let (font, diagnostics) = font(&[(3, "<< /Type /Font /Subtype /Type0 /BaseFont /Foo >>")]);
    assert!(font.is_none());
    assert!(diagnostics.is_empty());
}

#[test]
fn a_font_without_a_known_subtype_is_read_as_type_1() {
    for subtype in ["", "/Subtype /OpenType", "/Subtype 1"] {
        let body = format!("<< /Type /Font {subtype} /BaseFont /Helvetica >>");
        let (font, diagnostics) = font(&[(3, &body)]);
        assert_eq!(font.unwrap().kind(), SimpleFontKind::Type1, "{subtype}");
        assert_eq!(diagnostics, [DiagnosticKind::UnknownFontType], "{subtype}");
    }
}

#[test]
fn invalid_widths_are_reported() {
    for (entries, width_of_a) in [
        // Not an array, or no valid /FirstChar.
        ("/FirstChar 65 /Widths 500", 0.0),
        ("/Widths [500]", 0.0),
        ("/FirstChar 300 /Widths [500]", 0.0),
        ("/FirstChar -1 /Widths [500]", 0.0),
        // An item that isn't a number is 0, and the others are kept.
        ("/FirstChar 65 /Widths [(x) 600]", 0.0),
    ] {
        let body = format!("<< /Type /Font /Subtype /Type1 {entries} >>");
        let (font, diagnostics) = font(&[(3, &body)]);
        assert_eq!(font.unwrap().width(b'A'), width_of_a, "{entries}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidWidths], "{entries}");
    }
    let (font, _) = font(&[(3, "<< /Subtype /Type1 /FirstChar 65 /Widths [(x) 600] >>")]);
    assert_eq!(font.unwrap().width(b'B'), 0.6);
}

#[test]
fn widths_past_code_255_are_dropped() {
    let (font, diagnostics) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /FirstChar 254 /Widths [100 200 300 400] >>",
    )]);
    let font = font.unwrap();
    assert_eq!(font.width(255), 0.2);
    assert_eq!(diagnostics, [DiagnosticKind::InvalidWidths]);
}

#[test]
fn every_entry_can_be_a_reference() {
    let (font, diagnostics) = font(&[
        (
            3,
            "<< /Type /Font /Subtype 4 0 R /BaseFont 5 0 R /FirstChar 6 0 R \
             /Widths 7 0 R /FontDescriptor 8 0 R /Encoding 10 0 R >>",
        ),
        (4, "/TrueType"),
        (5, "/ABCDEF+DejaVuSans"),
        (6, "65"),
        (7, "[9 0 R 700]"),
        (
            8,
            "<< /Type /FontDescriptor /Flags 9 0 R /FontFile2 11 0 R >>",
        ),
        (9, "4"),
        (10, "/MacRomanEncoding"),
        (11, "<< /Length 0 >>\nstream\n\nendstream"),
    ]);
    let font = font.unwrap();
    assert_eq!(font.kind(), SimpleFontKind::TrueType);
    assert_eq!(font.base_font().unwrap().as_bytes(), b"ABCDEF+DejaVuSans");
    assert_eq!(font.width(b'A'), 0.004);
    assert_eq!(font.width(b'B'), 0.7);
    assert!(font.is_symbolic());
    assert!(font.is_embedded());
    assert_eq!(font.encoding().unwrap().base(), BaseEncoding::MacRoman);
    assert_eq!(text(&font, 0x8E).as_deref(), Some("é"));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_symbolic_truetype_font_without_an_encoding_has_no_glyph_names() {
    let cmap = "1 beginbfchar <01> <0041> endbfchar";
    let stream = format!("<< /Length {} >>\nstream\n{cmap}\nendstream", cmap.len());
    let (font, _) = font(&[
        (
            3,
            "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+DejaVuSans \
             /FontDescriptor << /Flags 4 >> /ToUnicode 4 0 R >>",
        ),
        (4, &stream),
    ]);
    let font = font.unwrap();
    assert!(font.encoding().is_none());
    assert_eq!(text(&font, 1).as_deref(), Some("A"));
    // Not the standard encoding's "B": the code selects a glyph of the font
    // program, which only the ToUnicode CMap names.
    assert_eq!(text(&font, b'B'), None);
    assert_eq!(font.glyph_name(b'B'), None);

    // Nonsymbolic, or with an /Encoding, it has names.
    let (font, _) = font_with_flags("32", "");
    assert!(font.encoding().is_some());
    let (font, _) = font_with_flags("4", "/Encoding /WinAnsiEncoding");
    assert_eq!(text(&font, 0xE9).as_deref(), Some("é"));
}

fn font_with_flags(flags: &str, encoding: &str) -> (SimpleFont, Vec<DiagnosticKind>) {
    let body = format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /Arial {encoding} \
         /FontDescriptor << /Flags {flags} >> >>"
    );
    let (font, diagnostics) = font(&[(3, &body)]);
    (font.unwrap(), diagnostics)
}

#[test]
fn a_standard_font_without_widths_uses_adobe_metrics() {
    let (font, diagnostics) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    )]);
    let font = font.unwrap();
    assert_eq!(font.standard_font(), Some(StandardFont::Helvetica));
    assert_eq!(font.width(b' '), 0.278);
    assert_eq!(font.width(b'A'), 0.667);
    // Accented letters aren't in the standard encoding, but have metrics.
    assert_eq!(font.width(0xE9), 0.556);
    assert_eq!(font.width(0x80), 0.556);
    assert!(font.has_width(0xE9));
    // Code 0x81 has no glyph in WinAnsi.
    assert!(!font.has_width(0x81));
    assert_eq!(font.width(0x81), 0.0);
    assert!(diagnostics.is_empty());
}

#[test]
fn widths_in_the_file_take_precedence_over_the_metrics() {
    let (font, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /FirstChar 65 /Widths [700] \
         /FontDescriptor << /MissingWidth 100 >> >>",
    )]);
    let font = font.unwrap();
    assert_eq!(font.width(b'A'), 0.7);
    // Outside /Widths, the missing width, not the metrics.
    assert_eq!(font.width(b'B'), 0.1);
}

#[test]
fn common_names_of_the_standard_fonts_get_their_metrics() {
    for (name, font_name, width_of_a) in [
        ("Arial", StandardFont::Helvetica, 0.667),
        ("Arial,Bold", StandardFont::HelveticaBold, 0.722),
        (
            "ABCDEF+Arial-BoldItalicMT",
            StandardFont::HelveticaBoldOblique,
            0.722,
        ),
        ("TimesNewRoman", StandardFont::TimesRoman, 0.722),
        ("TimesNewRomanPS-ItalicMT", StandardFont::TimesItalic, 0.611),
        ("CourierNew,Bold", StandardFont::CourierBold, 0.6),
        ("Times-BoldItalic", StandardFont::TimesBoldItalic, 0.667),
    ] {
        let body = format!("<< /Type /Font /Subtype /TrueType /BaseFont /{name} >>");
        let (font, _) = font(&[(3, &body)]);
        let font = font.unwrap();
        assert_eq!(font.standard_font(), Some(font_name), "{name}");
        assert_eq!(font.width(b'A'), width_of_a, "{name}");
    }
}

#[test]
fn symbol_and_zapf_dingbats_have_metrics_for_their_own_glyphs() {
    let (symbol, _) = font(&[(3, "<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>")]);
    let symbol = symbol.unwrap();
    assert_eq!(symbol.width(b'a'), 0.631);
    assert_eq!(symbol.width(0xE5), 0.713);
    let (zapf, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /ZapfDingbats >>",
    )]);
    assert_eq!(zapf.unwrap().width(b'!'), 0.974);
}

// A Type 1 font program whose clear text part has `encoding`, followed by
// `eexec` and data that isn't read.
fn type1_program(encoding: &str) -> String {
    let data = format!(
        "%!PS-AdobeFont-1.0: CMMI10 003.002\n/FontName /CMMI10 def\n{encoding}\n\
         currentdict end\ncurrentfile eexec\n\u{1}\u{2}\u{3} /dup 66 /B put"
    );
    format!(
        "<< /Length {} /Length1 {} >>\nstream\n{data}\nendstream",
        data.len(),
        data.len()
    )
}

const CUSTOM: &str = "/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\n\
    dup 65 /alpha put\ndup 66 /fi put\ndup 200 /uni2211 put\nreadonly def";

fn embedded_type1(encoding_entry: &str, program: &str) -> SimpleFont {
    let body = format!(
        "<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+CMMI10 {encoding_entry} \
         /FontDescriptor 4 0 R >>"
    );
    let program = type1_program(program);
    let (font, diagnostics) = font(&[
        (3, &body),
        (4, "<< /Type /FontDescriptor /Flags 4 /FontFile 5 0 R >>"),
        (5, &program),
    ]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    font.unwrap()
}

#[test]
fn an_embedded_type_1_font_uses_its_built_in_encoding() {
    let font = embedded_type1("", CUSTOM);
    assert!(font.is_embedded());
    assert_eq!(text(&font, b'A').as_deref(), Some("α"));
    assert_eq!(text(&font, b'B').as_deref(), Some("ﬁ"));
    assert_eq!(text(&font, 200).as_deref(), Some("∑"));
    // The program's encoding has no base: other codes have no glyph.
    assert_eq!(text(&font, b'C'), None);
    assert!(!font.encoding().unwrap().has_base());
}

#[test]
fn differences_change_the_built_in_encoding_of_the_program() {
    let font = embedded_type1("/Encoding << /Differences [67 /C] >>", CUSTOM);
    assert_eq!(text(&font, b'A').as_deref(), Some("α"));
    assert_eq!(text(&font, b'C').as_deref(), Some("C"));
}

#[test]
fn a_named_encoding_replaces_the_program_one() {
    let font = embedded_type1("/Encoding /WinAnsiEncoding", CUSTOM);
    assert_eq!(text(&font, b'A').as_deref(), Some("A"));
    let font = embedded_type1("/Encoding << /BaseEncoding /MacRomanEncoding >>", CUSTOM);
    assert_eq!(text(&font, b'A').as_deref(), Some("A"));
}

#[test]
fn a_program_with_the_standard_encoding_or_none_uses_the_standard_one() {
    for program in [
        "/Encoding StandardEncoding def",
        "",
        "/Encoding 256 array readonly def",
    ] {
        let font = embedded_type1("", program);
        assert_eq!(text(&font, b'A').as_deref(), Some("A"), "{program}");
        assert_eq!(text(&font, b'\'').as_deref(), Some("’"), "{program}");
        assert!(font.encoding().unwrap().has_base(), "{program}");
    }
}

fn extent_of(font_entries: &str, descriptor: &str) -> Extent {
    let font_body = format!("<< /Type /Font {font_entries} /FontDescriptor 4 0 R >>");
    let descriptor = format!("<< /Type /FontDescriptor {descriptor} >>");
    let (font, _) = font(&[(3, &font_body), (4, &descriptor)]);
    font.unwrap().extent()
}

fn close_extent(extent: Extent, ascent: f64, descent: f64) -> bool {
    (extent.ascent - ascent).abs() < 1e-9 && (extent.descent - descent).abs() < 1e-9
}

#[test]
fn the_extent_comes_from_the_descriptor() {
    let extent = extent_of(
        "/Subtype /TrueType /BaseFont /DejaVuSans",
        "/Ascent 928 /Descent -235 /FontBBox [-1020 -462 1793 1232]",
    );
    assert!(close_extent(extent, 0.928, -0.235), "{extent:?}");
}

#[test]
fn a_standard_font_without_a_usable_descriptor_uses_its_metrics() {
    let extent = extent_of(
        "/Subtype /Type1 /BaseFont /Helvetica",
        "/Ascent 0 /Descent 0",
    );
    assert!(close_extent(extent, 0.718, -0.207), "{extent:?}");
    let (courier, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold >>",
    )]);
    assert!(close_extent(courier.unwrap().extent(), 0.629, -0.157));
    let (symbol, _) = font(&[(3, "<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>")]);
    assert!(close_extent(symbol.unwrap().extent(), 1.010, -0.293));
}

#[test]
fn without_ascent_and_descent_the_font_bbox_is_used() {
    let extent = extent_of(
        "/Subtype /TrueType /BaseFont /Unknown",
        "/Ascent 5000 /Descent -10 /FontBBox [0 -300 1000 900]",
    );
    assert!(close_extent(extent, 0.9, -0.3), "{extent:?}");
}

#[test]
fn a_font_that_says_nothing_gets_the_default_extent() {
    let (unknown, _) = font(&[(3, "<< /Type /Font /Subtype /TrueType /BaseFont /Unknown >>")]);
    assert_eq!(unknown.unwrap().extent(), Extent::DEFAULT);
    let extent = extent_of(
        "/Subtype /TrueType /BaseFont /Unknown",
        "/Ascent 800 /Descent 200 /FontBBox [0 0 0 0]",
    );
    assert_eq!(extent, Extent::DEFAULT);
}

#[test]
fn a_type_3_extent_is_scaled_by_the_font_matrix() {
    let extent = extent_of(
        "/Subtype /Type3 /FontMatrix [0.01 0 0 0.01 0 0] /FontBBox [0 0 100 100] \
         /CharProcs << >> /Encoding << /Differences [] >>",
        "/Ascent 80 /Descent -20",
    );
    assert!(close_extent(extent, 0.8, -0.2), "{extent:?}");
    // A matrix that flips the glyphs upside down flips their extent too.
    let extent = extent_of(
        "/Subtype /Type3 /FontMatrix [0.01 0 0 -0.01 0 0] /FontBBox [0 0 100 100] \
         /CharProcs << >> /Encoding << /Differences [] >>",
        "/Ascent 80 /Descent -20",
    );
    assert!(close_extent(extent, 0.2, -0.8), "{extent:?}");
}

#[test]
fn a_type_3_font_without_a_descriptor_uses_its_own_font_bbox() {
    let (font, _) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type3 /FontMatrix [0.01 0 0 0.01 0 0] /FontBBox [0 -10 100 90] \
         /CharProcs << >> /Encoding << /Differences [] >> >>",
    )]);
    assert!(close_extent(font.unwrap().extent(), 0.9, -0.1));
}
