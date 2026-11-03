use miniz_oxide::deflate::compress_to_vec_zlib;
use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::syntax::{Name, Object, ObjectId, Stream, parse_indirect_object};
use ozaru_engine::xref::{ParsedObjectStream, parse_object_stream};

// A file holding `5 0 obj`, an object stream with `entries` and `data`. Returns
// the file and the stream.
fn object_stream(entries: &str, data: &[u8]) -> (Vec<u8>, Stream) {
    let mut file = format!(
        "5 0 obj\n<< /Type /ObjStm {entries} /Length {} >>\nstream\n",
        data.len()
    )
    .into_bytes();
    file.extend_from_slice(data);
    file.extend_from_slice(b"\nendstream\nendobj\n");
    let parsed = parse_indirect_object(&file, 0, Limits::default()).unwrap();
    let Object::Stream(stream) = parsed.object.object else {
        panic!("not a stream");
    };
    (file, stream)
}

fn parse(entries: &str, data: &[u8]) -> ParsedObjectStream {
    let (file, stream) = object_stream(entries, data);
    parse_object_stream(&stream, &file, Limits::default()).expect("stream should be valid")
}

fn error(entries: &str, data: &[u8]) -> (ErrorKind, usize) {
    let (file, stream) = object_stream(entries, data);
    let error = parse_object_stream(&stream, &file, Limits::default()).expect_err("should fail");
    (error.kind(), error.offset())
}

// Where the stream data starts in the files above, for errors and diagnostics.
fn data_start(entries: &str, data: &[u8]) -> usize {
    object_stream(entries, data).1.data_range().start
}

// Objects 11, 12 and 13 at offsets 0, 7 and 13 after /First.
const HEADER: &str = "11 0 12 7 13 13 ";
const OBJECTS: &str = "(text) /Name [1 2 R]";

fn data() -> Vec<u8> {
    format!("{HEADER}{OBJECTS}").into_bytes()
}

fn entries() -> String {
    format!("/N 3 /First {}", HEADER.len())
}

// Valid streams

#[test]
fn objects_are_read_by_index() {
    let parsed = parse(&entries(), &data());
    let stream = &parsed.stream;

    assert_eq!(stream.len(), 3);
    let objects: Vec<_> = (0..3).map(|i| stream.object(i).unwrap().object).collect();
    assert_eq!(objects[0].id, ObjectId::new(11, 0));
    assert_eq!(objects[0].object.as_string().unwrap().as_bytes(), b"text");
    assert_eq!(objects[1].id, ObjectId::new(12, 0));
    assert_eq!(objects[1].object, Object::Name(Name::from("Name")));
    assert_eq!(objects[2].id, ObjectId::new(13, 0));
    assert_eq!(
        objects[2].object,
        Object::Array(vec![
            Object::Integer(1),
            Object::Integer(2),
            Object::Name(Name::from("R"))
        ])
        .as_array()
        .map(|_| objects[2].object.clone())
        .unwrap()
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn numbers_and_indexes_can_be_looked_up() {
    let stream = parse(&entries(), &data()).stream;

    assert_eq!(stream.number(1), Some(12));
    assert_eq!(stream.number(3), None);
    assert_eq!(stream.index_of(13), Some(2));
    assert_eq!(stream.index_of(99), None);
}

#[test]
fn compressed_streams_are_decoded() {
    let data = compress_to_vec_zlib(&data(), 6);
    let parsed = parse(&format!("{} /Filter /FlateDecode", entries()), &data);

    let object = parsed.stream.object(1).unwrap().object;

    assert_eq!(object.object, Object::Name(Name::from("Name")));
}

#[test]
fn objects_can_be_listed_in_any_order() {
    let header = "21 11 20 0 ";
    let data = format!("{header}<< /A 1 >> (second)");
    let stream = parse(&format!("/N 2 /First {}", header.len()), data.as_bytes()).stream;

    let first = stream.object(0).unwrap().object;

    assert_eq!(first.id, ObjectId::new(21, 0));
    assert_eq!(first.object.as_string().unwrap().as_bytes(), b"second");
    assert!(
        stream
            .object(1)
            .unwrap()
            .object
            .object
            .as_dictionary()
            .is_some()
    );
}

#[test]
fn an_empty_object_stream_is_valid() {
    let parsed = parse("/N 0 /First 0", b"");

    assert!(parsed.stream.is_empty());
    assert!(parsed.diagnostics.is_empty());
}

// Streams that are read with a diagnostic

#[test]
fn a_header_shorter_than_n_keeps_the_objects_listed() {
    let entries = format!("/N 5 /First {}", HEADER.len());

    let parsed = parse(&entries, &data());

    assert_eq!(parsed.stream.len(), 3);
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|d| (d.kind(), d.offset()))
            .collect::<Vec<_>>(),
        [(
            DiagnosticKind::ObjectStreamHeaderTooShort,
            data_start(&entries, &data())
        )]
    );
}

#[test]
fn diagnostics_of_an_object_are_reported_at_the_stream() {
    let header = "1 0 ";
    let data = format!("{header}<< /A 1 /A 2 >>");
    let entries = format!("/N 1 /First {}", header.len());

    let object = parse(&entries, data.as_bytes()).stream.object(0).unwrap();

    assert_eq!(
        object
            .diagnostics
            .iter()
            .map(|d| (d.kind(), d.offset()))
            .collect::<Vec<_>>(),
        [(
            DiagnosticKind::DuplicateDictionaryKey,
            data_start(&entries, data.as_bytes())
        )]
    );
}

// Errors

#[test]
fn the_stream_must_be_of_type_objstm() {
    let file =
        b"5 0 obj << /Type /XRef /N 0 /First 0 /Length 0 >> stream\n\nendstream endobj".to_vec();
    let parsed = parse_indirect_object(&file, 0, Limits::default()).unwrap();
    let stream = parsed.object.object.as_stream().unwrap().clone();

    let error = parse_object_stream(&stream, &file, Limits::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::NotAnObjectStream);
}

#[test]
fn n_and_first_must_be_valid() {
    for entries in [
        "/First 16",
        "/N 3",
        "/N -1 /First 16",
        "/N 3 /First -1",
        "/N 3 /First /Sixteen",
        "/N 3 /First 1000",
    ] {
        assert_eq!(
            error(entries, &data()),
            (ErrorKind::InvalidObjectStream, data_start(entries, &data())),
            "{entries}"
        );
    }
}

#[test]
fn the_header_must_be_pairs_of_non_negative_integers() {
    for header in [
        "11 0 /Name 7 ",
        "11 0 12 -7 ",
        "-11 0 ",
        "11 (zero) ",
        "11 0 12 1.5 ",
    ] {
        let data = format!("{header}(object)");
        let entries = format!("/N 2 /First {}", header.len());

        assert_eq!(
            error(&entries, data.as_bytes()).0,
            ErrorKind::InvalidObjectStream,
            "{header}"
        );
    }
}

#[test]
fn asking_for_a_missing_object_is_an_error() {
    let stream = parse(&entries(), &data()).stream;

    for index in [3, u32::MAX] {
        assert_eq!(
            stream.object(index).unwrap_err().kind(),
            ErrorKind::InvalidObjectStream
        );
    }
}

#[test]
fn an_offset_past_the_data_is_an_error() {
    let header = "1 0 2 999 3 9223372036854775807 ";
    let data = format!("{header}(one)");
    let stream = parse(&format!("/N 3 /First {}", header.len()), data.as_bytes()).stream;

    assert!(stream.object(0).is_ok());
    assert_eq!(
        stream.object(1).unwrap_err().kind(),
        ErrorKind::InvalidObjectStream
    );
    assert_eq!(
        stream.object(2).unwrap_err().kind(),
        ErrorKind::InvalidObjectStream
    );
}

#[test]
fn a_damaged_object_is_an_error_at_the_stream() {
    let header = "1 0 2 3 ";
    let data = format!("{header}(a) <<");
    let entries = format!("/N 2 /First {}", header.len());
    let stream = parse(&entries, data.as_bytes()).stream;

    let error = stream.object(1).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (
            ErrorKind::UnexpectedEnd,
            data_start(&entries, data.as_bytes())
        )
    );
}

#[test]
fn an_offset_at_the_end_of_the_data_is_an_error() {
    let header = "1 0 ";
    let stream = parse(&format!("/N 1 /First {}", header.len()), header.as_bytes()).stream;

    assert_eq!(
        stream.object(0).unwrap_err().kind(),
        ErrorKind::UnexpectedEnd
    );
}
