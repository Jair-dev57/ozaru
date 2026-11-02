use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::syntax::{Object, ObjectId, ParsedIndirectObject, Stream, parse_indirect_object};

fn parse_at(input: &[u8], offset: usize) -> ParsedIndirectObject {
    parse_indirect_object(input, offset, Limits::default()).expect("input should be valid")
}

fn parse(input: &[u8]) -> ParsedIndirectObject {
    parse_at(input, 0)
}

fn stream(parsed: &ParsedIndirectObject) -> &Stream {
    parsed
        .object
        .object
        .as_stream()
        .expect("object should be a stream")
}

fn data<'a>(parsed: &ParsedIndirectObject, input: &'a [u8]) -> &'a [u8] {
    stream(parsed)
        .raw_data(input)
        .expect("data should be in the file")
}

fn diagnostics(parsed: &ParsedIndirectObject) -> Vec<DiagnosticKind> {
    parsed.diagnostics.iter().map(|d| d.kind()).collect()
}

// Valid streams

#[test]
fn a_stream_with_the_right_length() {
    let input = b"5 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj";
    let parsed = parse(input);

    assert_eq!(parsed.object.id, ObjectId::new(5, 0));
    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(
        stream(&parsed).dictionary().get("Length"),
        Some(&Object::Integer(5))
    );
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.end, input.len());
}

#[test]
fn the_end_of_line_after_stream_can_be_cr_lf() {
    let input = b"5 0 obj << /Length 5 >> stream\r\nHello\r\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn endstream_can_follow_the_data_without_an_end_of_line() {
    let input = b"5 0 obj << /Length 5 >> stream\nHelloendstream endobj";

    assert_eq!(data(&parse(input), input), b"Hello");
}

#[test]
fn an_empty_stream() {
    let input = b"5 0 obj << /Length 0 >> stream\n\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"");
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn binary_data_is_never_parsed() {
    let mut input = b"5 0 obj << /Length 8 >> stream\n".to_vec();
    input.extend_from_slice(b"\x00)\xff(<</\x80");
    input.extend_from_slice(b"\nendstream endobj");
    let parsed = parse(&input);

    assert_eq!(data(&parsed, &input), b"\x00)\xff(<</\x80");
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn the_length_wins_over_endstream_inside_the_data() {
    let input = b"5 0 obj << /Length 13 >> stream\nendstream ABC\nendstream endobj";

    assert_eq!(data(&parse(input), input), b"endstream ABC");
}

#[test]
fn streams_can_be_read_from_an_offset() {
    let input = b"1 0 obj null endobj\n2 0 obj << /Length 2 >> stream\nhi\nendstream\nendobj";
    let first = parse(input);
    let second = parse_at(input, first.end);

    assert_eq!(second.object.id, ObjectId::new(2, 0));
    assert_eq!(data(&second, input), b"hi");
    assert_eq!(second.end, input.len());
}

#[test]
fn the_data_range_is_counted_from_the_start_of_the_file() {
    let input = b"5 0 obj << /Length 2 >> stream\nhi\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(stream(&parsed).data_range(), 31..33);
}

// Wrong or missing lengths

#[test]
fn a_length_that_is_too_long_falls_back_to_endstream() {
    let input = b"5 0 obj << /Length 50 >> stream\nHello\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(diagnostics(&parsed), [DiagnosticKind::WrongStreamLength]);
}

#[test]
fn a_length_that_is_too_short_falls_back_to_endstream() {
    let input = b"5 0 obj << /Length 2 >> stream\r\nHello\r\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(diagnostics(&parsed), [DiagnosticKind::WrongStreamLength]);
}

#[test]
fn a_negative_or_non_integer_length_falls_back_to_endstream() {
    for length in ["-5", "5.0", "/Five", "(5)"] {
        let input = format!("5 0 obj << /Length {length} >> stream\nHello\nendstream endobj");
        let parsed = parse(input.as_bytes());

        assert_eq!(data(&parsed, input.as_bytes()), b"Hello", "{length}");
        assert_eq!(
            diagnostics(&parsed),
            [DiagnosticKind::WrongStreamLength],
            "{length}"
        );
    }
}

#[test]
fn a_missing_length_falls_back_to_endstream() {
    let input = b"5 0 obj << /Filter /FlateDecode >> stream\nHello\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(diagnostics(&parsed), [DiagnosticKind::MissingStreamLength]);
}

#[test]
fn an_indirect_length_falls_back_to_endstream_for_now() {
    let input = b"5 0 obj << /Length 6 0 R >> stream\nHello\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(
        diagnostics(&parsed),
        [DiagnosticKind::UnresolvedStreamLength]
    );
}

// End of line after the stream keyword

#[test]
fn a_lone_carriage_return_after_stream_is_accepted() {
    let input = b"5 0 obj << /Length 5 >> stream\rHello\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(
        diagnostics(&parsed),
        [DiagnosticKind::StreamLineEndIsCarriageReturn]
    );
}

#[test]
fn a_missing_end_of_line_after_stream_is_accepted() {
    let input = b"5 0 obj << /Length 3 >> stream[1]\nendstream endobj";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"[1]");
    assert_eq!(diagnostics(&parsed), [DiagnosticKind::MissingStreamLineEnd]);
}

// endobj and errors

#[test]
fn a_missing_endobj_after_endstream_is_a_diagnostic() {
    let input = b"5 0 obj << /Length 5 >> stream\nHello\nendstream";
    let parsed = parse(input);

    assert_eq!(data(&parsed, input), b"Hello");
    assert_eq!(diagnostics(&parsed), [DiagnosticKind::MissingEndobj]);
    assert_eq!(parsed.end, input.len());
}

#[test]
fn diagnostics_from_the_dictionary_and_the_stream_are_all_kept() {
    let input = b"5 0 obj << /Length 9 /Length 50 >> stream\nHello\nendstream";
    let parsed = parse(input);

    assert_eq!(
        diagnostics(&parsed),
        [
            DiagnosticKind::DuplicateDictionaryKey,
            DiagnosticKind::WrongStreamLength,
            DiagnosticKind::MissingEndobj
        ]
    );
}

#[test]
fn a_stream_without_endstream_is_an_error() {
    let input = b"5 0 obj << /Length 50 >> stream\nHello";
    let error = parse_indirect_object(input, 0, Limits::default()).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::MissingEndstream, 32)
    );
}

#[test]
fn only_a_dictionary_can_have_a_stream() {
    let input = b"5 0 obj [1 2] stream\nHello\nendstream endobj";
    let error = parse_indirect_object(input, 0, Limits::default()).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::UnexpectedToken, 14)
    );
}
