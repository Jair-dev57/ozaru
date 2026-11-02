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
