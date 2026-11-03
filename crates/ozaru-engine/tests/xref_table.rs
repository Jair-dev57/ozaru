use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::{ParsedXrefTable, XrefEntry, parse_xref_table};

fn parse_at(input: &[u8], offset: usize) -> ParsedXrefTable {
    parse_xref_table(input, offset, Limits::default()).expect("input should be valid")
}

fn parse(input: &[u8]) -> ParsedXrefTable {
    parse_at(input, 0)
}

fn error(input: &[u8]) -> (ErrorKind, usize) {
    let error = parse_xref_table(input, 0, Limits::default()).expect_err("should fail");
    (error.kind(), error.offset())
}

fn diagnostics(parsed: &ParsedXrefTable) -> Vec<(DiagnosticKind, usize)> {
    parsed
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

fn position(input: &[u8], text: &[u8]) -> usize {
    input
        .windows(text.len())
        .position(|w| w == text)
        .expect("text should be in the input")
}

fn in_use(offset: usize, generation: u16) -> XrefEntry {
    XrefEntry::InUse { offset, generation }
}

fn free(generation: u16) -> XrefEntry {
    XrefEntry::Free { generation }
}

// The example of ISO 32000-2, section 7.5.4, with CR LF line ends.
const STANDARD: &[u8] = b"xref\r\n\
0 6\r\n\
0000000003 65535 f\r\n\
0000000017 00000 n\r\n\
0000000081 00000 n\r\n\
0000000000 00007 f\r\n\
0000000331 00000 n\r\n\
0000000409 00000 n\r\n\
trailer\r\n\
<< /Size 6 /Root 1 0 R >>\r\n";

// Valid tables

#[test]
fn the_example_of_the_standard_is_read() {
    let parsed = parse(STANDARD);
    let table = &parsed.table;

    assert_eq!(table.len(), 6);
    assert_eq!(
        table.iter().collect::<Vec<_>>(),
        [
            (0, free(65535)),
            (1, in_use(17, 0)),
            (2, in_use(81, 0)),
            (3, free(7)),
            (4, in_use(331, 0)),
            (5, in_use(409, 0)),
        ]
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn the_trailer_dictionary_is_kept() {
    let parsed = parse(STANDARD);
    let trailer = parsed.table.trailer();

    assert_eq!(trailer.get(b"Size"), Some(&Object::Integer(6)));
    assert_eq!(
        trailer.get(b"Root"),
        Some(&Object::Reference(ObjectId::new(1, 0)))
    );
    assert_eq!(parsed.end, STANDARD.len() - 2);
}

#[test]
fn entries_can_end_with_a_space_and_a_single_line_end() {
    let input = b"xref\n0 2\n0000000000 65535 f \n0000000017 00000 n\r\ntrailer << >>";

    let parsed = parse(input);

    assert_eq!(parsed.table.get(1), Some(in_use(17, 0)));
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn a_table_can_have_several_subsections() {
    let input = b"xref\n\
0 1\n\
0000000000 65535 f \n\
3 1\n\
0000025325 00000 n \n\
23 2\n\
0000025518 00002 n \n\
0000025635 00000 n \n\
trailer\n<< /Size 25 >>";

    let table = parse(input).table;

    assert_eq!(table.len(), 4);
    assert_eq!(table.get(3), Some(in_use(25325, 0)));
    assert_eq!(table.get(23), Some(in_use(25518, 2)));
    assert_eq!(table.get(24), Some(in_use(25635, 0)));
    assert_eq!(table.get(1), None);
}

#[test]
fn an_empty_subsection_is_valid() {
    let parsed = parse(b"xref\n0 0\ntrailer\n<< >>");

    assert!(parsed.table.is_empty());
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn the_table_is_read_at_the_given_offset() {
    let mut input = b"%PDF-1.7\n".to_vec();
    let offset = input.len();
    input.extend_from_slice(STANDARD);

    assert_eq!(parse_at(&input, offset).table.len(), 6);
}

#[test]
fn whitespace_before_xref_is_skipped() {
    assert_eq!(parse(b"\r\n  xref\n0 0\ntrailer<<>>").table.len(), 0);
}

// Tables that are read with a diagnostic

#[test]
fn entries_that_are_not_20_bytes_are_read_and_reported_once() {
    let input = b"xref\n0 3\n0000000000 65535 f\n17 0 n\n0000000081 00000 n\ntrailer << >>";

    let parsed = parse(input);

    assert_eq!(parsed.table.get(1), Some(in_use(17, 0)));
    assert_eq!(parsed.table.get(2), Some(in_use(81, 0)));
    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::NonStandardXrefEntry, 9)]
    );
}

#[test]
fn a_subsection_with_fewer_entries_than_stated_keeps_them() {
    let input = b"xref\n0 3\n0000000000 65535 f \n0000000017 00000 n \ntrailer << >>";

    let parsed = parse(input);

    assert_eq!(parsed.table.len(), 2);
    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::XrefSubsectionTooShort, 5)]
    );
}

#[test]
fn a_repeated_object_number_keeps_the_first_entry() {
    let input = b"xref\n1 1\n0000000017 00000 n \n1 1\n0000000099 00000 n \ntrailer << >>";

    let parsed = parse(input);

    assert_eq!(parsed.table.get(1), Some(in_use(17, 0)));
    assert_eq!(
        diagnostics(&parsed),
        [(
            DiagnosticKind::DuplicateXrefEntry,
            position(input, b"0000000099")
        )]
    );
}

#[test]
fn diagnostics_of_the_trailer_are_kept() {
    let input = b"xref\n0 0\ntrailer << /Size 1 /Size 2 >>";

    let parsed = parse(input);

    assert_eq!(
        parsed.table.trailer().get(b"Size"),
        Some(&Object::Integer(2))
    );
    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::DuplicateDictionaryKey, 28)]
    );
}

// Errors

#[test]
fn a_table_must_start_with_xref() {
    assert_eq!(error(b"1 0 obj"), (ErrorKind::NotAnXrefTable, 0));
    assert_eq!(error(b"xrefs\n0 0\n"), (ErrorKind::NotAnXrefTable, 0));
    assert_eq!(error(b""), (ErrorKind::NotAnXrefTable, 0));
}

#[test]
fn an_offset_past_the_end_is_an_error() {
    let error = parse_xref_table(b"xref", 100, Limits::default()).expect_err("should fail");

    assert_eq!(error.kind(), ErrorKind::NotAnXrefTable);
}

#[test]
fn a_table_must_end_with_a_trailer() {
    assert_eq!(error(b"xref\n0 0\n"), (ErrorKind::MissingTrailer, 9));
    assert_eq!(error(b"xref\n0 0\n<< >>"), (ErrorKind::MissingTrailer, 9));
}

#[test]
fn the_trailer_must_be_a_dictionary() {
    assert_eq!(
        error(b"xref\n0 0\ntrailer\n[1 2]"),
        (ErrorKind::InvalidTrailer, 9)
    );
    assert_eq!(
        error(b"xref\n0 0\ntrailer\n"),
        (ErrorKind::InvalidTrailer, 9)
    );
}

#[test]
fn a_damaged_trailer_dictionary_is_an_error() {
    assert_eq!(
        error(b"xref\n0 0\ntrailer\n<< /Size"),
        (ErrorKind::UnexpectedEnd, 25)
    );
}

#[test]
fn subsection_headers_must_be_two_integers() {
    assert_eq!(
        error(b"xref\n0\n0000000000 65535 f \ntrailer"),
        (ErrorKind::InvalidXrefSubsection, 5)
    );
    assert_eq!(
        error(b"xref\n0 x\ntrailer"),
        (ErrorKind::InvalidXrefSubsection, 5)
    );
}

#[test]
fn object_numbers_must_fit_in_32_bits() {
    assert_eq!(
        error(b"xref\n4294967296 1\n"),
        (ErrorKind::InvalidXrefSubsection, 5)
    );
    assert_eq!(
        error(b"xref\n4294967295 2\n"),
        (ErrorKind::InvalidXrefSubsection, 5)
    );
    assert_eq!(
        error(b"xref\n99999999999999999999999 1\n"),
        (ErrorKind::InvalidXrefSubsection, 5)
    );
    let parsed = parse(b"xref\n4294967295 1\n0000000017 00000 n \ntrailer<<>>");
    assert_eq!(parsed.table.get(u32::MAX), Some(in_use(17, 0)));
}

#[test]
fn entries_must_have_an_offset_a_generation_and_a_type() {
    for entry in [
        &b"0000000017 00000 x \n"[..],
        b"0000000017 00000 \n",
        b"0000000017 n \n",
        b"0000000017 00000 nn\n",
        b"000000001700000 n \n",
        b"0000000017 00000n \n",
        b"0000000017\n00000 n \n",
    ] {
        let mut input = b"xref\n0 1\n".to_vec();
        input.extend_from_slice(entry);
        input.extend_from_slice(b"trailer << >>");

        assert_eq!(
            error(&input),
            (ErrorKind::InvalidXrefEntry, 9),
            "{}",
            String::from_utf8_lossy(entry)
        );
    }
}

#[test]
fn generations_must_fit_in_16_bits() {
    assert_eq!(
        error(b"xref\n0 1\n0000000000 65536 f \ntrailer << >>"),
        (ErrorKind::InvalidXrefEntry, 9)
    );
}

#[test]
fn a_huge_count_with_no_entries_fails_quickly() {
    assert_eq!(
        error(b"xref\n0 4294967295\n0000000000 65535 f \n"),
        (ErrorKind::InvalidXrefEntry, 38)
    );
}
