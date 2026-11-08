use std::path::Path;

use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::document::Rectangle;
use ozaru_engine::{Document, OpenOptions};

fn read(name: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read(root.join(name)).unwrap()
}

fn kinds(document: &Document<'_>) -> Vec<DiagnosticKind> {
    document.diagnostics().iter().map(|d| d.kind()).collect()
}

const USER_PASSWORD_PDF: &str = "crates/ozaru-engine/tests/files/report-user-password.pdf";

#[test]
fn every_readable_real_pdf_opens_with_its_pages() {
    // The page counts ozaru 0.1.7 gives with pdf_oxide.
    for (name, count) in [
        ("crates/ozaru-engine/tests/files/report-flate.pdf", 3),
        ("crates/ozaru-engine/tests/files/report-rc4-128.pdf", 3),
        ("crates/ozaru-engine/tests/files/report-rc4-40.pdf", 3),
        (USER_PASSWORD_PDF, 3),
        ("crates/ozaru-engine/tests/files/report-xrefstm.pdf", 3),
        ("docs/assets/damaged-page.pdf", 3),
        ("docs/assets/report.pdf", 3),
        ("tests/factura.pdf", 3),
        ("tests/owner_password.pdf", 1),
        ("tests/page_error.pdf", 3),
        ("tests/page_panic.pdf", 3),
        ("tests/password.pdf", 1),
    ] {
        let file = read(name);

        let document = Document::open(&file).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert_eq!(document.page_count(), count, "{name}");
        assert_eq!(document.pages().len(), count, "{name}");
        assert!(document.page(count - 1).is_some(), "{name}");
        assert!(document.page(count).is_none(), "{name}");
    }
}

#[test]
fn the_sample_report_opens_without_diagnostics() {
    let file = read("docs/assets/report.pdf");

    let document = Document::open(&file).unwrap();

    assert!(!document.is_encrypted());
    assert!(!document.needs_password());
    assert!(document.catalog().id().is_some());
    let page = document.page(0).unwrap();
    assert!((page.media_box().width() - 595.2756).abs() < 0.01);
    assert_eq!(page.rotation(), 0);
    let title = document.info().and_then(|info| info.title());
    assert_eq!(title.as_deref(), Some("ozaru sample report"));
    assert!(document.diagnostics().is_empty());
}

#[test]
fn the_diagnostics_of_every_stage_are_together() {
    // Its cross-reference table is broken, and the /MediaBox key of page 3 too.
    let file = read("tests/page_panic.pdf");

    let document = Document::open(&file).unwrap();

    assert_eq!(
        kinds(&document),
        [DiagnosticKind::XrefRebuilt, DiagnosticKind::MissingMediaBox]
    );
}

#[test]
fn the_damaged_pages_are_marked() {
    let file = read("docs/assets/damaged-page.pdf");

    let document = Document::open(&file).unwrap();

    let damaged: Vec<_> = document.pages().iter().map(|p| p.is_damaged()).collect();
    assert_eq!(damaged, [false, false, true]);
    // In this file each page has its own media box, so the damaged one gets the
    // default.
    assert_eq!(document.page(2).unwrap().media_box(), Rectangle::LETTER);
    // Searching for the root of the page tree reads every object, and one of its
    // streams has a damaged /Length too.
    assert_eq!(
        kinds(&document),
        [
            DiagnosticKind::MissingStreamLength,
            DiagnosticKind::PageTreeFound,
            DiagnosticKind::DamagedPage,
        ]
    );
}

#[test]
fn a_pdf_whose_page_tree_is_destroyed_gets_its_pages_rebuilt() {
    // No reference reader opens it: the header of its page tree, object 8, is
    // damaged into `8 . obj`, and its second page object is damaged too.
    let file = read("tests/damaged.pdf");

    let document = Document::open(&file).unwrap();

    assert_eq!(document.page_count(), 3);
    let damaged: Vec<_> = document.pages().iter().map(|p| p.is_damaged()).collect();
    assert_eq!(damaged, [false, true, false]);
    assert_eq!(
        kinds(&document),
        [
            DiagnosticKind::XrefRebuilt,
            DiagnosticKind::PageTreeRebuilt,
            DiagnosticKind::DamagedPage,
        ]
    );
}

#[test]
fn input_that_is_not_a_pdf_is_an_error() {
    for input in [
        &b""[..],
        b"hello",
        b"%PDF-1.7\n",
        b"%PDF-1.7\n1 0 obj\n42\nendobj\n",
    ] {
        assert!(Document::open(input).is_err(), "{input:?}");
    }
}

// Passwords

#[test]
fn a_file_with_a_user_password_opens_locked_without_it() {
    let file = read(USER_PASSWORD_PDF);

    let document = Document::open(&file).unwrap();

    assert!(document.is_encrypted());
    assert!(document.needs_password());
    assert_eq!(document.page_count(), 3);
    assert!(document.info().is_none());
    assert_eq!(kinds(&document), [DiagnosticKind::LockedInfo]);
}

#[test]
fn the_right_password_opens_it_unlocked() {
    let file = read(USER_PASSWORD_PDF);
    let options = OpenOptions {
        password: Some(b"user"),
        ..OpenOptions::default()
    };

    let document = Document::open_with(&file, options).unwrap();

    assert!(document.is_encrypted());
    assert!(!document.needs_password());
    assert_eq!(document.page_count(), 3);
    let title = document.info().and_then(|info| info.title());
    assert_eq!(title.as_deref(), Some("ozaru sample report"));
    assert!(document.diagnostics().is_empty());
}

#[test]
fn a_wrong_password_is_an_error() {
    let file = read(USER_PASSWORD_PDF);
    for password in [&b"wrong"[..], b"", b"Owner"] {
        let options = OpenOptions {
            password: Some(password),
            ..OpenOptions::default()
        };

        let error = Document::open_with(&file, options).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::PasswordRequired, "{password:?}");
        assert!(error.offset() > 0, "{password:?}");
    }
}

#[test]
fn unlocking_reads_the_document_again_decrypted() {
    let file = read(USER_PASSWORD_PDF);
    let mut document = Document::open(&file).unwrap();

    assert!(!document.unlock(b"wrong"));
    assert!(document.needs_password());
    assert!(document.unlock(b"user"));

    assert!(!document.needs_password());
    assert_eq!(document.page_count(), 3);
    assert!(document.info().is_some());
    assert!(document.diagnostics().is_empty());
    assert!(document.unlock(b"anything"), "it is unlocked already");
}

#[test]
fn files_that_open_without_a_password_are_decrypted() {
    for name in [
        "tests/owner_password.pdf",
        "crates/ozaru-engine/tests/files/report-rc4-40.pdf",
        "crates/ozaru-engine/tests/files/report-rc4-128.pdf",
    ] {
        let file = read(name);

        let document = Document::open(&file).unwrap();

        assert!(document.is_encrypted(), "{name}");
        assert!(!document.needs_password(), "{name}");
        assert!(document.info().and_then(|i| i.title()).is_some(), "{name}");
    }
}

#[test]
fn a_password_is_ignored_for_a_file_that_is_not_encrypted() {
    let file = read("docs/assets/report.pdf");
    let options = OpenOptions {
        password: Some(b"anything"),
        ..OpenOptions::default()
    };

    let mut document = Document::open_with(&file, options).unwrap();

    assert_eq!(document.page_count(), 3);
    assert!(document.unlock(b"anything"));
}

// Limits

#[test]
fn the_limits_of_the_options_are_used() {
    let file = read("docs/assets/report.pdf");
    let options = OpenOptions {
        limits: Limits {
            max_pages: 2,
            ..Limits::default()
        },
        password: None,
    };

    let error = Document::open_with(&file, options).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::LimitExceeded(Limit::Pages));
}

#[test]
fn the_resolver_reads_what_the_document_does_not() {
    let file = read("docs/assets/report.pdf");
    let mut document = Document::open(&file).unwrap();
    let contents = document
        .page(0)
        .unwrap()
        .dictionary()
        .get("Contents")
        .and_then(|c| c.as_reference())
        .unwrap();

    let data = document.resolver().stream_data(contents).unwrap();

    assert!(data.data.windows(2).any(|w| w == b"BT"));
}
