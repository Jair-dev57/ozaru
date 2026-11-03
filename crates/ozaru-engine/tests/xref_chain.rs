use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::xref::{LoadedXref, XrefEntry, load_xref, parse_xref_section};

mod common;
use common::Pdf;

fn load(file: &[u8]) -> LoadedXref {
    load_xref(file, Limits::default()).expect("file should be valid")
}

fn diagnostics(loaded: &LoadedXref) -> Vec<(DiagnosticKind, usize)> {
    loaded
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

fn in_use(offset: usize) -> XrefEntry {
    XrefEntry::InUse {
        offset,
        generation: 0,
    }
}

fn free(generation: u16) -> XrefEntry {
    XrefEntry::Free { generation }
}

// An original file with objects 1 and 2, before any update. Returns the file and
// the offset of its table.
fn original() -> (Pdf, usize, usize, usize) {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    let two = pdf.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
    let table = pdf.table(
        &[(0, free(65535)), (1, in_use(one)), (2, in_use(two))],
        "/Size 3 /Root 1 0 R",
    );
    (pdf, one, two, table)
}

// One section

#[test]
fn a_file_without_updates_has_one_section() {
    let (mut pdf, one, two, table) = original();
    pdf.end(table);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 1);
    assert_eq!(
        loaded.table.iter().collect::<Vec<_>>(),
        [(0, free(65535)), (1, in_use(one)), (2, in_use(two))]
    );
    assert!(loaded.diagnostics.is_empty());
}

#[test]
fn a_file_with_only_an_xref_stream_is_loaded() {
    let mut pdf = Pdf::new();
    let one = pdf.object(1, "<< /Type /Catalog >>");
    let stream = pdf.xref_stream(
        2,
        &[(0, free(65535)), (1, in_use(one))],
        "/Size 3 /Root 1 0 R",
    );
    pdf.end(stream);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.table.get(1), Some(in_use(one)));
    assert_eq!(
        loaded.table.trailer().get("Root"),
        Some(&Object::Reference(ObjectId::new(1, 0)))
    );
}

#[test]
fn a_section_is_a_table_or_a_stream() {
    let (mut pdf, _, _, table) = original();
    let stream = pdf.xref_stream(3, &[(3, in_use(table))], "/Size 4");

    let limits = Limits::default();
    assert_eq!(
        parse_xref_section(&pdf.file, table, limits)
            .unwrap()
            .table
            .len(),
        3
    );
    assert_eq!(
        parse_xref_section(&pdf.file, stream, limits)
            .unwrap()
            .table
            .len(),
        1
    );
    assert_eq!(
        parse_xref_section(&pdf.file, 0, limits).unwrap_err().kind(),
        ErrorKind::NotAnXrefStream
    );
}

// Incremental updates

#[test]
fn an_update_replaces_the_entries_it_lists() {
    let (mut pdf, one, _, table) = original();
    let new_two = pdf.object(2, "<< /Type /Pages /Kids [] /Count 0 /Changed true >>");
    let update = pdf.table(
        &[(2, in_use(new_two))],
        &format!("/Size 3 /Root 1 0 R /Info 9 0 R /Prev {table}"),
    );
    pdf.end(update);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 2);
    assert_eq!(loaded.table.get(1), Some(in_use(one)));
    assert_eq!(loaded.table.get(2), Some(in_use(new_two)));
    assert!(loaded.diagnostics.is_empty());
}

#[test]
fn the_newest_trailer_is_kept() {
    let (mut pdf, _, _, table) = original();
    let update = pdf.table(
        &[],
        &format!("/Size 3 /Root 1 0 R /Info 9 0 R /Prev {table}"),
    );
    pdf.end(update);

    let loaded = load(&pdf.file);

    assert!(loaded.table.trailer().get("Info").is_some());
    assert_eq!(
        loaded.table.trailer().get("Prev"),
        Some(&Object::Integer(table as i64))
    );
}

#[test]
fn an_update_can_delete_an_object() {
    let (mut pdf, _, _, table) = original();
    let update = pdf.table(&[(2, free(1))], &format!("/Size 3 /Prev {table}"));
    pdf.end(update);

    assert_eq!(load(&pdf.file).table.get(2), Some(free(1)));
}

#[test]
fn a_chain_of_several_updates_is_followed() {
    let (mut pdf, one, two, mut previous) = original();
    let mut newest_one = one;
    for _ in 0..5 {
        newest_one = pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
        previous = pdf.table(
            &[(1, in_use(newest_one))],
            &format!("/Size 3 /Prev {previous}"),
        );
    }
    pdf.end(previous);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 6);
    assert_eq!(loaded.table.get(1), Some(in_use(newest_one)));
    assert_eq!(loaded.table.get(2), Some(in_use(two)));
}

#[test]
fn tables_and_streams_can_be_mixed_in_a_chain() {
    let (mut pdf, one, _, table) = original();
    let three = pdf.object(3, "(added)");
    let stream = pdf.xref_stream(
        4,
        &[
            (3, in_use(three)),
            (
                5,
                XrefEntry::Compressed {
                    stream: 6,
                    index: 0,
                },
            ),
        ],
        &format!("/Size 7 /Prev {table}"),
    );
    pdf.end(stream);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 2);
    assert_eq!(loaded.table.get(1), Some(in_use(one)));
    assert_eq!(loaded.table.get(3), Some(in_use(three)));
    assert_eq!(
        loaded.table.get(5),
        Some(XrefEntry::Compressed {
            stream: 6,
            index: 0
        })
    );
}

// Hybrid files

#[test]
fn a_hybrid_file_adds_the_entries_of_its_xref_stream() {
    let (mut pdf, one, _, table) = original();
    let stream = pdf.xref_stream(
        9,
        &[
            (1, in_use(1)),
            (
                3,
                XrefEntry::Compressed {
                    stream: 8,
                    index: 0,
                },
            ),
            (
                4,
                XrefEntry::Compressed {
                    stream: 8,
                    index: 1,
                },
            ),
        ],
        "/Size 10",
    );
    let three = pdf.object(3, "(older)");
    let older_three = pdf.table(&[(3, in_use(three))], &format!("/Size 4 /Prev {table}"));
    // The table marks 4 as free for older readers; the stream has the real entry.
    let main = pdf.table(
        &[(1, in_use(one)), (4, free(0))],
        &format!("/Size 10 /XRefStm {stream} /Prev {older_three}"),
    );
    pdf.end(main);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 4);
    // An entry in use in the table wins over the stream.
    assert_eq!(loaded.table.get(1), Some(in_use(one)));
    // The stream comes before /Prev.
    assert_eq!(
        loaded.table.get(3),
        Some(XrefEntry::Compressed {
            stream: 8,
            index: 0
        })
    );
    // The stream replaces a free entry in its own table.
    assert_eq!(
        loaded.table.get(4),
        Some(XrefEntry::Compressed {
            stream: 8,
            index: 1
        })
    );
    assert!(loaded.diagnostics.is_empty());
}

#[test]
fn a_broken_xref_stream_in_a_hybrid_file_keeps_the_table() {
    let (mut pdf, one, _, table) = original();
    let main = pdf.table(&[], &format!("/Size 3 /XRefStm 5 /Prev {table}"));
    pdf.end(main);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.table.get(1), Some(in_use(one)));
    assert_eq!(
        diagnostics(&loaded),
        [(DiagnosticKind::BrokenXrefSection, 5)]
    );
}

// Broken chains

#[test]
fn a_loop_in_the_chain_stops_it() {
    let (mut pdf, one, ..) = original();
    let a_offset = pdf.offset();
    // The first update points at the second, which points back at the first.
    let b_offset_guess = a_offset + "xref\ntrailer\n<< /Size 3 /Prev 0000 >>\n".len();
    let a = pdf.table(&[], &format!("/Size 3 /Prev {b_offset_guess:04}"));
    let b = pdf.table(&[(1, in_use(one))], &format!("/Size 3 /Prev {a}"));
    assert_eq!(b, b_offset_guess);
    pdf.end(a);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 2);
    assert_eq!(loaded.table.get(1), Some(in_use(one)));
    assert_eq!(diagnostics(&loaded), [(DiagnosticKind::XrefLoop, a)]);
    // The original table isn't reachable, so object 2 is unknown.
    assert_eq!(loaded.table.get(2), None);
}

#[test]
fn a_section_that_points_at_itself_stops_the_chain() {
    let (mut pdf, ..) = original();
    let offset = pdf.offset();
    let update = pdf.table(&[], &format!("/Size 3 /Prev {offset}"));
    pdf.end(update);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.sections, 1);
    assert_eq!(diagnostics(&loaded), [(DiagnosticKind::XrefLoop, offset)]);
}

#[test]
fn a_broken_older_section_keeps_the_newer_entries() {
    let (mut pdf, ..) = original();
    let three = pdf.object(3, "(new)");
    let update = pdf.table(&[(3, in_use(three))], "/Size 4 /Prev 9");
    pdf.end(update);

    let loaded = load(&pdf.file);

    assert_eq!(loaded.table.get(3), Some(in_use(three)));
    assert_eq!(
        diagnostics(&loaded),
        [(DiagnosticKind::BrokenXrefSection, 9)]
    );
}

#[test]
fn a_prev_that_isnt_an_offset_stops_the_chain() {
    for prev in ["/Prev /None", "/Prev -1", "/Prev 1.5"] {
        let (mut pdf, ..) = original();
        let update = pdf.table(&[], &format!("/Size 3 {prev}"));
        pdf.end(update);

        let loaded = load(&pdf.file);

        assert_eq!(loaded.sections, 1, "{prev}");
        assert_eq!(
            diagnostics(&loaded),
            [(DiagnosticKind::BrokenXrefSection, update)],
            "{prev}"
        );
    }
}

#[test]
fn diagnostics_of_each_section_are_kept() {
    let mut pdf = Pdf::new();
    let table = pdf.offset();
    pdf.write("xref\n0 1\n0000000000 65535 f\ntrailer\n<< /Size 1 >>\n");
    pdf.write("startxref\n");
    pdf.write(&table.to_string());

    let loaded = load(&pdf.file);

    assert_eq!(
        diagnostics(&loaded),
        [
            (DiagnosticKind::MissingEndOfFileMarker, pdf.file.len()),
            (DiagnosticKind::NonStandardXrefEntry, table + 9),
        ]
    );
}

// Errors

#[test]
fn the_section_at_startxref_must_be_readable() {
    let (mut pdf, ..) = original();
    pdf.end(3);

    let error = load_xref(&pdf.file, Limits::default()).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::InvalidObjectHeader, 3)
    );
}

#[test]
fn a_file_without_startxref_is_an_error() {
    let (pdf, ..) = original();

    let error = load_xref(&pdf.file, Limits::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::MissingStartxref);
}

#[test]
fn too_many_sections_are_an_error() {
    let (mut pdf, _, _, mut previous) = original();
    for _ in 0..3 {
        previous = pdf.table(&[], &format!("/Size 3 /Prev {previous}"));
    }
    pdf.end(previous);
    let limits = Limits {
        max_xref_sections: 3,
        ..Limits::default()
    };

    let error = load_xref(&pdf.file, limits).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::LimitExceeded(Limit::XrefSections));
    assert_eq!(load_xref(&pdf.file, Limits::default()).unwrap().sections, 4);
}
