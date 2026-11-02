use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::syntax::{
    Dictionary, IndirectObject, Name, Object, ObjectId, ParsedIndirectObject, PdfString,
    StringFormat, parse_indirect_object,
};

fn parse_at(input: &[u8], offset: usize) -> ParsedIndirectObject {
    parse_indirect_object(input, offset, Limits::default()).expect("input should be valid")
}

fn parse(input: &[u8]) -> ParsedIndirectObject {
    parse_at(input, 0)
}

fn error(input: &[u8]) -> (ErrorKind, usize) {
    let error = parse_indirect_object(input, 0, Limits::default()).expect_err("should fail");
    (error.kind(), error.offset())
}

fn diagnostics(parsed: &ParsedIndirectObject) -> Vec<(DiagnosticKind, usize)> {
    parsed
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

// The type

#[test]
fn indirect_objects_pair_a_number_with_an_object() {
    let indirect = IndirectObject::new(ObjectId::new(4, 0), Object::Name(Name::from("Page")));

    assert_eq!(indirect.id, ObjectId::new(4, 0));
    assert_eq!(indirect.object, Object::Name(Name::from("Page")));
}

#[test]
fn indirect_objects_with_different_generations_are_different() {
    assert_ne!(
        IndirectObject::new(ObjectId::new(4, 0), Object::Null),
        IndirectObject::new(ObjectId::new(4, 1), Object::Null)
    );
}

// Parsing

#[test]
fn the_example_from_the_standard() {
    let parsed = parse(b"12 0 obj\n(Brillig)\nendobj");

    assert_eq!(parsed.object.id, ObjectId::new(12, 0));
    assert_eq!(
        parsed.object.object,
        Object::String(PdfString::new("Brillig", StringFormat::Literal))
    );
    assert_eq!(parsed.end, 25);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn a_page_dictionary() {
    let parsed = parse(b"4 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n");
    let mut expected = Dictionary::new();
    expected.insert(Name::from("Type"), Object::Name(Name::from("Page")));
    expected.insert(Name::from("Parent"), Object::Reference(ObjectId::new(2, 0)));

    assert_eq!(parsed.object.id, ObjectId::new(4, 0));
    assert_eq!(parsed.object.object, Object::Dictionary(expected));
}

#[test]
fn numbers_and_references_as_the_object() {
    assert_eq!(
        parse(b"3 0 obj 12 endobj").object.object,
        Object::Integer(12)
    );
    assert_eq!(
        parse(b"3 0 obj 12 0 R endobj").object.object,
        Object::Reference(ObjectId::new(12, 0))
    );
}

#[test]
fn generations_other_than_zero() {
    assert_eq!(parse(b"7 2 obj null endobj").object.id, ObjectId::new(7, 2));
    assert_eq!(
        parse(b"7 65535 obj null endobj").object.id,
        ObjectId::new(7, 65535)
    );
}

#[test]
fn whitespace_and_comments_are_allowed_anywhere() {
    let parsed = parse(b"  % object\n 5\n0 % generation\n obj true endobj");

    assert_eq!(parsed.object.id, ObjectId::new(5, 0));
    assert_eq!(parsed.object.object, Object::Boolean(true));
}

#[test]
fn objects_can_be_read_from_an_offset() {
    let file = b"1 0 obj (first) endobj\n2 0 obj (second) endobj";
    let first = parse_at(file, 0);
    let second = parse_at(file, first.end);

    assert_eq!(first.object.id, ObjectId::new(1, 0));
    assert_eq!(second.object.id, ObjectId::new(2, 0));
    assert_eq!(
        second.object.object,
        Object::String(PdfString::new("second", StringFormat::Literal))
    );
    assert_eq!(second.end, file.len());
}

#[test]
fn offsets_are_counted_from_the_start_of_the_file() {
    let file = b"junk 9 0 obj << /A 1 /A 2 >> endobj";

    let parsed = parse_at(file, 5);

    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::DuplicateDictionaryKey, 21)]
    );
    assert_eq!(parsed.end, file.len());
}

// Missing endobj

#[test]
fn a_missing_endobj_at_the_end_of_the_file_is_a_diagnostic() {
    let parsed = parse(b"4 0 obj << /Type /Page >>");

    assert_eq!(parsed.object.id, ObjectId::new(4, 0));
    assert_eq!(diagnostics(&parsed), [(DiagnosticKind::MissingEndobj, 25)]);
    assert_eq!(parsed.end, 25);
}

#[test]
fn a_missing_endobj_before_the_next_object_is_a_diagnostic() {
    let file = b"4 0 obj (a)\n5 0 obj (b) endobj";

    let first = parse(file);
    let second = parse_at(file, first.end);

    assert_eq!(diagnostics(&first), [(DiagnosticKind::MissingEndobj, 11)]);
    assert_eq!(first.end, 11);
    assert_eq!(second.object.id, ObjectId::new(5, 0));
}

#[test]
fn diagnostics_from_the_object_are_kept() {
    let parsed = parse(b"1 0 obj << /A 1 /A 2 >>");

    assert_eq!(
        diagnostics(&parsed),
        [
            (DiagnosticKind::DuplicateDictionaryKey, 16),
            (DiagnosticKind::MissingEndobj, 23)
        ]
    );
}

// Errors

#[test]
fn headers_that_are_not_n_g_obj_are_an_error() {
    assert_eq!(
        error(b"4 obj null endobj"),
        (ErrorKind::InvalidObjectHeader, 2)
    );
    assert_eq!(error(b"4 0 R"), (ErrorKind::InvalidObjectHeader, 4));
    assert_eq!(error(b"(x) 0 obj"), (ErrorKind::InvalidObjectHeader, 0));
    assert_eq!(error(b"4 0.5 obj"), (ErrorKind::InvalidObjectHeader, 2));
}

#[test]
fn object_numbers_out_of_range_are_an_error() {
    assert_eq!(
        error(b"-1 0 obj null endobj"),
        (ErrorKind::InvalidObjectHeader, 0)
    );
    assert_eq!(
        error(b"4294967296 0 obj null"),
        (ErrorKind::InvalidObjectHeader, 0)
    );
    assert_eq!(
        error(b"4 65536 obj null"),
        (ErrorKind::InvalidObjectHeader, 2)
    );
}

#[test]
fn an_object_without_a_value_is_an_error() {
    assert_eq!(error(b"4 0 obj endobj"), (ErrorKind::UnexpectedToken, 8));
    assert_eq!(error(b"4 0 obj"), (ErrorKind::UnexpectedEnd, 7));
}

#[test]
fn an_incomplete_header_is_an_error() {
    assert_eq!(error(b""), (ErrorKind::UnexpectedEnd, 0));
    assert_eq!(error(b"4"), (ErrorKind::UnexpectedEnd, 1));
    assert_eq!(error(b"4 0"), (ErrorKind::UnexpectedEnd, 3));
}

#[test]
fn an_offset_past_the_end_is_an_error() {
    let error = parse_indirect_object(b"1 0 obj null endobj", 100, Limits::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEnd);
}

#[test]
fn streams_are_not_read_yet() {
    assert_eq!(
        error(b"4 0 obj << /Length 0 >> stream\n\nendstream endobj"),
        (ErrorKind::UnexpectedToken, 24)
    );
}
