use std::sync::Arc;

use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::syntax::{Name, Object, ObjectId};
use ozaru_engine::xref::{Resolver, XrefEntry};

mod common;
use common::Pdf;

fn id(number: u32) -> ObjectId {
    ObjectId::new(number, 0)
}

fn in_use(offset: usize) -> XrefEntry {
    XrefEntry::InUse {
        offset,
        generation: 0,
    }
}

fn compressed(stream: u32, index: u32) -> XrefEntry {
    XrefEntry::Compressed { stream, index }
}

fn diagnostics(resolver: &Resolver<'_>) -> Vec<(DiagnosticKind, usize)> {
    resolver
        .diagnostics()
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

fn error_kind(resolver: &mut Resolver<'_>, number: u32) -> ErrorKind {
    resolver
        .resolve(id(number))
        .expect_err("should fail")
        .kind()
}

// A file whose objects are all listed in one table. `entries` are added to the
// ones of `objects`, to write broken entries.
fn file(objects: &[(u32, &str)], entries: &[(u32, XrefEntry)]) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut table: Vec<(u32, XrefEntry)> = objects
        .iter()
        .map(|&(number, body)| (number, in_use(pdf.object(number, body))))
        .collect();
    table.extend_from_slice(entries);
    let start = pdf.table(&table, "/Size 20 /Root 1 0 R");
    pdf.end(start);
    pdf.file
}

// A file with objects 1 and 2 loose, and 3 and 4 in object stream 10.
fn file_with_object_stream() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    let two = pdf.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
    let ten = pdf.object_stream(10, &[(3, "(three)"), (4, "<< /Four 4 >>")]);
    let start = pdf.xref_stream(
        11,
        &[
            (1, in_use(one)),
            (2, in_use(two)),
            (3, compressed(10, 0)),
            (4, compressed(10, 1)),
            (10, in_use(ten)),
        ],
        "/Size 12 /Root 1 0 R",
    );
    pdf.end(start);
    pdf.file
}

// Objects in the file

#[test]
fn a_reference_resolves_to_its_object() {
    let file = file(&[(1, "<< /Type /Catalog >>"), (2, "42")], &[]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(*resolver.resolve(id(2)).unwrap(), Object::Integer(42));
    let catalog = resolver.resolve(id(1)).unwrap();
    assert_eq!(
        catalog.as_dictionary().unwrap().get("Type"),
        Some(&Object::Name(Name::from("Catalog")))
    );
    assert!(resolver.diagnostics().is_empty());
}

#[test]
fn objects_are_read_once_and_cached() {
    let file = file(&[(1, "[1 2 3]")], &[]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    let first = resolver.resolve(id(1)).unwrap();
    let second = resolver.resolve(id(1)).unwrap();

    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn the_trailer_gives_the_root() {
    let file = file(&[(1, "<< /Type /Catalog >>")], &[]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    let root = resolver
        .trailer()
        .get("Root")
        .unwrap()
        .as_reference()
        .unwrap();

    assert!(resolver.resolve(root).unwrap().as_dictionary().is_some());
    assert_eq!(resolver.table().len(), 1);
}

#[test]
fn missing_and_free_objects_resolve_to_null() {
    let file = file(&[(1, "1")], &[(2, XrefEntry::Free { generation: 1 })]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(*resolver.resolve(id(2)).unwrap(), Object::Null);
    assert_eq!(*resolver.resolve(id(9)).unwrap(), Object::Null);
    assert!(resolver.diagnostics().is_empty());
}

#[test]
fn a_reference_with_another_generation_resolves_to_null() {
    let file = file(&[(1, "1")], &[]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let offset = match resolver.table().get(1) {
        Some(XrefEntry::InUse { offset, .. }) => offset,
        entry => panic!("unexpected entry {entry:?}"),
    };

    let object = resolver.resolve(ObjectId::new(1, 3)).unwrap();

    assert_eq!(*object, Object::Null);
    assert_eq!(
        diagnostics(&resolver),
        [(DiagnosticKind::GenerationMismatch, offset)]
    );
}

#[test]
fn diagnostics_of_the_objects_are_kept_once() {
    let mut pdf = Pdf::new();
    let offset = pdf.offset();
    pdf.write("1 0 obj\n<< /A 1 /A 2 >>\nendobj\n");
    let start = pdf.table(&[(1, in_use(offset))], "/Size 2");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    resolver.resolve(id(1)).unwrap();
    resolver.resolve(id(1)).unwrap();

    assert_eq!(
        diagnostics(&resolver),
        [(DiagnosticKind::DuplicateDictionaryKey, offset + 16)]
    );
}

// Objects in object streams

#[test]
fn compressed_objects_are_read_from_their_object_stream() {
    let file = file_with_object_stream();
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    let three = resolver.resolve(id(3)).unwrap();
    let four = resolver.resolve(id(4)).unwrap();

    assert_eq!(three.as_string().unwrap().as_bytes(), b"three");
    assert_eq!(
        four.as_dictionary().unwrap().get("Four"),
        Some(&Object::Integer(4))
    );
    assert!(resolver.resolve(id(10)).unwrap().as_stream().is_some());
    assert!(resolver.diagnostics().is_empty());
}

#[test]
fn a_wrong_index_is_found_by_number() {
    let mut pdf = Pdf::new();
    let ten = pdf.object_stream(10, &[(3, "(three)"), (4, "(four)")]);
    let start = pdf.xref_stream(11, &[(4, compressed(10, 0)), (10, in_use(ten))], "/Size 12");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    let four = resolver.resolve(id(4)).unwrap();

    assert_eq!(four.as_string().unwrap().as_bytes(), b"four");
    assert_eq!(
        diagnostics(&resolver),
        [(DiagnosticKind::WrongObjectStreamIndex, ten)]
    );
}

#[test]
fn an_object_missing_from_its_object_stream_is_an_error() {
    let mut pdf = Pdf::new();
    let ten = pdf.object_stream(10, &[(3, "(three)")]);
    let start = pdf.xref_stream(11, &[(7, compressed(10, 0)), (10, in_use(ten))], "/Size 12");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    let error = resolver.resolve(id(7)).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::ObjectMismatch, ten)
    );
}

#[test]
fn the_container_of_compressed_objects_must_be_an_object_stream() {
    let mut pdf = Pdf::new();
    let ten = pdf.object(10, "<< /Type /ObjStm >>");
    let start = pdf.xref_stream(11, &[(3, compressed(10, 0)), (10, in_use(ten))], "/Size 12");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    assert_eq!(error_kind(&mut resolver, 3), ErrorKind::NotAnObjectStream);
}

// Errors

#[test]
fn an_offset_with_another_object_is_an_error() {
    let mut pdf = Pdf::new();
    let offset = pdf.object(5, "(five)");
    let start = pdf.table(&[(1, in_use(offset))], "/Size 6");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    let error = resolver.resolve(id(1)).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::ObjectMismatch, offset)
    );
}

#[test]
fn an_offset_without_an_object_is_an_error() {
    let file = file(&[(1, "1")], &[(2, in_use(3)), (3, in_use(1_000_000))]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(error_kind(&mut resolver, 2), ErrorKind::InvalidObjectHeader);
    assert_eq!(error_kind(&mut resolver, 3), ErrorKind::UnexpectedEnd);
}

#[test]
fn errors_are_cached_too() {
    let file = file(&[], &[(2, in_use(3))]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    let first = resolver.resolve(id(2)).unwrap_err();
    let second = resolver.resolve(id(2)).unwrap_err();

    assert_eq!(first, second);
}

#[test]
fn an_object_stream_stored_inside_itself_is_a_cycle() {
    let mut pdf = Pdf::new();
    let start = pdf.xref_stream(
        11,
        &[(3, compressed(10, 0)), (10, compressed(10, 1))],
        "/Size 12",
    );
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    assert_eq!(error_kind(&mut resolver, 3), ErrorKind::ReferenceCycle);
    assert_eq!(error_kind(&mut resolver, 10), ErrorKind::ReferenceCycle);
}

#[test]
fn nesting_deeper_than_the_limit_is_an_error() {
    let file = file_with_object_stream();
    let limits = Limits {
        max_resolve_depth: 1,
        ..Limits::default()
    };
    let mut resolver = Resolver::open(&file, limits).unwrap();

    // Object 3 needs its object stream, one level deeper.
    assert_eq!(
        error_kind(&mut resolver, 3),
        ErrorKind::LimitExceeded(Limit::ResolveDepth)
    );
    assert!(resolver.resolve(id(1)).is_ok());
}

#[test]
fn a_file_that_cant_be_loaded_is_an_error() {
    let error = Resolver::open(b"%PDF-1.7\n", Limits::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::MissingStartxref);
}

#[test]
fn resolvers_can_be_sent_to_other_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Resolver<'static>>();
}

// Indirect stream lengths

// Object 1 is a stream whose `/Length` is `length`, holding `data`. The data has
// `endstream` inside it, so only the real length finds its end.
const DATA: &[u8] = b"BT (endstream) Tj ET";

fn stream_file(length: &str, others: &[(u32, &str)]) -> (Vec<u8>, usize) {
    let mut pdf = Pdf::new();
    let offset = pdf.offset();
    pdf.write(&format!("1 0 obj\n<< /Length {length} >>\nstream\n"));
    let data_start = pdf.offset();
    pdf.file.extend_from_slice(DATA);
    pdf.write("\nendstream\nendobj\n");
    let mut entries = vec![(1, in_use(offset))];
    for &(number, body) in others {
        entries.push((number, in_use(pdf.object(number, body))));
    }
    let start = pdf.table(&entries, "/Size 10");
    pdf.end(start);
    (pdf.file, data_start)
}

// Cutting the data at the `endstream` inside it leaves the rest of it before
// `endobj`, so a stream without its real length also misses `endobj`.
fn cut_at_endstream(resolver: &Resolver<'_>, kind: DiagnosticKind, data_start: usize) {
    let found = diagnostics(resolver);
    assert_eq!(found[0], (kind, data_start), "{found:?}");
    assert_eq!(found[1].0, DiagnosticKind::MissingEndobj, "{found:?}");
    assert_eq!(found.len(), 2, "{found:?}");
}

fn stream_data(file: &[u8], resolver: &mut Resolver<'_>) -> Vec<u8> {
    let object = resolver.resolve(id(1)).unwrap();
    object.as_stream().unwrap().raw_data(file).unwrap().to_vec()
}

#[test]
fn an_indirect_length_is_resolved() {
    let (file, _) = stream_file("2 0 R", &[(2, &DATA.len().to_string())]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(stream_data(&file, &mut resolver), DATA);
    assert!(resolver.diagnostics().is_empty());
}

#[test]
fn an_indirect_length_can_be_in_an_object_stream() {
    let mut pdf = Pdf::new();
    let one = pdf.offset();
    pdf.write("1 0 obj\n<< /Length 3 0 R >>\nstream\n");
    pdf.file.extend_from_slice(DATA);
    pdf.write("\nendstream\nendobj\n");
    let length = DATA.len().to_string();
    let ten = pdf.object_stream(10, &[(3, &length)]);
    let start = pdf.xref_stream(
        11,
        &[(1, in_use(one)), (3, compressed(10, 0)), (10, in_use(ten))],
        "/Size 12",
    );
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    assert_eq!(stream_data(&pdf.file, &mut resolver), DATA);
    assert!(resolver.diagnostics().is_empty());
}

#[test]
fn a_length_that_refers_to_its_own_stream_ends_at_endstream() {
    let (file, data_start) = stream_file("1 0 R", &[]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(stream_data(&file, &mut resolver), b"BT (");
    cut_at_endstream(
        &resolver,
        DiagnosticKind::UnresolvedStreamLength,
        data_start,
    );
}

#[test]
fn lengths_of_two_streams_that_refer_to_each_other_end_at_endstream() {
    let (file, _) = stream_file(
        "2 0 R",
        &[(2, "<< /Length 1 0 R >>\nstream\nabc\nendstream")],
    );
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(stream_data(&file, &mut resolver), b"BT (");
    let other = resolver.resolve(id(2)).unwrap();
    assert_eq!(other.as_stream().unwrap().raw_data(&file).unwrap(), b"abc");
    let kinds: Vec<_> = resolver.diagnostics().iter().map(|d| d.kind()).collect();
    // Object 1 is cut at the `endstream` inside its data, so it misses `endobj`.
    assert_eq!(
        kinds,
        [
            DiagnosticKind::UnresolvedStreamLength,
            DiagnosticKind::UnresolvedStreamLength,
            DiagnosticKind::MissingEndobj,
        ]
    );
}

#[test]
fn a_length_that_isnt_an_integer_ends_at_endstream() {
    for (length, others) in [("9 0 R", &[][..]), ("2 0 R", &[(2, "/Name")][..])] {
        let (file, data_start) = stream_file(length, others);
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

        assert_eq!(stream_data(&file, &mut resolver), b"BT (", "{length}");
        cut_at_endstream(
            &resolver,
            DiagnosticKind::UnresolvedStreamLength,
            data_start,
        );
    }
}

#[test]
fn a_wrong_indirect_length_ends_at_endstream() {
    let (file, data_start) = stream_file("2 0 R", &[(2, "7")]);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

    assert_eq!(stream_data(&file, &mut resolver), b"BT (");
    cut_at_endstream(&resolver, DiagnosticKind::WrongStreamLength, data_start);
}

#[test]
fn errors_about_entries_past_the_end_are_reported_at_the_end() {
    // Found by fuzzing: object stream 10 has an offset past the end of the file
    // and another generation, so it resolves to null, and the error about object
    // 3 used that offset.
    let mut pdf = Pdf::new();
    let start = pdf.xref_stream(
        11,
        &[
            (3, compressed(10, 0)),
            (
                10,
                XrefEntry::InUse {
                    offset: 54_785,
                    generation: 7,
                },
            ),
        ],
        "/Size 12",
    );
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    let error = resolver.resolve(id(3)).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::NotAnObjectStream, pdf.file.len())
    );
    assert!(
        resolver
            .diagnostics()
            .iter()
            .all(|d| d.offset() <= pdf.file.len())
    );
}
