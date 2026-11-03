use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits};
use ozaru_engine::xref::{Startxref, find_startxref};

fn find(input: &[u8]) -> Startxref {
    find_startxref(input, Limits::default()).expect("input should be valid")
}

fn error(input: &[u8]) -> (ErrorKind, usize) {
    let error = find_startxref(input, Limits::default()).expect_err("should fail");
    (error.kind(), error.offset())
}

fn diagnostics(found: &Startxref) -> Vec<(DiagnosticKind, usize)> {
    found
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

#[test]
fn the_offset_after_startxref_is_found() {
    let found = find(b"%PDF-1.7\nxref\nstartxref\n9\n%%EOF\n");

    assert_eq!(found.offset, 9);
    assert!(found.diagnostics.is_empty());
}

#[test]
fn line_ends_and_spaces_around_the_offset_are_accepted() {
    assert_eq!(find(b"%PDF-1.7\r\nstartxref\r\n0\r\n%%EOF").offset, 0);
    assert_eq!(find(b"%PDF-1.7 startxref 3 %%EOF").offset, 3);
}

#[test]
fn the_last_startxref_wins_after_an_incremental_update() {
    let input = b"%PDF-1.7\nstartxref\n1\n%%EOF\nstartxref\n2\n%%EOF\n";

    assert_eq!(find(input).offset, 2);
}

#[test]
fn junk_after_the_end_of_file_marker_is_accepted() {
    let mut input = b"%PDF-1.7\nstartxref\n5\n%%EOF\n".to_vec();
    input.extend(std::iter::repeat_n(b'\0', 500));

    assert_eq!(find(&input).offset, 5);
}

#[test]
fn a_missing_end_of_file_marker_is_reported() {
    let found = find(b"%PDF-1.7\nstartxref\n5\n");

    assert_eq!(found.offset, 5);
    assert_eq!(
        diagnostics(&found),
        [(DiagnosticKind::MissingEndOfFileMarker, 21)]
    );
}

#[test]
fn startxref_must_be_near_the_end() {
    let mut input = b"%PDF-1.7\nstartxref\n5\n%%EOF\n".to_vec();
    input.extend(std::iter::repeat_n(b' ', 2000));

    assert_eq!(error(&input), (ErrorKind::MissingStartxref, input.len()));
}

#[test]
fn a_file_without_startxref_is_an_error() {
    assert_eq!(error(b""), (ErrorKind::MissingStartxref, 0));
    assert_eq!(error(b"%PDF-1.7\n"), (ErrorKind::MissingStartxref, 9));
}

#[test]
fn startxref_inside_a_longer_word_is_ignored() {
    assert_eq!(
        error(b"%PDF-1.7\nxstartxref 1\n"),
        (ErrorKind::MissingStartxref, 22)
    );
    assert_eq!(find(b"%PDF startxref 1 xstartxref 2").offset, 1);
}

#[test]
fn the_offset_must_be_an_integer_inside_the_file() {
    assert_eq!(
        error(b"%PDF-1.7\nstartxref\nabc\n"),
        (ErrorKind::InvalidStartxref, 19)
    );
    assert_eq!(
        error(b"%PDF-1.7\nstartxref\n-1\n"),
        (ErrorKind::InvalidStartxref, 19)
    );
    assert_eq!(
        error(b"%PDF-1.7\nstartxref\n1.5\n"),
        (ErrorKind::InvalidStartxref, 19)
    );
    assert_eq!(
        error(b"%PDF-1.7\nstartxref\n99\n"),
        (ErrorKind::InvalidStartxref, 19)
    );
    assert_eq!(
        error(b"%PDF-1.7\nstartxref"),
        (ErrorKind::InvalidStartxref, 18)
    );
}
