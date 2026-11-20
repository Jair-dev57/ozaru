use std::time::{Duration, Instant};

use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::{LoadedXref, Resolver, XrefEntry, rebuild_xref};

mod common;
use common::Pdf;

fn rebuild(file: &[u8]) -> LoadedXref {
    rebuild_xref(file, Limits::default()).expect("file should have objects")
}

fn in_use(offset: usize) -> XrefEntry {
    XrefEntry::InUse {
        offset,
        generation: 0,
    }
}

fn root(loaded: &LoadedXref) -> Option<ObjectId> {
    loaded
        .table
        .trailer()
        .get("Root")
        .and_then(Object::as_reference)
}

fn diagnostics(resolver: &Resolver<'_>) -> Vec<(DiagnosticKind, usize)> {
    resolver
        .diagnostics()
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

// Rebuilding

#[test]
fn every_object_header_becomes_an_entry() {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    let two = pdf.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
    pdf.write("trailer\n<< /Size 3 /Root 1 0 R >>\n");

    let loaded = rebuild(&pdf.file);

    assert_eq!(
        loaded.table.iter().collect::<Vec<_>>(),
        [(1, in_use(one)), (2, in_use(two))]
    );
    assert_eq!(root(&loaded), Some(ObjectId::new(1, 0)));
    assert_eq!(loaded.sections, 0);
}

#[test]
fn headers_keep_their_generation() {
    let mut pdf = Pdf::new();
    let offset = pdf.offset();
    pdf.write("7 3 obj\n(seven)\nendobj\n");

    assert_eq!(
        rebuild(&pdf.file).table.get(7),
        Some(XrefEntry::InUse {
            offset,
            generation: 3
        })
    );
}

#[test]
fn the_last_version_of_an_object_wins() {
    let mut pdf = Pdf::new();
    pdf.object(1, "(old)");
    let newer = pdf.object(1, "(new)");

    assert_eq!(rebuild(&pdf.file).table.get(1), Some(in_use(newer)));
}

#[test]
fn headers_inside_stream_data_are_skipped() {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "<< /Length 18 >>\nstream\n9 0 obj (fake) \nendstream");
    let two = pdf.object(2, "(real)");

    let table = rebuild(&pdf.file).table;

    assert_eq!(
        table.iter().collect::<Vec<_>>(),
        [(1, in_use(one)), (2, in_use(two))]
    );
}

#[test]
fn things_that_only_look_like_headers_are_ignored() {
    let mut pdf = Pdf::new();
    pdf.write("1 0 endobj\nx1 0 obj\n1 0 objx\n1 99999 obj\n4294967296 0 obj\n1 obj\n");
    let real = pdf.object(2, "(real)");

    let table = rebuild(&pdf.file).table;

    assert_eq!(table.iter().collect::<Vec<_>>(), [(2, in_use(real))]);
}

#[test]
fn objects_in_object_streams_become_compressed_entries() {
    let mut pdf = Pdf::new();
    let ten = pdf.object_stream(10, &[(3, "(three)"), (4, "(four)")]);
    let four = pdf.object(4, "(loose four)");

    let table = rebuild(&pdf.file).table;

    assert_eq!(table.get(10), Some(in_use(ten)));
    assert_eq!(
        table.get(3),
        Some(XrefEntry::Compressed {
            stream: 10,
            index: 0
        })
    );
    // An object also outside of object streams keeps that entry.
    assert_eq!(table.get(4), Some(in_use(four)));
}

// The trailer

#[test]
fn the_last_trailer_with_a_known_root_is_used() {
    let mut pdf = Pdf::new();
    pdf.object(1, "<< /Type /Catalog >>");
    pdf.object(2, "<< /Type /Catalog >>");
    pdf.write("trailer\n<< /Root 1 0 R /Info 5 0 R >>\n");
    pdf.write("trailer\n<< /Root 2 0 R >>\n");
    pdf.write("trailer\n<< /Root 9 0 R >>\n");
    pdf.write("trailer\n<< /Size 3 >>\n");

    assert_eq!(root(&rebuild(&pdf.file)), Some(ObjectId::new(2, 0)));
}

#[test]
fn a_cross_reference_stream_dictionary_can_be_the_trailer() {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "<< /Type /Catalog >>");
    pdf.xref_stream(5, &[(1, in_use(one))], "/Size 6 /Root 1 0 R");

    assert_eq!(root(&rebuild(&pdf.file)), Some(ObjectId::new(1, 0)));
}

#[test]
fn without_a_trailer_the_last_catalog_is_the_root() {
    let mut pdf = Pdf::new();
    pdf.object(1, "<< /Type /Catalog /Version /1.4 >>");
    pdf.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
    pdf.object(3, "<< /Type /Catalog /Pages 2 0 R >>");
    pdf.object(4, "(mentions /Catalog but isn't one)");

    assert_eq!(root(&rebuild(&pdf.file)), Some(ObjectId::new(3, 0)));
}

#[test]
fn without_a_trailer_or_catalog_the_trailer_is_empty() {
    let mut pdf = Pdf::new();
    pdf.object(1, "(alone)");

    assert!(rebuild(&pdf.file).table.trailer().is_empty());
}

#[test]
fn a_file_without_objects_cant_be_rebuilt() {
    for file in [&b""[..], b"%PDF-1.7\ntrailer << /Root 1 0 R >>\n"] {
        let error = rebuild_xref(file, Limits::default()).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NoObjectsFound);
    }
}

// The resolver

#[test]
fn a_file_with_broken_cross_reference_data_is_rebuilt() {
    let mut pdf = Pdf::new();
    pdf.object(1, "<< /Type /Catalog >>");
    pdf.object(2, "(two)");
    pdf.write("trailer\n<< /Root 1 0 R >>\n");
    let broken = pdf.offset();
    pdf.write("xref\n0 3\nbroken\n");
    pdf.end(broken);

    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    assert_eq!(
        diagnostics(&resolver),
        [(DiagnosticKind::XrefRebuilt, broken + 9)]
    );
    let two = resolver.resolve(ObjectId::new(2, 0)).unwrap();
    assert_eq!(two.as_string().unwrap().as_bytes(), b"two");
    assert!(resolver.trailer().get("Root").is_some());
}

#[test]
fn a_file_that_cant_be_rebuilt_keeps_the_original_error() {
    let error = Resolver::open(b"%PDF-1.7\nnothing here\n", Limits::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::MissingStartxref);
}

#[test]
fn an_object_that_isnt_at_its_offset_is_found_by_scanning() {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "(one)");
    pdf.write("% a comment added after the table was written\n");
    let two = pdf.object(2, "(two)");
    // The table gives object 2 the offset of object 1.
    let wrong = one;
    assert_ne!(wrong, two);
    let start = pdf.table(&[(1, in_use(one)), (2, in_use(wrong))], "/Size 3");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    let object = resolver.resolve(ObjectId::new(2, 0)).unwrap();

    assert_eq!(object.as_string().unwrap().as_bytes(), b"two");
    assert_eq!(
        diagnostics(&resolver),
        [(DiagnosticKind::ObjectRelocated, wrong)]
    );
    assert!(resolver.resolve(ObjectId::new(1, 0)).is_ok());
}

#[test]
fn an_object_that_isnt_anywhere_is_still_an_error() {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "(one)");
    let start = pdf.table(&[(1, in_use(one)), (2, in_use(3))], "/Size 3");
    pdf.end(start);
    let mut resolver = Resolver::open(&pdf.file, Limits::default()).unwrap();

    let error = resolver.resolve(ObjectId::new(2, 0)).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidObjectHeader);
    assert!(resolver.diagnostics().is_empty());
}

// Hostile files

#[test]
fn rebuilding_takes_linear_time() {
    let cases: [&[u8]; 3] = [
        b"1 0 obj << >> stream\n",
        b"trailer << /A [",
        b"1 0 obj << /Type /Catalog ",
    ];
    for unit in cases {
        let file = unit.repeat(2_000_000 / unit.len());
        let start = Instant::now();

        let _ = rebuild_xref(&file, Limits::default());

        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{} took {:?}",
            String::from_utf8_lossy(unit),
            start.elapsed()
        );
    }
}
