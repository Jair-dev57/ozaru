use ozaru_engine::base::{Diagnostic, DiagnosticKind, Diagnostics};

#[test]
fn diagnostics_keep_their_kind_and_offset() {
    let diagnostic = Diagnostic::new(DiagnosticKind::DuplicateDictionaryKey, 9);

    assert_eq!(diagnostic.kind(), DiagnosticKind::DuplicateDictionaryKey);
    assert_eq!(diagnostic.offset(), 9);
}

#[test]
fn diagnostics_describe_what_was_worked_around_and_where() {
    assert_eq!(
        Diagnostic::new(DiagnosticKind::DuplicateDictionaryKey, 9).to_string(),
        "repeated dictionary key, the last value is kept at byte 9"
    );
    assert_eq!(
        Diagnostic::new(DiagnosticKind::MissingEndobj, 30).to_string(),
        "missing endobj, the object is read anyway at byte 30"
    );
    let messages: Vec<String> = [
        DiagnosticKind::MissingStreamLength,
        DiagnosticKind::WrongStreamLength,
        DiagnosticKind::UnresolvedStreamLength,
        DiagnosticKind::StreamLineEndIsCarriageReturn,
        DiagnosticKind::MissingStreamLineEnd,
        DiagnosticKind::MissingEndOfFileMarker,
        DiagnosticKind::NonStandardXrefEntry,
        DiagnosticKind::XrefSubsectionTooShort,
        DiagnosticKind::DuplicateXrefEntry,
        DiagnosticKind::DamagedFlateData,
        DiagnosticKind::MissingZlibHeader,
        DiagnosticKind::IncompletePredictorRow,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(
        messages,
        [
            "stream has no length, its data ends at endstream",
            "stream length is wrong, its data ends at endstream",
            "stream length is a reference, its data ends at endstream",
            "stream keyword is followed by a lone carriage return",
            "stream keyword is not followed by an end of line",
            "missing %%EOF after startxref",
            "cross-reference entry is not 20 bytes, it is read anyway",
            "cross-reference subsection has fewer entries than stated",
            "repeated cross-reference entry, the first one is kept",
            "Flate data is damaged, the data before the damage is kept",
            "Flate data has no zlib header, it is read as raw deflate",
            "last predictor row is incomplete, it is decoded as far as it goes",
        ]
    );
}

#[test]
fn a_new_list_is_empty() {
    let diagnostics = Diagnostics::new(10);

    assert!(diagnostics.is_empty());
    assert_eq!(diagnostics.len(), 0);
    assert_eq!(diagnostics.dropped(), 0);
}

#[test]
fn diagnostics_are_kept_in_order() {
    let mut diagnostics = Diagnostics::new(10);
    diagnostics.push(DiagnosticKind::DuplicateDictionaryKey, 5);
    diagnostics.push(DiagnosticKind::DuplicateDictionaryKey, 2);

    let offsets: Vec<_> = diagnostics.iter().map(Diagnostic::offset).collect();
    assert_eq!(offsets, [5, 2]);
    assert_eq!(diagnostics.len(), 2);
}

#[test]
fn diagnostics_past_the_capacity_are_counted_but_not_kept() {
    let mut diagnostics = Diagnostics::new(2);
    for offset in 0..5 {
        diagnostics.push(DiagnosticKind::DuplicateDictionaryKey, offset);
    }

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics.dropped(), 3);
}

#[test]
fn a_list_with_only_dropped_diagnostics_is_not_empty() {
    let mut diagnostics = Diagnostics::new(0);
    diagnostics.push(DiagnosticKind::DuplicateDictionaryKey, 0);

    assert_eq!(diagnostics.len(), 0);
    assert!(!diagnostics.is_empty());
}
