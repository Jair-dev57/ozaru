use std::sync::Arc;

use ozaru_engine::base::{DiagnosticKind, Limits};
use ozaru_engine::content::{Glyph, Interpreter, PageContent, TextRenderingMode};
use ozaru_engine::{Document, OpenOptions};

mod common;
use common::{file_with, stream_object};

// Helvetica, a standard 14 font whose widths come from Adobe's metrics: H 722,
// e 556, l 222, o 556, space 278, A 667, B 667.
const HELVETICA: &str = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";

// A composite font with two-byte codes: every CID is 500 wide, but CID 32 is
// 250.
const COMPOSITE: &str = "<< /Type /Font /Subtype /Type0 /BaseFont /Noto /Encoding /Identity-H \
     /DescendantFonts [<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Noto \
     /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
     /DW 500 /W [32 [250]] >>] >>";

// A one-page file whose content is `content`, with Helvetica as /F1, the
// composite font as /F2, and `resources` added to its resources.
fn file(content: &str, resources: &str) -> Vec<u8> {
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 4 0 R /F2 5 0 R >> {resources} >> /Contents 6 0 R >>"
            ),
        ),
        (4, HELVETICA.to_string()),
        (5, COMPOSITE.to_string()),
        (6, stream_object("", content)),
    ])
}

fn run_with(file: &[u8], limits: Limits) -> PageContent {
    let options = OpenOptions {
        limits,
        ..OpenOptions::default()
    };
    let mut document = Document::open_with(file, options).unwrap();
    let page = document.page(0).unwrap().clone();
    Interpreter::new(limits).run_page(document.resolver(), &page)
}

fn run(content: &str) -> PageContent {
    run_with(&file(content, ""), Limits::default())
}

fn kinds(content: &PageContent) -> Vec<DiagnosticKind> {
    content.diagnostics.iter().map(|d| d.kind()).collect()
}

fn text(glyphs: &[Glyph]) -> String {
    glyphs.iter().filter_map(|g| g.text.as_deref()).collect()
}

fn origins(glyphs: &[Glyph]) -> Vec<(f64, f64)> {
    glyphs
        .iter()
        .map(|g| {
            let (x, y) = g.origin();
            ((x * 1000.0).round() / 1000.0, (y * 1000.0).round() / 1000.0)
        })
        .collect()
}

#[test]
fn tj_places_each_glyph_after_the_last() {
    let content = run("BT /F1 10 Tf 100 700 Td (Hello) Tj ET");
    assert!(content.diagnostics.is_empty(), "{:?}", kinds(&content));
    assert_eq!(text(&content.glyphs), "Hello");
    assert_eq!(
        origins(&content.glyphs),
        [
            (100.0, 700.0),
            (107.22, 700.0),
            (112.78, 700.0),
            (115.0, 700.0),
            (117.22, 700.0)
        ]
    );
    let first = &content.glyphs[0];
    assert_eq!(first.font_size, 10.0);
    assert_eq!(first.width, 0.722);
    assert_eq!(first.font.base_font().unwrap().as_bytes(), b"Helvetica");
    assert_eq!(first.rendering_mode, TextRenderingMode::Fill);
    let (x, y) = first.end();
    assert!((x - 107.22).abs() < 1e-9 && y == 700.0);
}

#[test]
fn spacing_and_scaling_move_the_glyphs() {
    // Tc adds 1 after each glyph and Tw 5 after the space, and Tz 50 halves
    // everything horizontally.
    let content = run("BT /F1 10 Tf 1 Tc 5 Tw 50 Tz (H H) Tj ET");
    // H: (7.22 + 1) / 2 = 4.11; space: (2.78 + 1 + 5) / 2 = 4.39.
    assert_eq!(
        origins(&content.glyphs),
        [(0.0, 0.0), (4.11, 0.0), (8.5, 0.0)]
    );
    // The rendering matrix is squeezed too.
    assert_eq!(content.glyphs[0].matrix.a, 5.0);
    assert_eq!(content.glyphs[0].matrix.d, 10.0);
}

#[test]
fn word_spacing_skips_two_byte_codes() {
    // Code 0x0020 in a two-byte font isn't a space for Tw.
    let content = run("BT /F2 10 Tf 100 Tw <00200041> Tj ET");
    assert_eq!(origins(&content.glyphs), [(0.0, 0.0), (2.5, 0.0)]);
    assert_eq!(content.glyphs[1].code.value, 0x41);
    assert_eq!(content.glyphs[1].code.length, 2);
}

#[test]
fn tj_adjustments_move_the_next_glyph() {
    // -1000 thousandths of 10 points move B 10 points right, 500 move it back.
    let content = run("BT /F1 10 Tf [(A) -1000 (B) 500 (A)] TJ ET");
    assert_eq!(
        origins(&content.glyphs),
        [(0.0, 0.0), (16.67, 0.0), (18.34, 0.0)]
    );
    assert_eq!(text(&content.glyphs), "ABA");
}

#[test]
fn quotes_go_to_the_next_line() {
    let content = run("BT /F1 10 Tf 12 TL 50 500 Td (A) ' (B) ' 3 1 (A B) \" ET");
    let origins = origins(&content.glyphs);
    assert_eq!(origins[0], (50.0, 488.0));
    assert_eq!(origins[1], (50.0, 476.0));
    assert_eq!(origins[2], (50.0, 464.0));
    // `"` set Tc to 1 and Tw to 3.
    assert_eq!(origins[3], (57.67, 464.0));
    assert_eq!(origins[4], (64.45, 464.0));
}

#[test]
fn td_with_capitals_sets_the_leading() {
    let content = run("BT /F1 10 Tf 100 700 Td 0 -20 TD (A) Tj T* (B) Tj ET");
    assert_eq!(origins(&content.glyphs), [(100.0, 680.0), (100.0, 660.0)]);
}

#[test]
fn cm_moves_the_text_and_q_restores_it() {
    let content =
        run("q 2 0 0 2 50 50 cm BT /F1 10 Tf 10 10 Td (A) Tj ET Q BT /F1 10 Tf 10 10 Td (B) Tj ET");
    assert_eq!(origins(&content.glyphs), [(70.0, 70.0), (10.0, 10.0)]);
    assert_eq!(content.glyphs[0].matrix.a, 20.0);
}

#[test]
fn tm_can_rotate_the_text() {
    let content = run("BT /F1 10 Tf 0 1 -1 0 300 100 Tm (AB) Tj ET");
    assert_eq!(origins(&content.glyphs), [(300.0, 100.0), (300.0, 106.67)]);
}

#[test]
fn rise_lifts_the_glyphs() {
    let content = run("BT /F1 10 Tf 100 100 Td 3 Ts (A) Tj ET");
    assert_eq!(origins(&content.glyphs), [(100.0, 103.0)]);
}

#[test]
fn the_rendering_mode_is_kept_with_each_glyph() {
    let content = run("BT /F1 10 Tf 3 Tr (A) Tj 0 Tr (B) Tj ET");
    assert_eq!(
        content.glyphs[0].rendering_mode,
        TextRenderingMode::Invisible
    );
    assert_eq!(content.glyphs[1].rendering_mode, TextRenderingMode::Fill);
    let content = run("BT /F1 10 Tf 9 Tr ET");
    assert_eq!(kinds(&content), [DiagnosticKind::InvalidOperands]);
}

#[test]
fn the_text_state_is_saved_with_q() {
    let content = run("BT /F1 10 Tf q /F2 20 Tf 2 Tc Q (A) Tj ET");
    let glyph = &content.glyphs[0];
    assert_eq!(glyph.font_size, 10.0);
    assert_eq!(glyph.font.base_font().unwrap().as_bytes(), b"Helvetica");
}

#[test]
fn a_font_that_is_missing_shows_nothing() {
    let content = run("BT /F9 10 Tf (A) Tj ET");
    assert!(content.glyphs.is_empty());
    assert_eq!(
        kinds(&content),
        [DiagnosticKind::MissingFont, DiagnosticKind::MissingFont]
    );
    let content = run("BT (A) Tj ET");
    assert_eq!(kinds(&content), [DiagnosticKind::MissingFont]);
}

#[test]
fn gs_can_set_the_font() {
    let file = file(
        "BT /G1 gs (A) Tj ET",
        "/ExtGState << /G1 << /Type /ExtGState /Font [4 0 R 14] >> >>",
    );
    let content = run_with(&file, Limits::default());
    assert_eq!(content.glyphs[0].font_size, 14.0);
    assert!(content.diagnostics.is_empty());
    let content = run("/G9 gs");
    assert_eq!(kinds(&content), [DiagnosticKind::MissingResource]);
}

#[test]
fn unknown_operators_are_reported_outside_bx() {
    let content = run("1 2 foo BX 3 bar EX");
    assert_eq!(kinds(&content), [DiagnosticKind::UnknownOperator]);
}

#[test]
fn operators_with_wrong_operands_are_ignored() {
    let content = run("BT /F1 10 Tf 5 Td (A) Tj (B) Td ET");
    assert_eq!(
        kinds(&content),
        [
            DiagnosticKind::InvalidOperands,
            DiagnosticKind::InvalidOperands
        ]
    );
    assert_eq!(origins(&content.glyphs), [(0.0, 0.0)]);
    // Extra operands, left by damaged content, are ignored silently.
    let content = run("BT /F1 10 Tf 1 2 3 Td (A) Tj ET");
    assert!(content.diagnostics.is_empty());
    assert_eq!(origins(&content.glyphs), [(2.0, 3.0)]);
}

#[test]
fn unbalanced_states_are_reported() {
    let content = run("Q ET BT BT ET");
    assert_eq!(
        kinds(&content),
        [
            DiagnosticKind::UnbalancedRestore,
            DiagnosticKind::UnbalancedTextObject,
            DiagnosticKind::UnbalancedTextObject
        ]
    );
}

#[test]
fn text_outside_a_text_object_is_shown_anyway() {
    let content = run("/F1 10 Tf 5 5 Td (A) Tj (B) Tj");
    assert_eq!(text(&content.glyphs), "AB");
    assert_eq!(kinds(&content), [DiagnosticKind::TextOutsideTextObject]);
}

#[test]
fn contents_can_be_split_between_streams() {
    let file = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
             /Contents [6 0 R 7 0 R] >>"
                .to_string(),
        ),
        (4, HELVETICA.to_string()),
        (6, stream_object("", "BT /F1 10 Tf 100 700")),
        (7, stream_object("", "Td (A) Tj ET")),
    ]);
    let content = run_with(&file, Limits::default());
    assert_eq!(origins(&content.glyphs), [(100.0, 700.0)]);
    assert!(content.diagnostics.is_empty());
}

#[test]
fn diagnostics_are_reported_at_their_stream() {
    let file = file("1 2 foo", "");
    let content = run_with(&file, Limits::default());
    let offset = content.diagnostics.iter().next().unwrap().offset();
    let object = file.windows(7).position(|w| w == b"6 0 obj").unwrap();
    assert_eq!(offset, object);
}

#[test]
fn the_number_of_operations_is_limited() {
    let limits = Limits {
        max_content_operations: 5,
        ..Limits::default()
    };
    let content = run_with(&file("BT /F1 10 Tf (Hello) Tj ET", ""), limits);
    // BT, Tf and Tj count, and two glyphs.
    assert_eq!(text(&content.glyphs), "He");
    assert_eq!(kinds(&content), [DiagnosticKind::ContentLimitReached]);
}

#[test]
fn fonts_are_read_once_for_every_page() {
    let file = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 \
             /Resources << /Font << /F1 4 0 R >> >> >>"
                .to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".to_string(),
        ),
        (4, HELVETICA.to_string()),
        (
            5,
            "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".to_string(),
        ),
        (6, stream_object("", "BT /F1 10 Tf (A) Tj ET")),
    ]);
    let mut document = Document::open(&file).unwrap();
    let pages = document.pages().to_vec();
    let mut interpreter = Interpreter::new(Limits::default());
    let first = interpreter.run_page(document.resolver(), &pages[0]);
    let second = interpreter.run_page(document.resolver(), &pages[1]);
    assert!(Arc::ptr_eq(&first.glyphs[0].font, &second.glyphs[0].font));
}

#[test]
fn a_page_without_contents_is_empty() {
    let file = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, "<< /Type /Page /Parent 2 0 R >>".to_string()),
    ]);
    let content = run_with(&file, Limits::default());
    assert!(content.glyphs.is_empty());
    assert!(content.diagnostics.is_empty());
}
