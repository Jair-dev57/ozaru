use ozaru_engine::base::{DiagnosticKind, Diagnostics, Limits};
use ozaru_engine::fonts::{Code, ToUnicode, read_to_unicode};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

// A CMap with the usual header and trailer around `body`.
fn cmap(body: &str) -> String {
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         {body}\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

fn parse(body: &str) -> (ToUnicode, Vec<DiagnosticKind>) {
    let mut diagnostics = Diagnostics::new(10);
    let to_unicode = ToUnicode::parse(
        cmap(body).as_bytes(),
        Limits::default(),
        0,
        &mut diagnostics,
    );
    (to_unicode, diagnostics.iter().map(|d| d.kind()).collect())
}

fn text(to_unicode: &ToUnicode, code: u32) -> Option<String> {
    to_unicode.get(code).map(|text| text.into_owned())
}

#[test]
fn bfchar_maps_single_codes() {
    let (to_unicode, diagnostics) = parse(
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         3 beginbfchar\n<0003> <0020>\n<0011> <00F1>\n<0024> <20AC>\nendbfchar",
    );
    assert_eq!(text(&to_unicode, 3).as_deref(), Some(" "));
    assert_eq!(text(&to_unicode, 0x11).as_deref(), Some("ñ"));
    assert_eq!(text(&to_unicode, 0x24).as_deref(), Some("€"));
    assert_eq!(text(&to_unicode, 0x25), None);
    assert!(diagnostics.is_empty());
}

#[test]
fn the_codespace_is_kept() {
    let (to_unicode, _) =
        parse("2 begincodespacerange\n<00> <80>\n<8140> <9FFC>\nendcodespacerange");
    assert_eq!(
        to_unicode.codespace(),
        [
            (
                Code {
                    value: 0,
                    length: 1
                },
                Code {
                    value: 0x80,
                    length: 1
                }
            ),
            (
                Code {
                    value: 0x8140,
                    length: 2
                },
                Code {
                    value: 0x9FFC,
                    length: 2
                }
            ),
        ]
    );
    assert!(to_unicode.is_empty());
}

#[test]
fn a_code_can_map_to_several_characters_or_to_none() {
    let (to_unicode, _) =
        parse("3 beginbfchar\n<01> <00660069>\n<02> <D835DC00>\n<03> <>\nendbfchar");
    assert_eq!(text(&to_unicode, 1).as_deref(), Some("fi"));
    // A surrogate pair: MATHEMATICAL BOLD CAPITAL A.
    assert_eq!(text(&to_unicode, 2).as_deref(), Some("𝐀"));
    assert_eq!(text(&to_unicode, 3).as_deref(), Some(""));
}

#[test]
fn bfrange_increments_the_last_character() {
    let (to_unicode, diagnostics) =
        parse("2 beginbfrange\n<0041> <005A> <0061>\n<0100> <0102> <D83DDE00>\nendbfrange");
    assert_eq!(text(&to_unicode, 0x41).as_deref(), Some("a"));
    assert_eq!(text(&to_unicode, 0x5A).as_deref(), Some("z"));
    assert_eq!(text(&to_unicode, 0x5B), None);
    assert_eq!(text(&to_unicode, 0x40), None);
    assert_eq!(text(&to_unicode, 0x102).as_deref(), Some("😂"));
    assert!(diagnostics.is_empty());
}

#[test]
fn bfrange_can_list_each_destination() {
    let (to_unicode, _) = parse("1 beginbfrange\n<10> <13> [<0066> <00660066> /fi]\nendbfrange");
    assert_eq!(text(&to_unicode, 0x10).as_deref(), Some("f"));
    assert_eq!(text(&to_unicode, 0x11).as_deref(), Some("ff"));
    assert_eq!(text(&to_unicode, 0x12).as_deref(), Some("ﬁ"));
    // The list is shorter than the range.
    assert_eq!(text(&to_unicode, 0x13), None);
}

#[test]
fn a_range_doesnt_go_past_the_last_utf16_unit() {
    let (to_unicode, _) = parse("1 beginbfrange\n<00> <02> <FFFE>\nendbfrange");
    assert_eq!(text(&to_unicode, 1).as_deref(), Some("\u{FFFF}"));
    assert_eq!(text(&to_unicode, 2), None);
}

#[test]
fn single_byte_destinations_and_glyph_names_are_accepted() {
    let (to_unicode, diagnostics) = parse("2 beginbfchar\n<20> <20>\n<41> /Aacute\nendbfchar");
    assert_eq!(text(&to_unicode, 0x20).as_deref(), Some(" "));
    assert_eq!(text(&to_unicode, 0x41).as_deref(), Some("Á"));
    assert!(diagnostics.is_empty());
}

#[test]
fn bfchar_takes_precedence_and_the_closest_range_wins() {
    let (to_unicode, _) = parse(
        "1 beginbfchar\n<05> <0058>\nendbfchar\n\
         2 beginbfrange\n<00> <FF> <0100>\n<04> <06> <0041>\nendbfrange",
    );
    assert_eq!(text(&to_unicode, 5).as_deref(), Some("X"));
    assert_eq!(text(&to_unicode, 4).as_deref(), Some("A"));
    assert_eq!(text(&to_unicode, 6).as_deref(), Some("C"));
    assert_eq!(text(&to_unicode, 7).as_deref(), Some("\u{0107}"));
}

#[test]
fn a_huge_range_is_not_expanded() {
    let (to_unicode, _) = parse("1 beginbfrange\n<00000000> <FFFFFFFF> <0000>\nendbfrange");
    assert_eq!(text(&to_unicode, 0x41).as_deref(), Some("A"));
    assert_eq!(text(&to_unicode, 0x10000), None);
}

#[test]
fn invalid_entries_are_skipped_and_reported() {
    let (to_unicode, diagnostics) = parse(
        "4 beginbfchar\n<0102030405> <0041>\n<10> 42\n<11> <0042>\n<12>\nendbfchar\n\
         2 beginbfrange\n<20> <10> <0041>\n<0030> <31> <0041>\nendbfrange",
    );
    assert_eq!(text(&to_unicode, 0x11).as_deref(), Some("B"));
    assert_eq!(text(&to_unicode, 0x20), None);
    assert_eq!(text(&to_unicode, 0x30), None);
    assert_eq!(diagnostics, [DiagnosticKind::DamagedCMap]);
}

#[test]
fn a_section_without_its_end_still_counts() {
    let mut diagnostics = Diagnostics::new(10);
    let to_unicode = ToUnicode::parse(
        b"1 beginbfchar <41> <0061>",
        Limits::default(),
        0,
        &mut diagnostics,
    );
    assert_eq!(text(&to_unicode, 0x41).as_deref(), Some("a"));
    assert!(diagnostics.is_empty());
}

#[test]
fn broken_syntax_keeps_what_came_before() {
    for tail in ["[<0041> <0042>", "<41 >> <zz>", ")"] {
        let data = format!("1 beginbfchar <41> <0061> endbfchar 1 beginbfrange <42> <43> {tail}");
        let mut diagnostics = Diagnostics::new(10);
        let to_unicode = ToUnicode::parse(data.as_bytes(), Limits::default(), 0, &mut diagnostics);
        assert_eq!(text(&to_unicode, 0x41).as_deref(), Some("a"), "{tail}");
        let kinds: Vec<_> = diagnostics.iter().map(|d| d.kind()).collect();
        assert_eq!(kinds, [DiagnosticKind::DamagedCMap], "{tail}");
    }
}

#[test]
fn garbage_gives_an_empty_cmap() {
    for data in [&b""[..], b"%!PS\n", b"\x00\xFF\x80 begin end def", b"<<<<"] {
        let mut diagnostics = Diagnostics::new(10);
        let to_unicode = ToUnicode::parse(data, Limits::default(), 0, &mut diagnostics);
        assert!(to_unicode.is_empty(), "{data:?}");
    }
}

// Reading /ToUnicode from a file

fn file_with(objects: &[(u32, String)]) -> Vec<u8> {
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
    pdf.file
}

fn read(file: &[u8], entry: Object) -> (Option<ToUnicode>, Vec<DiagnosticKind>) {
    let mut resolver = Resolver::open(file, Limits::default()).unwrap();
    let mut diagnostics = Diagnostics::new(10);
    let to_unicode = read_to_unicode(
        &mut resolver,
        &entry,
        Limits::default(),
        0,
        &mut diagnostics,
    );
    (to_unicode, diagnostics.iter().map(|d| d.kind()).collect())
}

const REFERENCE: Object = Object::Reference(ObjectId {
    number: 3,
    generation: 0,
});

#[test]
fn to_unicode_is_read_from_its_stream() {
    let data = cmap("1 beginbfchar\n<01> <00E9>\nendbfchar");
    let stream = format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len());
    let file = file_with(&[(3, stream)]);
    let (to_unicode, diagnostics) = read(&file, REFERENCE);
    assert_eq!(text(&to_unicode.unwrap(), 1).as_deref(), Some("é"));
    assert!(diagnostics.is_empty());
}

#[test]
fn a_compressed_to_unicode_is_decoded() {
    // `1 beginbfchar <01> <0041> endbfchar`, in ASCIIHex.
    let hex: String = b"1 beginbfchar <01> <0041> endbfchar"
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    let stream = format!(
        "<< /Length {} /Filter /ASCIIHexDecode >>\nstream\n{hex}>\nendstream",
        hex.len() + 1
    );
    let file = file_with(&[(3, stream)]);
    let (to_unicode, _) = read(&file, REFERENCE);
    assert_eq!(text(&to_unicode.unwrap(), 1).as_deref(), Some("A"));
}

#[test]
fn a_to_unicode_that_isnt_a_stream_is_ignored() {
    let file = file_with(&[(3, "<< /Type /CMap >>".to_string())]);
    for entry in [REFERENCE, Object::Integer(3)] {
        let (to_unicode, diagnostics) = read(&file, entry);
        assert_eq!(to_unicode, None);
        assert_eq!(diagnostics, [DiagnosticKind::InvalidToUnicode]);
    }
    // A predefined CMap name isn't allowed in /ToUnicode.
    let name = Object::Name(ozaru_engine::syntax::Name::new("Identity-H"));
    let (to_unicode, diagnostics) = read(&file, name);
    assert_eq!(to_unicode, None);
    assert_eq!(diagnostics, [DiagnosticKind::InvalidToUnicode]);
}
