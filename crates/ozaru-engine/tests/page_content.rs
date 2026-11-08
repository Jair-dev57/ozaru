//! The content of whole pages: their `/Contents`, damaged or locked, and the
//! real PDFs of the repository.

use std::path::{Path, PathBuf};

use ozaru_engine::base::{DiagnosticKind, Limits};
use ozaru_engine::content::{Interpreter, PageContent};
use ozaru_engine::{Document, OpenOptions};

mod common;
use common::{file_with, stream_object};

fn file(contents: &str, objects: &[(u32, String)]) -> Vec<u8> {
    let mut all = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
                 /Contents {contents} >>"
            ),
        ),
        (
            4,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
    ];
    all.extend_from_slice(objects);
    file_with(&all)
}

fn run(file: &[u8]) -> PageContent {
    let mut document = Document::open(file).unwrap();
    let page = document.page(0).unwrap().clone();
    Interpreter::new(Limits::default()).run_page(document.resolver(), &page)
}

fn kinds(content: &PageContent) -> Vec<DiagnosticKind> {
    content.diagnostics.iter().map(|d| d.kind()).collect()
}

fn text(content: &PageContent) -> String {
    content
        .glyphs
        .iter()
        .filter_map(|g| g.text.as_deref())
        .collect()
}

const HELLO: &str = "BT /F1 10 Tf (Hello) Tj ET";

#[test]
fn contents_can_be_an_array_in_another_object() {
    // The streams are joined, so an operation can start in one and end in
    // the next.
    let content = run(&file(
        "10 0 R",
        &[
            (10, "[11 0 R 12 0 R]".to_string()),
            (11, stream_object("", "BT /F1 10 Tf")),
            (12, stream_object("", "(Hello) Tj ET")),
        ],
    ));
    assert_eq!(text(&content), "Hello");
    assert!(content.diagnostics.is_empty());
}

#[test]
fn contents_that_are_not_streams_are_skipped() {
    let content = run(&file("7", &[]));
    assert_eq!(kinds(&content), [DiagnosticKind::InvalidContents]);

    let content = run(&file("10 0 R", &[(10, "(text)".to_string())]));
    assert_eq!(kinds(&content), [DiagnosticKind::InvalidContents]);

    // In an array, the streams that can be read are kept.
    let content = run(&file(
        "[5 11 0 R 12 0 R 13 0 R]",
        &[
            (11, stream_object("", HELLO)),
            (12, "<< /Not /AStream >>".to_string()),
        ],
    ));
    assert_eq!(text(&content), "Hello");
    // 5 isn't a reference, 12 isn't a stream, and 13 is missing, so null.
    assert_eq!(
        kinds(&content),
        [
            DiagnosticKind::InvalidContents,
            DiagnosticKind::InvalidContents
        ]
    );
}

#[test]
fn streams_that_cant_be_decoded_are_skipped() {
    let content = run(&file(
        "[10 0 R 11 0 R]",
        &[
            (10, stream_object("/Filter /JBIG2Decode", "xx")),
            (11, stream_object("", HELLO)),
        ],
    ));
    assert_eq!(text(&content), "Hello");
    assert_eq!(kinds(&content), [DiagnosticKind::InvalidContents]);
}

#[test]
fn damaged_compressed_content_keeps_what_was_decoded() {
    let mut data =
        miniz_oxide::deflate::compress_to_vec_zlib(b"BT /F1 10 Tf (Hello) Tj (World) Tj ET", 6);
    data.truncate(data.len() - 12);
    let mut pdf = common::Pdf::new();
    let mut table = Vec::new();
    for (number, body) in [
        (1, "<< /Type /Catalog /Pages 2 0 R >>"),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        ),
        (4, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"),
    ] {
        table.push((
            number,
            ozaru_engine::xref::XrefEntry::InUse {
                offset: pdf.object(number, body),
                generation: 0,
            },
        ));
    }
    let offset = pdf.offset();
    pdf.write(&format!(
        "5 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n",
        data.len()
    ));
    pdf.file.extend(&data);
    pdf.write("\nendstream\nendobj\n");
    table.push((
        5,
        ozaru_engine::xref::XrefEntry::InUse {
            offset,
            generation: 0,
        },
    ));
    let start = pdf.table(&table, "/Size 6 /Root 1 0 R");
    pdf.end(start);

    let content = run(&pdf.file);
    assert!(text(&content).starts_with("Hello"), "{}", text(&content));
    assert!(kinds(&content).contains(&DiagnosticKind::DamagedFlateData));
}

#[test]
fn truncated_content_keeps_the_glyphs_before_the_cut() {
    let content = run(&file(
        "10 0 R",
        &[(10, stream_object("", "BT /F1 10 Tf (Hello) Tj (Wor"))],
    ));
    assert_eq!(text(&content), "Hello");
    assert_eq!(kinds(&content), [DiagnosticKind::DamagedContent]);
}

#[test]
fn a_locked_file_has_no_content_until_unlocked() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let file = std::fs::read(root.join("tests/files/report-user-password.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    assert!(document.needs_password());
    let page = document.page(0).unwrap().clone();
    let mut interpreter = Interpreter::new(Limits::default());
    let content = interpreter.run_page(document.resolver(), &page);
    assert!(content.glyphs.is_empty());
    assert_eq!(kinds(&content), [DiagnosticKind::LockedContent]);

    assert!(document.unlock(b"user"));
    let page = document.page(0).unwrap().clone();
    let content = interpreter.run_page(document.resolver(), &page);
    assert!(!content.glyphs.is_empty());
}

fn pdf_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<PathBuf> = [
        "tests",
        "docs/assets",
        "crates/ozaru-engine/tests/files",
        "crates/ozaru-engine/tests/corpus",
    ]
    .iter()
    .flat_map(|folder| std::fs::read_dir(root.join(folder)).unwrap())
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|e| e == "pdf"))
    .collect();
    files.sort();
    files
}

#[test]
fn every_page_of_the_repository_runs() {
    for path in pdf_files() {
        let file = std::fs::read(&path).unwrap();
        let options = OpenOptions {
            password: Some(b"user"),
            ..OpenOptions::default()
        };
        let Ok(mut document) =
            Document::open(&file).or_else(|_| Document::open_with(&file, options))
        else {
            continue;
        };
        let pages = document.pages().to_vec();
        let mut interpreter = Interpreter::new(Limits::default());
        for page in pages.iter().take(20) {
            let content = interpreter.run_page(document.resolver(), page);
            for glyph in &content.glyphs {
                assert!(glyph.matrix.is_finite(), "{}", path.display());
                assert!(glyph.width.is_finite());
            }
            assert!(content.diagnostics.iter().all(|d| d.offset() <= file.len()));
        }
    }
}

#[test]
fn the_sample_report_reads_without_problems() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = std::fs::read(root.join("docs/assets/report.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    let pages = document.pages().to_vec();
    let mut interpreter = Interpreter::new(Limits::default());
    for page in &pages {
        let content = interpreter.run_page(document.resolver(), page);
        assert!(content.diagnostics.is_empty(), "{:?}", kinds(&content));
        assert!(!content.glyphs.is_empty());
        let media_box = page.media_box();
        for glyph in &content.glyphs {
            let (x, y) = glyph.origin();
            assert!(x >= media_box.left && x <= media_box.right);
            assert!(y >= media_box.bottom && y <= media_box.top);
            assert!(glyph.text.is_some());
        }
    }
}
