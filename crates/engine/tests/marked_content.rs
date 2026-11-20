use ozaru_engine::Document;
use ozaru_engine::base::{DiagnosticKind, Limits};
use ozaru_engine::content::{Interpreter, PageContent};

mod common;
use common::{file_with, stream_object};

fn file(content: &str, resources: &str, objects: &[(u32, String)]) -> Vec<u8> {
    let mut all = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> {resources} >> \
                 /Contents 5 0 R >>"
            ),
        ),
        (
            4,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
        (5, stream_object("", content)),
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

#[test]
fn glyphs_know_their_sequence() {
    let content = run(&file(
        "BT /F1 10 Tf (a) Tj /P <</MCID 3>> BDC (b) Tj /Span <</ActualText (fi)>> BDC (c) Tj \
         EMC (d) Tj EMC (e) Tj ET",
        "",
        &[],
    ));
    assert!(content.diagnostics.is_empty(), "{:?}", kinds(&content));
    assert_eq!(content.marked_content.len(), 2);
    let paragraph = &content.marked_content[0];
    assert_eq!(paragraph.tag.as_bytes(), b"P");
    assert_eq!(paragraph.mcid, Some(3));
    assert_eq!(paragraph.parent, None);
    let span = &content.marked_content[1];
    assert_eq!(span.actual_text.as_deref(), Some("fi"));
    assert_eq!(span.parent, Some(0));
    let sequences: Vec<_> = content.glyphs.iter().map(|g| g.marked_content).collect();
    assert_eq!(sequences, [None, Some(0), Some(1), Some(0), None]);
    // From the innermost out.
    let tags: Vec<_> = content
        .marked_content_of(&content.glyphs[2])
        .map(|m| m.tag.as_bytes().to_vec())
        .collect();
    assert_eq!(tags, [b"Span".to_vec(), b"P".to_vec()]);
}

#[test]
fn bmc_has_no_properties() {
    let content = run(&file("/Artifact BMC BT /F1 10 Tf (a) Tj ET EMC", "", &[]));
    let artifact = &content.marked_content[0];
    assert_eq!(artifact.tag.as_bytes(), b"Artifact");
    assert!(artifact.properties.is_none());
    assert_eq!(content.glyphs[0].marked_content, Some(0));
}

#[test]
fn properties_can_be_named_in_the_resources() {
    let content = run(&file(
        "/Span /P1 BDC EMC /Span /P2 BDC EMC /OC /P9 BDC EMC",
        "/Properties << /P1 << /ActualText <FEFF00E9> >> /P2 10 0 R >>",
        &[
            (10, "<< /MCID 7 /ActualText 11 0 R >>".to_string()),
            (11, "(x)".to_string()),
        ],
    ));
    assert_eq!(content.marked_content[0].actual_text.as_deref(), Some("é"));
    assert_eq!(content.marked_content[1].mcid, Some(7));
    assert_eq!(content.marked_content[1].actual_text.as_deref(), Some("x"));
    assert!(content.marked_content[2].properties.is_none());
    assert_eq!(kinds(&content), [DiagnosticKind::MissingResource]);
}

#[test]
fn unbalanced_sequences_are_closed_at_the_end_of_their_stream() {
    let content = run(&file("EMC /A BMC /B BMC", "", &[]));
    assert_eq!(
        kinds(&content),
        [
            DiagnosticKind::UnbalancedMarkedContent,
            DiagnosticKind::UnbalancedMarkedContent
        ]
    );
    assert_eq!(content.marked_content.len(), 2);
}

#[test]
fn sequences_of_a_form_end_with_it() {
    let form = stream_object(
        "/Subtype /Form /BBox [0 0 10 10]",
        "/Inner BMC BT /F1 10 Tf (a) Tj ET",
    );
    let content = run(&file(
        "/Outer BMC /X1 Do BT /F1 10 Tf (b) Tj ET EMC",
        "/XObject << /X1 10 0 R >>",
        &[(10, form)],
    ));
    assert_eq!(kinds(&content), [DiagnosticKind::UnbalancedMarkedContent]);
    assert_eq!(content.marked_content[1].parent, Some(0));
    let sequences: Vec<_> = content.glyphs.iter().map(|g| g.marked_content).collect();
    assert_eq!(sequences, [Some(1), Some(0)]);
}

#[test]
fn bdc_with_wrong_operands_still_opens_a_sequence() {
    let content = run(&file("/Span 5 BDC EMC /Span BDC EMC", "", &[]));
    assert_eq!(
        kinds(&content),
        [
            DiagnosticKind::InvalidOperands,
            DiagnosticKind::InvalidOperands
        ]
    );
    assert_eq!(content.marked_content.len(), 2);
}
