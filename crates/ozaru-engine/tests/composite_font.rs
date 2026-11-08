use ozaru_engine::base::{DiagnosticKind, Diagnostics, Limits};
use ozaru_engine::fonts::{
    CidCMap, CidFontKind, Code, CompositeFont, VerticalMetrics, read_composite_font,
};
use ozaru_engine::syntax::ObjectId;
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

fn stream(data: &str) -> String {
    format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len())
}

// Reads object 3 of a file with `objects` as a composite font.
fn font(objects: &[(u32, String)]) -> (Option<CompositeFont>, Vec<DiagnosticKind>) {
    let mut pdf = Pdf::new();
    let mut table = Vec::new();
    let catalog = (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string());
    for (number, body) in std::iter::once(&catalog).chain(objects) {
        let offset = pdf.object(*number, body);
        table.push((
            *number,
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
    let font = read_composite_font(
        &mut resolver,
        dictionary.as_dictionary().unwrap(),
        Limits::default(),
        0,
        &mut diagnostics,
    );
    (font, diagnostics.iter().map(|d| d.kind()).collect())
}

fn type0(encoding: &str, descendant: &str) -> String {
    format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+NotoSans {encoding} /DescendantFonts [{descendant}] >>"
    )
}

const DESCENDANT: &str = "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /ABCDEF+NotoSans \
    /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
    /FontDescriptor << /Flags 4 /FontFile2 9 0 R >> /DW 500 \
    /W [3 [250 300] 10 20 600] >>";

fn code(value: u32, length: u8) -> Code {
    Code { value, length }
}

fn text(font: &CompositeFont, code: Code) -> Option<String> {
    font.text(code).map(|text| text.into_owned())
}

#[test]
fn identity_h_reads_two_byte_codes_that_are_cids() {
    let cmap = "2 beginbfchar <0003> <0048> <0004> <00F1> endbfchar";
    let (font, diagnostics) = font(&[
        (
            3,
            format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+NotoSans /Encoding /Identity-H \
                 /DescendantFonts [{DESCENDANT}] /ToUnicode 4 0 R >>"
            ),
        ),
        (4, stream(cmap)),
    ]);
    let font = font.unwrap();
    assert_eq!(font.cmap().name().unwrap().as_bytes(), b"Identity-H");
    assert!(!font.cmap().is_vertical());
    assert_eq!(font.codes(b"\x00\x03\x00\x04"), [code(3, 2), code(4, 2)]);
    assert_eq!(font.cid(code(4, 2)), Some(4));
    assert_eq!(text(&font, code(3, 2)).as_deref(), Some("H"));
    assert_eq!(text(&font, code(4, 2)).as_deref(), Some("ñ"));
    assert_eq!(text(&font, code(5, 2)), None);
    assert_eq!(font.kind(), CidFontKind::Type2);
    assert_eq!(font.system_info(), Some(("Adobe", "Identity", 0)));
    assert!(font.is_embedded());
    assert!(diagnostics.is_empty());
}

#[test]
fn widths_come_from_w_or_dw() {
    let (font, _) = font(&[(3, type0("/Encoding /Identity-H", DESCENDANT))]);
    let font = font.unwrap();
    assert_eq!(font.width(code(3, 2)), 0.25);
    assert_eq!(font.width(code(4, 2)), 0.3);
    assert_eq!(font.width(code(10, 2)), 0.6);
    assert_eq!(font.width(code(20, 2)), 0.6);
    assert_eq!(font.width(code(21, 2)), 0.5);
    assert_eq!(font.cid_width(15), Some(600.0));
    assert_eq!(font.cid_width(5), None);

    // Without /DW, the default is 1000.
    let descendant = "<< /Subtype /CIDFontType0 >>";
    let (font, _) = font_with(descendant);
    assert_eq!(font.width(code(1, 2)), 1.0);
    assert_eq!(font.kind(), CidFontKind::Type0);
}

fn font_with(descendant: &str) -> (CompositeFont, Vec<DiagnosticKind>) {
    let (font, diagnostics) = font(&[(3, type0("/Encoding /Identity-H", descendant))]);
    (font.unwrap(), diagnostics)
}

#[test]
fn a_code_cut_short_takes_the_bytes_left() {
    let (font, _) = font_with(DESCENDANT);
    assert_eq!(font.codes(b"\x00\x03\x00"), [code(3, 2), code(0, 1)]);
    assert_eq!(font.codes(b""), []);
}

#[test]
fn identity_v_is_vertical() {
    let (font, _) = font(&[(3, type0("/Encoding /Identity-V", DESCENDANT))]);
    assert!(font.unwrap().cmap().is_vertical());
}

#[test]
fn unicode_cmaps_give_the_text_of_their_codes() {
    let ucs2 = CidCMap::predefined(b"UniGB-UCS2-H").unwrap();
    let codes = ucs2.codes(
        "中文"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>()
            .as_slice(),
    );
    assert_eq!(codes.len(), 2);
    assert_eq!(ucs2.unicode(codes[0]).as_deref(), Some("中"));
    assert_eq!(ucs2.cid(codes[0]), None);

    let utf16 = CidCMap::predefined(b"UniJIS-UTF16-V").unwrap();
    assert!(utf16.is_vertical());
    let bytes: Vec<u8> = "😀a".encode_utf16().flat_map(u16::to_be_bytes).collect();
    let codes = utf16.codes(&bytes);
    assert_eq!(codes, [code(0xD83D_DE00, 4), code(0x61, 2)]);
    assert_eq!(utf16.unicode(codes[0]).as_deref(), Some("😀"));

    let utf8 = CidCMap::predefined(b"UniKS-UTF8-H").unwrap();
    let codes = utf8.codes("añ한".as_bytes());
    assert_eq!(codes.len(), 3);
    let text: String = codes
        .iter()
        .filter_map(|&code| utf8.unicode(code))
        .collect();
    assert_eq!(text, "añ한");

    let utf32 = CidCMap::predefined(b"UniCNS-UTF32-H").unwrap();
    let codes = utf32.codes(&0x1F600u32.to_be_bytes());
    assert_eq!(utf32.unicode(codes[0]).as_deref(), Some("😀"));
}

#[test]
fn a_font_with_a_unicode_cmap_has_text_and_default_widths() {
    let (font, diagnostics) = font(&[(3, type0("/Encoding /UniGB-UCS2-H", DESCENDANT))]);
    let font = font.unwrap();
    let codes = font.codes(&[0x4E, 0x2D]);
    assert_eq!(text(&font, codes[0]).as_deref(), Some("中"));
    assert_eq!(font.width(codes[0]), 0.5);
    assert!(diagnostics.is_empty());
}

#[test]
fn only_known_predefined_cmaps_are_found() {
    for name in [
        "90ms-RKSJ-H",
        "GBK-EUC-H",
        "Identity",
        "UniGB-UCS2",
        "UniGB-XYZ-H",
        "Foo-H",
        "",
    ] {
        assert_eq!(CidCMap::predefined(name.as_bytes()), None, "{name}");
    }
}

#[test]
fn an_unsupported_cmap_falls_back_to_identity_h() {
    let (font, diagnostics) = font(&[(3, type0("/Encoding /90ms-RKSJ-H", DESCENDANT))]);
    assert_eq!(font.unwrap().cmap(), &CidCMap::identity(false));
    assert_eq!(diagnostics, [DiagnosticKind::UnsupportedCMap]);
}

#[test]
fn a_missing_or_invalid_encoding_is_identity_h() {
    for encoding in ["", "/Encoding 7", "/Encoding 8 0 R"] {
        let (font, diagnostics) = font(&[
            (3, type0(encoding, DESCENDANT)),
            (8, "<< /Type /CMap >>".to_string()),
        ]);
        assert_eq!(
            font.unwrap().cmap(),
            &CidCMap::identity(false),
            "{encoding}"
        );
        assert_eq!(diagnostics, [DiagnosticKind::InvalidCMap], "{encoding}");
    }
}

// A CMap like the predefined ones for Japanese: one-byte codes up to 0x80 and
// two-byte codes from 0x8140.
const MIXED: &str = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
    /CMapName /Test-H def\n\
    2 begincodespacerange <00> <80> <8140> <9FFC> endcodespacerange\n\
    1 begincidrange <20> <7E> 1 endcidrange\n\
    1 begincidrange <8140> <817E> 633 endcidrange\n\
    1 begincidchar <8150> 9999 endcidchar\n\
    endcmap CMapName currentdict /CMap defineresource pop end end";

#[test]
fn an_embedded_cmap_reads_codes_of_mixed_lengths() {
    let (font, diagnostics) = font(&[
        (3, type0("/Encoding 4 0 R", DESCENDANT)),
        (4, stream(MIXED)),
    ]);
    let font = font.unwrap();
    assert_eq!(font.cmap().name().unwrap().as_bytes(), b"Test-H");
    let codes = font.codes(b"A\x81\x41B\x81\x50");
    assert_eq!(
        codes,
        [
            code(0x41, 1),
            code(0x8141, 2),
            code(0x42, 1),
            code(0x8150, 2)
        ]
    );
    assert_eq!(font.cid(codes[0]), Some(34));
    assert_eq!(font.cid(codes[1]), Some(634));
    assert_eq!(font.cid(codes[3]), Some(9999));
    // A one-byte code that no range maps has no CID.
    assert_eq!(font.cid(code(0x10, 1)), None);
    assert!(diagnostics.is_empty());
}

#[test]
fn bytes_outside_the_codespace_take_the_length_of_a_matching_range() {
    let (font, _) = font(&[
        (3, type0("/Encoding 4 0 R", DESCENDANT)),
        (4, stream(MIXED)),
    ]);
    let font = font.unwrap();
    // 0x81 0x20 isn't in <8140> <9FFC>, but its first byte is: two bytes.
    assert_eq!(font.codes(b"\x81\x20"), [code(0x8120, 2)]);
    // 0xFF matches no range at all: one byte.
    assert_eq!(font.codes(b"\xFFA"), [code(0xFF, 1), code(0x41, 1)]);
}

#[test]
fn an_embedded_cmap_can_use_another() {
    // `usecmap` with a predefined name.
    let own = "/Identity-H usecmap 1 begincidchar <0041> 7 endcidchar";
    let (used, diagnostics) = font(&[(3, type0("/Encoding 4 0 R", DESCENDANT)), (4, stream(own))]);
    let used = used.unwrap();
    assert_eq!(used.cid(code(0x41, 2)), Some(7));
    assert_eq!(used.cid(code(0x42, 2)), Some(0x42));
    assert_eq!(used.codes(b"\x00\x42"), [code(0x42, 2)]);
    assert!(diagnostics.is_empty());

    // `/UseCMap` with a stream.
    let parent = "1 begincodespacerange <00> <FF> endcodespacerange 1 begincidrange <00> <FF> 100 endcidrange";
    let child = "1 begincidchar <41> 1 endcidchar";
    let (font, diagnostics) = font(&[
        (3, type0("/Encoding 4 0 R", DESCENDANT)),
        (
            4,
            format!(
                "<< /Length {} /UseCMap 5 0 R >>\nstream\n{child}\nendstream",
                child.len()
            ),
        ),
        (5, stream(parent)),
    ]);
    let font = font.unwrap();
    assert_eq!(font.cid(code(0x41, 1)), Some(1));
    assert_eq!(font.cid(code(0x42, 1)), Some(166));
    assert!(diagnostics.is_empty());
}

#[test]
fn cmaps_that_use_each_other_in_a_cycle_stop() {
    let data = "1 begincidchar <41> 1 endcidchar";
    let (font, diagnostics) = font(&[
        (3, type0("/Encoding 4 0 R", DESCENDANT)),
        (
            4,
            format!(
                "<< /Length {} /UseCMap 4 0 R >>\nstream\n{data}\nendstream",
                data.len()
            ),
        ),
    ]);
    assert_eq!(font.unwrap().cid(code(0x41, 1)), Some(1));
    assert!(diagnostics.contains(&DiagnosticKind::InvalidCMap));
}

#[test]
fn a_vertical_embedded_cmap_is_found_by_its_wmode() {
    let data = "/WMode 1 def 1 begincodespacerange <0000> <FFFF> endcodespacerange";
    let (font, _) = font(&[(3, type0("/Encoding 4 0 R", DESCENDANT)), (4, stream(data))]);
    assert!(font.unwrap().cmap().is_vertical());
}

#[test]
fn a_missing_descendant_font_gives_the_defaults() {
    for descendants in [
        "",
        "/DescendantFonts []",
        "/DescendantFonts [7]",
        "/DescendantFonts 5",
    ] {
        let body = format!("<< /Type /Font /Subtype /Type0 /Encoding /Identity-H {descendants} >>");
        let (read, diagnostics) = font(&[(3, body)]);
        let read = read.unwrap();
        assert_eq!(read.width(code(1, 2)), 1.0, "{descendants}");
        assert_eq!(read.system_info(), None);
        assert_eq!(
            diagnostics,
            [DiagnosticKind::InvalidDescendantFont],
            "{descendants}"
        );
    }
    let (_, diagnostics) = font_with("<< /Subtype /CIDFontType9 >>");
    assert_eq!(diagnostics, [DiagnosticKind::InvalidDescendantFont]);
}

#[test]
fn invalid_widths_keep_those_before_the_error() {
    for (w, width_of_3) in [
        ("/W [3 [250] (x) 5 6]", 0.25),
        ("/W [3 [250] 10]", 0.25),
        ("/W [3 [250] 10 5 600]", 0.25),
        ("/W [3 [250 (x)]]", 0.25),
        ("/W 7", 1.0),
        ("/W [-1 [250]]", 1.0),
    ] {
        let (font, diagnostics) = font_with(&format!("<< /Subtype /CIDFontType2 {w} >>"));
        assert_eq!(font.width(code(3, 2)), width_of_3, "{w}");
        assert_eq!(diagnostics, [DiagnosticKind::InvalidCidWidths], "{w}");
    }
}

#[test]
fn only_type_0_fonts_are_composite() {
    let (font, diagnostics) = font(&[(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    )]);
    assert!(font.is_none());
    assert!(diagnostics.is_empty());
}

fn vertical(width: f64, origin_x: f64, origin_y: f64) -> VerticalMetrics {
    VerticalMetrics {
        width,
        origin_x,
        origin_y,
    }
}

#[test]
fn vertical_metrics_come_from_w2_or_dw2() {
    let descendant = "<< /Subtype /CIDFontType0 /DW 1000 /W [5 [600]] \
                      /DW2 [900 -1100] /W2 [3 [-800 300 700] 10 12 -500 250 450] >>";
    let (font, diagnostics) = font(&[(3, type0("/Encoding /Identity-V", descendant))]);
    let font = font.unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(font.cmap().is_vertical());
    assert_eq!(font.vertical_metrics(code(3, 2)), vertical(-0.8, 0.3, 0.7));
    assert_eq!(
        font.vertical_metrics(code(11, 2)),
        vertical(-0.5, 0.25, 0.45)
    );
    // Not in /W2: /DW2, with the origin half the horizontal width across.
    assert_eq!(font.vertical_metrics(code(5, 2)), vertical(-1.1, 0.3, 0.9));
    assert_eq!(font.vertical_metrics(code(4, 2)), vertical(-1.1, 0.5, 0.9));
}

#[test]
fn without_dw2_the_default_is_880_and_minus_1000() {
    let (font, _) = font(&[(
        3,
        type0("/Encoding /Identity-V", "<< /Subtype /CIDFontType2 >>"),
    )]);
    let font = font.unwrap();
    assert_eq!(font.vertical_metrics(code(1, 2)), vertical(-1.0, 0.5, 0.88));
}

#[test]
fn invalid_w2_keeps_what_comes_before() {
    let descendant = "<< /Subtype /CIDFontType2 /W2 [1 [-900 500 880] 7 /x] >>";
    let (font, diagnostics) = font(&[(3, type0("/Encoding /Identity-V", descendant))]);
    let font = font.unwrap();
    assert_eq!(font.vertical_metrics(code(1, 2)), vertical(-0.9, 0.5, 0.88));
    assert_eq!(diagnostics, [DiagnosticKind::InvalidCidWidths]);
}
