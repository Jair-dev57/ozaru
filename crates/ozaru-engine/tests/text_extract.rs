use ozaru_engine::Document;
use ozaru_engine::document::Rectangle;
use ozaru_engine::text::{PageProblem, document_text, page_text};

mod common;
use common::{file_with, first_page, one_page, stream_object};

fn text(content: &str) -> String {
    page_text(
        &first_page(&one_page(content, "")),
        &Rectangle::new(0.0, 0.0, 612.0, 792.0),
    )
}

// Lines of 10 point text at the given heights, from x = 72.
fn at(lines: &[(f64, &str)]) -> String {
    let mut content = String::from("BT /F1 10 Tf ");
    for (y, line) in lines {
        content.push_str(&format!("1 0 0 1 72 {y} Tm ({line}) Tj "));
    }
    content.push_str("ET");
    content
}

#[test]
fn lines_are_joined_by_line_feeds() {
    assert_eq!(
        text(&at(&[(700.0, "one"), (688.0, "two"), (676.0, "three")])),
        "one\ntwo\nthree"
    );
}

#[test]
fn a_wider_gap_starts_a_paragraph() {
    // Lines 12 apart, and 30 between the paragraphs.
    let content = at(&[(700.0, "a"), (688.0, "b"), (658.0, "c"), (646.0, "d")]);
    assert_eq!(text(&content), "a\nb\n\nc\nd");
}

#[test]
fn evenly_spread_lines_are_one_paragraph() {
    // Rows of a table 18 apart: wide for 10 point text, but usual on the page.
    let content = at(&[(700.0, "r1"), (682.0, "r2"), (664.0, "r3"), (646.0, "r4")]);
    assert_eq!(text(&content), "r1\nr2\nr3\nr4");
}

#[test]
fn a_heading_is_followed_by_a_line_feed() {
    let content = "BT /F1 18 Tf 72 760 Td (Title) Tj ET BT /F1 12 Tf 72 730 Td (Body one) Tj \
                   0 -20 Td (Body two) Tj ET";
    assert_eq!(text(content), "Title\nBody one\nBody two");
}

#[test]
fn a_new_column_starts_a_paragraph() {
    let content = at(&[(700.0, "left 1"), (688.0, "left 2")])
        + " "
        + &at(&[(700.0, "right 1"), (688.0, "right 2")]).replace(" 72 ", " 320 ");
    assert_eq!(text(&content), "left 1\nleft 2\n\nright 1\nright 2");
}

#[test]
fn turned_text_is_its_own_line() {
    let content =
        "BT /F1 10 Tf 72 700 Td (across) Tj ET BT /F1 10 Tf 0 1 -1 0 300 100 Tm (up) Tj ET";
    assert_eq!(text(content), "across\nup");
}

#[test]
fn whitespace_at_the_ends_and_blank_lines_go() {
    assert_eq!(
        text(&at(&[(700.0, "a  "), (688.0, "   "), (676.0, "  b")])),
        "a\nb"
    );
}

#[test]
fn text_outside_the_visible_area_is_left_out() {
    let content = at(&[(700.0, "inside"), (900.0, "above"), (-50.0, "below")])
        + " BT /F1 10 Tf -100 400 Td (left) Tj 800 0 Td (right) Tj ET"
        // A character across the edge is kept, as viewers show part of it,
        // and those past it aren't.
        + " BT /F1 10 Tf 608 300 Td (edge) Tj ET";
    let text = text(&content);
    assert_eq!(text.split_whitespace().collect::<Vec<_>>(), ["inside", "e"]);
}

#[test]
fn pages_with_problems_say_so() {
    let file = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 9 0 R >> >> /Contents 6 0 R >>"
                .to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>".to_string(),
        ),
        (9, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
        (6, stream_object("", "BT /F1 10 Tf 72 700 Td (first) Tj ET")),
        (7, stream_object("/Filter /JBIG2Decode", "xx")),
    ]);
    // Page 5 0 R doesn't exist, so it isn't a page: tests/page_error.pdf has
    // a damaged one.
    let mut document = Document::open(&file).unwrap();
    let pages = document_text(&mut document);
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert_eq!(pages[0].text, "first");
    assert_eq!(pages[0].problem, None);
    assert_eq!(pages[1].text, "");
    assert_eq!(pages[1].problem, Some(PageProblem::UnreadableContent));
}

#[test]
fn the_sample_report_reads_as_ozaru_did() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = std::fs::read(root.join("docs/assets/report.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    let pages: Vec<String> = document_text(&mut document)
        .into_iter()
        .map(|p| p.text)
        .collect();
    assert_eq!(
        pages,
        [
            "Quarterly Report\nPrepared by the ozaru team.\nThis sample document has three pages.",
            "Sales\nSales grew by 12% this quarter.\nThe café in São Paulo had its best month.",
            "Next Steps\nOpen two new stores.\nThank you for reading.",
        ]
    );
}

#[test]
fn a_damaged_page_has_no_text() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = std::fs::read(root.join("tests/page_error.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    let pages = document_text(&mut document);
    assert_eq!(pages[2].problem, Some(PageProblem::Damaged));
    assert_eq!(pages[2].text, "");
    assert!(pages[0].text.starts_with("Factura de prueba"));
}
