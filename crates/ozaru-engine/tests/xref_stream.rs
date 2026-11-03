use miniz_oxide::deflate::compress_to_vec_zlib;
use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::{ParsedXrefTable, XrefEntry, parse_xref_stream};

// An indirect object `7 0 obj` holding a cross-reference stream with `data`.
fn xref_stream(entries: &str, data: &[u8]) -> Vec<u8> {
    let mut input = format!(
        "7 0 obj\n<< /Type /XRef {entries} /Length {} >>\nstream\n",
        data.len()
    )
    .into_bytes();
    input.extend_from_slice(data);
    input.extend_from_slice(b"\nendstream\nendobj\n");
    input
}

fn parse(input: &[u8]) -> ParsedXrefTable {
    parse_xref_stream(input, 0, Limits::default()).expect("input should be valid")
}

fn error(input: &[u8]) -> (ErrorKind, usize) {
    let error = parse_xref_stream(input, 0, Limits::default()).expect_err("should fail");
    (error.kind(), error.offset())
}

fn diagnostics(parsed: &ParsedXrefTable) -> Vec<(DiagnosticKind, usize)> {
    parsed
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

fn in_use(offset: usize, generation: u16) -> XrefEntry {
    XrefEntry::InUse { offset, generation }
}

fn compressed(stream: u32, index: u32) -> XrefEntry {
    XrefEntry::Compressed { stream, index }
}

fn free(generation: u16) -> XrefEntry {
    XrefEntry::Free { generation }
}

// /W [1 2 1]: object 0 free, 1 at offset 15, 2 inside object stream 3 at index 0,
// and 3, the object stream, at offset 331.
const ROWS: [[u8; 4]; 4] = [[0, 0, 0, 255], [1, 0, 15, 0], [2, 0, 3, 0], [1, 1, 75, 0]];

// Valid streams

#[test]
fn the_three_entry_types_are_read() {
    let input = xref_stream("/Size 4 /W [1 2 1]", ROWS.as_flattened());

    let parsed = parse(&input);

    assert_eq!(
        parsed.table.iter().collect::<Vec<_>>(),
        [
            (0, free(255)),
            (1, in_use(15, 0)),
            (2, compressed(3, 0)),
            (3, in_use(331, 0)),
        ]
    );
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.end, input.len() - 1);
}

#[test]
fn the_stream_dictionary_is_the_trailer() {
    let input = xref_stream("/Size 4 /Root 1 0 R /W [1 2 1]", ROWS.as_flattened());

    let parsed = parse(&input);
    let trailer = parsed.table.trailer();

    assert_eq!(
        trailer.get("Root"),
        Some(&Object::Reference(ObjectId::new(1, 0)))
    );
    assert_eq!(trailer.get("Size"), Some(&Object::Integer(4)));
}

#[test]
fn compressed_data_with_the_up_predictor_is_read() {
    // How most writers store them: Flate with PNG Up, one row per entry.
    let mut predicted = Vec::new();
    let mut above = [0u8; 4];
    for row in ROWS {
        predicted.push(2);
        predicted.extend(
            row.iter()
                .zip(above)
                .map(|(byte, up)| byte.wrapping_sub(up)),
        );
        above = row;
    }
    let data = compress_to_vec_zlib(&predicted, 6);
    let input = xref_stream(
        "/Size 4 /W [1 2 1] /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 4 >>",
        &data,
    );

    let parsed = parse(&input);

    assert_eq!(parsed.table.get(2), Some(compressed(3, 0)));
    assert_eq!(parsed.table.get(3), Some(in_use(331, 0)));
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn filters_can_be_given_as_arrays() {
    let data = compress_to_vec_zlib(ROWS.as_flattened(), 6);
    let input = xref_stream(
        "/Size 4 /W [1 2 1] /Filter [/FlateDecode] /DecodeParms [null]",
        &data,
    );

    assert_eq!(parse(&input).table.len(), 4);
}

#[test]
fn the_index_lists_the_object_numbers() {
    let input = xref_stream("/Size 30 /Index [0 1 20 3] /W [1 2 1]", ROWS.as_flattened());

    let table = parse(&input).table;

    assert_eq!(table.get(0), Some(free(255)));
    assert_eq!(table.get(20), Some(in_use(15, 0)));
    assert_eq!(table.get(21), Some(compressed(3, 0)));
    assert_eq!(table.get(22), Some(in_use(331, 0)));
    assert_eq!(table.get(1), None);
}

#[test]
fn fields_of_width_zero_take_their_defaults() {
    // No type field means type 1, no third field means generation 0.
    let input = xref_stream("/Size 2 /W [0 2 0]", &[0, 15, 1, 75]);

    let table = parse(&input).table;

    assert_eq!(table.get(0), Some(in_use(15, 0)));
    assert_eq!(table.get(1), Some(in_use(331, 0)));
}

#[test]
fn wide_fields_are_read_big_endian() {
    let input = xref_stream("/Size 1 /W [1 8 2]", &[1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 7]);

    assert_eq!(parse(&input).table.get(0), Some(in_use(65536, 7)));
}

#[test]
fn bytes_after_the_last_entry_are_ignored() {
    let mut data = ROWS.as_flattened().to_vec();
    data.extend_from_slice(&[9, 9]);

    let parsed = parse(&xref_stream("/Size 4 /W [1 2 1]", &data));

    assert_eq!(parsed.table.len(), 4);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn the_stream_is_read_at_the_given_offset() {
    let mut input = b"%PDF-1.7\n".to_vec();
    input.extend(xref_stream("/Size 4 /W [1 2 1]", ROWS.as_flattened()));

    let parsed = parse_xref_stream(&input, 9, Limits::default()).unwrap();

    assert_eq!(parsed.table.len(), 4);
}

// Streams that are read with a diagnostic

#[test]
fn a_stream_with_fewer_entries_than_its_index_keeps_them() {
    let input = xref_stream("/Size 6 /W [1 2 1]", &ROWS.as_flattened()[..14]);

    let parsed = parse(&input);

    assert_eq!(parsed.table.len(), 3);
    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::XrefStreamTooShort, 0)]
    );
}

#[test]
fn entries_of_unknown_types_are_ignored_and_reported_once() {
    let input = xref_stream("/Size 3 /W [1 2 1]", &[3, 0, 15, 0, 1, 0, 9, 0, 7, 0, 0, 0]);

    let parsed = parse(&input);

    assert_eq!(parsed.table.iter().collect::<Vec<_>>(), [(1, in_use(9, 0))]);
    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::UnknownXrefEntryType, 0)]
    );
}

#[test]
fn an_object_number_in_two_subsections_keeps_the_first_entry() {
    let input = xref_stream(
        "/Size 2 /Index [1 1 1 1] /W [1 2 1]",
        &[1, 0, 15, 0, 1, 0, 99, 0],
    );

    let parsed = parse(&input);

    assert_eq!(parsed.table.get(1), Some(in_use(15, 0)));
    assert_eq!(
        diagnostics(&parsed),
        [(DiagnosticKind::DuplicateXrefEntry, 0)]
    );
}

#[test]
fn damaged_compressed_data_keeps_the_entries_before_the_damage() {
    let rows: Vec<u8> = (0..2000u32)
        .flat_map(|i| [1, (i >> 8) as u8, i as u8, 0])
        .collect();
    let data = compress_to_vec_zlib(&rows, 6);
    let input = xref_stream(
        "/Size 2000 /W [1 2 1] /Filter /FlateDecode",
        &data[..data.len() / 2],
    );

    let parsed = parse(&input);
    let kinds: Vec<_> = parsed.diagnostics.iter().map(|d| d.kind()).collect();

    assert!(parsed.table.len() < 2000);
    assert_eq!(parsed.table.get(1), Some(in_use(1, 0)));
    assert_eq!(
        kinds,
        [
            DiagnosticKind::DamagedFlateData,
            DiagnosticKind::XrefStreamTooShort
        ]
    );
}

// Errors

#[test]
fn the_object_must_be_a_stream_of_type_xref() {
    assert_eq!(
        error(b"7 0 obj << /Type /XRef >> endobj"),
        (ErrorKind::NotAnXrefStream, 0)
    );
    assert_eq!(
        error(b"7 0 obj << /Length 0 >> stream\n\nendstream endobj"),
        (ErrorKind::NotAnXrefStream, 0)
    );
    assert_eq!(
        error(b"7 0 obj << /Type /ObjStm /Length 0 >> stream\n\nendstream endobj"),
        (ErrorKind::NotAnXrefStream, 0)
    );
}

#[test]
fn errors_of_the_indirect_object_are_kept() {
    assert_eq!(error(b"xref\n0 0\n"), (ErrorKind::InvalidObjectHeader, 0));
}

#[test]
fn errors_are_reported_at_the_stream_offset() {
    let mut input = b"%PDF-1.7\n".to_vec();
    input.extend(xref_stream("/Size 4", ROWS.as_flattened()));

    let error = parse_xref_stream(&input, 9, Limits::default()).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::InvalidXrefStream, 9)
    );
}

#[test]
fn the_widths_must_be_three_small_integers() {
    for entries in [
        "/Size 4",
        "/Size 4 /W 4",
        "/Size 4 /W [1 2]",
        "/Size 4 /W [1 2 1 1]",
        "/Size 4 /W [1 -2 1]",
        "/Size 4 /W [1 9 1]",
        "/Size 4 /W [1 2.0 1]",
        "/Size 4 /W [0 0 0]",
    ] {
        let input = xref_stream(entries, ROWS.as_flattened());

        assert_eq!(
            error(&input),
            (ErrorKind::InvalidXrefStream, 0),
            "{entries}"
        );
    }
}

#[test]
fn the_size_and_index_must_be_valid() {
    for entries in [
        "/W [1 2 1]",
        "/Size -1 /W [1 2 1]",
        "/Size 4 /Index [0] /W [1 2 1]",
        "/Size 4 /Index [0 -1] /W [1 2 1]",
        "/Size 4 /Index 0 /W [1 2 1]",
        "/Size 4 /Index [4294967295 2] /W [1 2 1]",
        "/Size 4294967297 /W [1 2 1]",
    ] {
        let input = xref_stream(entries, ROWS.as_flattened());

        assert_eq!(
            error(&input),
            (ErrorKind::InvalidXrefStream, 0),
            "{entries}"
        );
    }
}

#[test]
fn values_that_dont_fit_are_an_error() {
    // Generation 65536.
    let input = xref_stream("/Size 1 /W [1 1 3]", &[1, 15, 1, 0, 0]);
    assert_eq!(error(&input), (ErrorKind::InvalidXrefEntry, 0));

    // Object stream number 2^32.
    let input = xref_stream("/Size 1 /W [1 5 1]", &[2, 1, 0, 0, 0, 0, 0]);
    assert_eq!(error(&input), (ErrorKind::InvalidXrefEntry, 0));
}

#[test]
fn streams_in_ascii_hex_are_read() {
    let input = xref_stream("/Size 1 /W [1 2 1] /Filter /ASCIIHexDecode", b"01000F00>");

    assert_eq!(parse(&input).table.get(0), Some(in_use(15, 0)));
}

#[test]
fn other_filters_are_not_supported_yet() {
    let input = xref_stream("/Size 1 /W [1 2 1] /Filter /DCTDecode", b"01000F00");

    assert_eq!(error(&input).0, ErrorKind::UnsupportedFilter);
}
