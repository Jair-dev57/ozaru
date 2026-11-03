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
