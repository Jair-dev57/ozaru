//! Spans, search, tables and Markdown.

use ozaru_engine::Document;
use ozaru_engine::text::{
    MarkdownStyle, Table, TableStrategy, TextBlock, TextMatch, TextSpan, document_blocks,
    document_blocks_colored, document_markdown, document_tables, page_search, page_spans,
};

mod common;
use common::{file_with, stream_object};

// A one-page file: F1 Helvetica, F2 Helvetica-Bold, F3 Helvetica-Oblique and
// F4 Courier.
fn page(content: &str) -> Vec<u8> {
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 8 0 R \
             /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R /F4 7 0 R >> >> >>"
                .to_string(),
        ),
        (
            4,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
        (
            5,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_string(),
        ),
        (
            6,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>".to_string(),
        ),
        (
            7,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_string(),
        ),
        (8, stream_object("", content)),
    ])
}

fn blocks_of(content: &str) -> Vec<TextBlock> {
    let data = page(content);
    let mut document = Document::open(&data).unwrap();
    document_blocks(&mut document).remove(0).blocks
}

fn spans(content: &str) -> Vec<TextSpan> {
    let data = page(content);
    let mut document = Document::open(&data).unwrap();
    page_spans(&document_blocks_colored(&mut document).remove(0).blocks)
}

fn search(content: &str, query: &str, case_sensitive: bool) -> Vec<TextMatch> {
    page_search(&blocks_of(content), query, case_sensitive)
}

fn tables(content: &str, strategy: TableStrategy) -> Vec<Table> {
    let data = page(content);
    let mut document = Document::open(&data).unwrap();
    document_tables(&mut document, strategy).remove(0).tables
}

fn markdown(content: &str) -> String {
    let data = page(content);
    let mut document = Document::open(&data).unwrap();
    document_markdown(&mut document, TableStrategy::Both)
        .remove(0)
        .text
}

fn texts(table: &Table) -> Vec<Vec<Option<&str>>> {
    table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.as_ref().map(|c| c.text.as_str()))
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Spans

#[test]
fn a_span_ends_where_the_font_size_or_color_changes() {
    let found = spans(
        "BT /F1 12 Tf 72 700 Td (Plain ) Tj /F2 12 Tf (bold) Tj /F1 12 Tf ( then ) Tj \
         1 0 0 rg (red) Tj 0 g /F1 20 Tf ( big) Tj /F3 20 Tf ( slanted) Tj ET",
    );
    let summary: Vec<_> = found
        .iter()
        .map(|s| (s.text.as_str(), s.size, s.color, s.bold, s.italic))
        .collect();
    assert_eq!(
        summary,
        [
            ("Plain ", 12.0, [0, 0, 0], false, false),
            ("bold", 12.0, [0, 0, 0], true, false),
            (" then ", 12.0, [0, 0, 0], false, false),
            ("red", 12.0, [255, 0, 0], false, false),
            (" big", 20.0, [0, 0, 0], false, false),
            (" slanted", 20.0, [0, 0, 0], false, true),
        ]
    );
    assert_eq!(found[1].font.as_deref(), Some("Helvetica-Bold"));
    assert_eq!((found[0].block, found[0].line), (0, 0));
    assert!(found[1].bbox.left > found[0].bbox.left);
}

#[test]
fn a_gap_of_more_than_an_em_starts_a_span() {
    let found = spans("BT /F1 10 Tf 72 700 Td (Cell one) Tj 100 0 Td (Cell two) Tj ET");
    let texts: Vec<_> = found.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, ["Cell one", "Cell two"]);
    // A word space doesn't.
    let found = spans("BT /F1 10 Tf 72 700 Td (Some words here) Tj ET");
    assert_eq!(found.len(), 1);
}

#[test]
fn lines_and_blocks_are_numbered() {
    let found =
        spans("BT /F1 10 Tf 72 700 Td (first) Tj 0 -12 Td (second) Tj 0 -60 Td (third) Tj ET");
    let places: Vec<_> = found
        .iter()
        .map(|s| (s.block, s.line, s.text.as_str()))
        .collect();
    assert_eq!(places, [(0, 0, "first"), (0, 1, "second"), (1, 0, "third")]);
}

#[test]
fn font_names_say_bold_and_italic() {
    use ozaru_engine::text::{is_bold_name, is_italic_name};
    for name in [
        "Arial-BoldMT",
        "ABCDEF+Helvetica-Bold",
        "CMBX10",
        "Roboto-Black",
        "Foo,Bold",
    ] {
        assert!(is_bold_name(name), "{name}");
    }
    for name in [
        "Arial-ItalicMT",
        "Helvetica-Oblique",
        "CMTI10",
        "Foo-It",
        "Foo,Italic",
    ] {
        assert!(is_italic_name(name), "{name}");
    }
    for name in ["Helvetica", "Courier", "Arial", "Bolder"] {
        assert!(!is_italic_name(name), "{name}");
    }
    assert!(!is_bold_name("Helvetica"));
}

// ---------------------------------------------------------------------------
// Search

#[test]
fn search_finds_each_place_with_its_box() {
    let content = "BT /F1 12 Tf 72 700 Td (The cat and the other cat) Tj ET";
    let found = search(content, "cat", true);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].text, "cat");
    assert_eq!(found[0].boxes.len(), 1);
    assert!(found[0].boxes[0].left > 72.0 && found[1].boxes[0].left > found[0].boxes[0].right);
    // The search is for text, not for words: "other" has "the" inside.
    assert_eq!(search(content, "the", true).len(), 2);
    assert_eq!(search(content, "the", false).len(), 3);
}

#[test]
fn search_ignores_case_unless_asked() {
    let content = "BT /F1 12 Tf 72 700 Td (Hello HELLO hello) Tj ET";
    assert_eq!(search(content, "HeLLo", false).len(), 3);
    assert_eq!(search(content, "HELLO", true).len(), 1);
    assert_eq!(search(content, "Hello", true)[0].text, "Hello");
}

#[test]
fn search_goes_over_lines_and_whitespace() {
    let content = "BT /F1 12 Tf 72 700 Td (the quick brown) Tj 0 -14 Td (fox jumps) Tj ET";
    let found = search(content, "brown  FOX", false);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "brown fox");
    // One box for each line.
    assert_eq!(found[0].boxes.len(), 2);
    assert!(found[0].boxes[0].top > found[0].boxes[1].top);
}

#[test]
fn search_finds_nothing_for_nothing() {
    let content = "BT /F1 12 Tf 72 700 Td (some text) Tj ET";
    assert!(search(content, "", false).is_empty());
    assert!(search(content, "   ", false).is_empty());
    assert!(search(content, "missing", false).is_empty());
    assert!(search("", "x", false).is_empty());
}

#[test]
fn search_does_not_overlap_its_matches() {
    let content = "BT /F1 12 Tf 72 700 Td (aaaa) Tj ET";
    assert_eq!(search(content, "aa", true).len(), 2);
}

// ---------------------------------------------------------------------------
// Tables with lines

// A grid of `rows` by `columns` cells of 60 by 20 points, its top left corner
// at (100, 600), drawn with stroked lines, with its text.
fn ruled(rows: usize, columns: usize, text: &[&[&str]]) -> String {
    let mut content = String::from("0.5 w ");
    for row in 0..=rows {
        let y = 600 - 20 * row;
        content.push_str(&format!("100 {y} m {} {y} l S ", 100 + 60 * columns));
    }
    for column in 0..=columns {
        let x = 100 + 60 * column;
        content.push_str(&format!("{x} 600 m {x} {} l S ", 600 - 20 * rows));
    }
    for (r, cells) in text.iter().enumerate() {
        for (c, cell) in cells.iter().enumerate() {
            let (x, y) = (105 + 60 * c, 586 - 20 * r);
            content.push_str(&format!("BT /F1 10 Tf {x} {y} Td ({cell}) Tj ET "));
        }
    }
    content
}

#[test]
fn a_grid_of_lines_is_a_table() {
    let content = ruled(3, 2, &[&["Name", "Qty"], &["Pens", "12"], &["Ink", "3"]]);
    let found = tables(&content, TableStrategy::Lines);
    assert_eq!(found.len(), 1);
    let table = &found[0];
    assert!(table.ruled);
    assert_eq!(
        texts(table),
        [
            vec![Some("Name"), Some("Qty")],
            vec![Some("Pens"), Some("12")],
            vec![Some("Ink"), Some("3")],
        ]
    );
    let bbox = table.bbox;
    assert_eq!(
        (bbox.left, bbox.top, bbox.right, bbox.bottom),
        (100.0, 600.0, 220.0, 540.0)
    );
    assert_eq!(table.rows[1][1].as_ref().unwrap().bbox.left, 160.0);
}

#[test]
fn rectangles_and_thin_filled_rules_make_tables_too() {
    // The outer frame as one rectangle, the inner rules as thin filled ones.
    let mut content = String::from("100 560 120 40 re S ");
    content.push_str("100 579.5 120 1 re f 159.5 560 1 40 re f ");
    content.push_str(
        "BT /F1 10 Tf 105 586 Td (a) Tj 60 0 Td (b) Tj -60 -20 Td (c) Tj 60 0 Td (d) Tj ET",
    );
    let found = tables(&content, TableStrategy::Lines);
    assert_eq!(found.len(), 1);
    assert_eq!(
        texts(&found[0]),
        [vec![Some("a"), Some("b")], vec![Some("c"), Some("d")]]
    );
}

#[test]
fn a_single_box_is_not_a_table() {
    let content = "100 500 200 100 re S BT /F1 10 Tf 110 560 Td (inside a box) Tj ET";
    assert!(tables(content, TableStrategy::Both).is_empty());
}

#[test]
fn filled_blocks_of_color_are_not_tables() {
    let content = "1 0 0 rg 100 500 100 100 re f 0 1 0 rg 200 500 100 100 re f";
    assert!(tables(content, TableStrategy::Lines).is_empty());
}

#[test]
fn lines_that_nearly_touch_still_make_cells() {
    // Each rule stops 1 point short of the frame.
    let content = "0.5 w 100 600 m 220 600 l S 100 560 m 220 560 l S 100 599 m 100 561 l S \
                   220 599 m 220 561 l S 100 580 m 219 580 l S 160 599 m 160 561 l S \
                   BT /F1 10 Tf 105 586 Td (x) Tj 60 0 Td (y) Tj -60 -20 Td (z) Tj ET";
    let found = tables(content, TableStrategy::Lines);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rows.len(), 2);
}

#[test]
fn a_cell_that_spans_columns_leaves_none_after_it() {
    // Two rows of two columns, but the second row has no middle rule.
    let content = "0.5 w 100 600 m 220 600 l S 100 580 m 220 580 l S 100 560 m 220 560 l S \
                   100 600 m 100 560 l S 220 600 m 220 560 l S 160 600 m 160 580 l S \
                   BT /F1 10 Tf 105 586 Td (A) Tj 60 0 Td (B) Tj -60 -20 Td (wide cell) Tj ET";
    let found = tables(content, TableStrategy::Lines);
    assert_eq!(found.len(), 1);
    let rows = texts(&found[0]);
    assert_eq!(rows[0], [Some("A"), Some("B")]);
    assert_eq!(rows[1], [Some("wide cell"), None]);
}

// ---------------------------------------------------------------------------
// Tables from text

fn aligned_rows(rows: &[(&str, &str, &str)]) -> String {
    let mut content = String::from("BT /F1 10 Tf ");
    let mut y = 700;
    for (a, b, c) in rows {
        content.push_str(&format!(
            "1 0 0 1 72 {y} Tm ({a}) Tj 1 0 0 1 200 {y} Tm ({b}) Tj 1 0 0 1 320 {y} Tm ({c}) Tj "
        ));
        y -= 14;
    }
    content.push_str("ET");
    content
}

#[test]
fn text_in_aligned_columns_is_a_table() {
    let content = aligned_rows(&[
        ("Item", "Price", "Notes"),
        ("Pen", "1.20", "blue"),
        ("Notebook", "4.50", "A5 ruled"),
        ("Stapler", "9.99", "metal"),
    ]);
    assert!(tables(&content, TableStrategy::Lines).is_empty());
    let found = tables(&content, TableStrategy::Text);
    assert_eq!(found.len(), 1);
    assert!(!found[0].ruled);
    assert_eq!(
        texts(&found[0]),
        [
            vec![Some("Item"), Some("Price"), Some("Notes")],
            vec![Some("Pen"), Some("1.20"), Some("blue")],
            vec![Some("Notebook"), Some("4.50"), Some("A5 ruled")],
            vec![Some("Stapler"), Some("9.99"), Some("metal")],
        ]
    );
}

#[test]
fn a_missing_cell_leaves_none() {
    let content = aligned_rows(&[
        ("Item", "Price", "Notes"),
        ("Pen", "1.20", ""),
        ("Stapler", "9.99", "metal"),
    ]);
    let found = tables(&content, TableStrategy::Text);
    assert_eq!(found.len(), 1);
    assert_eq!(texts(&found[0])[1], [Some("Pen"), Some("1.20"), None]);
}

#[test]
fn two_rows_are_not_enough_for_a_table_of_text() {
    let content = aligned_rows(&[("Item", "Price", "Notes"), ("Pen", "1.20", "blue")]);
    assert!(tables(&content, TableStrategy::Text).is_empty());
}

#[test]
fn columns_of_prose_are_not_a_table() {
    let mut content = String::from("BT /F1 10 Tf ");
    let left = [
        "the quick brown fox jumps over",
        "the lazy dog and then runs far",
        "away from the barking farm dogs",
    ];
    let right = [
        "while another story begins here",
        "with entirely different words in",
        "its own column of ordinary prose",
    ];
    for (i, (l, r)) in left.iter().zip(right).enumerate() {
        let y = 700 - 14 * i;
        content.push_str(&format!(
            "1 0 0 1 72 {y} Tm ({l}) Tj 1 0 0 1 320 {y} Tm ({r}) Tj "
        ));
    }
    content.push_str("ET");
    assert!(tables(&content, TableStrategy::Text).is_empty());
}

#[test]
fn a_list_with_bullets_is_not_a_table() {
    let content = "BT /F1 10 Tf 1 0 0 1 72 700 Tm (-) Tj 1 0 0 1 90 700 Tm (first item) Tj \
                   1 0 0 1 72 686 Tm (-) Tj 1 0 0 1 90 686 Tm (second item) Tj \
                   1 0 0 1 72 672 Tm (-) Tj 1 0 0 1 90 672 Tm (third item) Tj ET";
    assert!(tables(content, TableStrategy::Text).is_empty());
}

#[test]
fn lines_win_over_text_where_both_are_there() {
    let content = ruled(3, 2, &[&["Name", "Qty"], &["Pens", "12"], &["Ink", "3"]]);
    let found = tables(&content, TableStrategy::Both);
    assert_eq!(found.len(), 1);
    assert!(found[0].ruled);
}

// ---------------------------------------------------------------------------
// Markdown

#[test]
fn markdown_has_headings_emphasis_and_paragraphs() {
    let content = "BT /F2 24 Tf 72 720 Td (Big title) Tj ET \
         BT /F2 16 Tf 72 680 Td (Section) Tj ET \
         BT /F1 11 Tf 72 650 Td (Some ) Tj /F2 11 Tf (strong) Tj /F1 11 Tf ( and ) Tj /F3 11 Tf (slanted) Tj \
         /F1 11 Tf ( text that goes on for a while so that the body size is clear.) Tj \
         0 -14 Td (A second line of the same paragraph keeps going on and on and on.) Tj ET";
    let text = markdown(content);
    assert_eq!(
        text,
        "# Big title\n\n## Section\n\nSome **strong** and *slanted* text that goes on for a while \
         so that the body size is clear. A second line of the same paragraph keeps going on and on and on."
    );
}

#[test]
fn markdown_lists_use_the_markers_of_the_text() {
    let content = "BT /F1 11 Tf 72 700 Td (An introduction that is long enough to be the body of the page.) Tj ET \
         BT /F1 11 Tf 72 660 Td (- first point) Tj 0 -14 Td (- second point continues) Tj \
         0 -14 Td (onto a second line) Tj 0 -14 Td (3. numbered) Tj ET";
    let text = markdown(content);
    assert!(
        text.contains("- first point\n- second point continues onto a second line\n3. numbered"),
        "{text}"
    );
}

#[test]
fn markdown_joins_words_cut_at_the_end_of_a_line() {
    let content = "BT /F1 11 Tf 72 700 Td (The inter-) Tj 0 -14 Td (national rules) Tj 0 -14 Td (are well-) Tj 0 -14 Td (Known) Tj ET";
    let text = markdown(content);
    // A hyphen before a lowercase letter is a cut word; before a capital it stays.
    assert_eq!(text, "The international rules are well-Known");
}

#[test]
fn markdown_escapes_what_would_be_syntax() {
    let text = markdown("BT /F1 11 Tf 72 700 Td (2 * 3 = 6 and snake_case) Tj ET");
    assert_eq!(text, "2 \\* 3 = 6 and snake\\_case");
    let text = markdown("BT /F1 11 Tf 72 700 Td (# not a heading) Tj ET");
    assert_eq!(text, "\\# not a heading");
}

#[test]
fn markdown_puts_tables_where_they_are_and_not_their_text_again() {
    let mut content = String::from(
        "BT /F2 18 Tf 72 720 Td (Report) Tj ET BT /F1 11 Tf 72 690 Td (Intro text for the report that is long enough.) Tj ET ",
    );
    content.push_str(&ruled(2, 2, &[&["Name", "Qty"], &["Pens", "12"]]));
    content.push_str(
        "BT /F1 11 Tf 72 400 Td (Closing words of the report that are long enough too.) Tj ET",
    );
    let text = markdown(&content);
    let table = "| Name | Qty |\n| --- | --- |\n| Pens | 12 |";
    assert!(text.contains(table), "{text}");
    assert_eq!(text.matches("Pens").count(), 1);
    let at = |needle: &str| text.find(needle).unwrap();
    assert!(
        at("Intro") < at("| Name") && at("| Name") < at("Closing"),
        "{text}"
    );
}

#[test]
fn markdown_escapes_pipes_and_breaks_in_cells() {
    let content = ruled(2, 2, &[&["a|b", "c"], &["d", "e"]]);
    let text = markdown(&content);
    assert!(text.contains("| a\\|b | c |"), "{text}");
}

#[test]
fn an_empty_page_is_empty_markdown() {
    assert_eq!(markdown(""), "");
}

#[test]
fn heading_levels_follow_the_sizes_of_the_document() {
    let blocks_by_page = [blocks_of(
        "BT /F1 30 Tf 72 700 Td (Huge) Tj ET BT /F1 20 Tf 72 640 Td (Large) Tj ET \
         BT /F1 12 Tf 72 600 Td (Body text that goes on long enough to be the common size of this page.) Tj ET",
    )];
    let pages: Vec<&[TextBlock]> = blocks_by_page.iter().map(Vec::as_slice).collect();
    let style = MarkdownStyle::of(&pages);
    assert_eq!(style.body, 12.0);
    assert_eq!(style.headings, [30.0, 20.0]);
}

// ---------------------------------------------------------------------------
// A real file

#[test]
fn a_real_report_is_read_as_mupdf_and_pdfplumber_read_it() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/text-structure.pdf");
    let data = std::fs::read(path).unwrap();
    let mut document = Document::open(&data).unwrap();
    // pdfplumber: one table with lines, these rows.
    let found = document_tables(&mut document, TableStrategy::Lines)
        .remove(0)
        .tables;
    assert_eq!(found.len(), 1);
    assert_eq!(
        texts(&found[0]),
        [
            vec![Some("Región"), Some("Q1"), Some("Q2"), Some("Total")],
            vec![Some("Norte"), Some("120"), Some("135"), Some("255")],
            vec![Some("Sur"), Some("98"), Some("101"), Some("199")],
            vec![Some("Este"), Some("143"), Some("150"), Some("293")],
        ]
    );
    // MuPDF: 55 spans, the first of them in Helvetica-Bold 18.
    let blocks = document_blocks(&mut document).remove(0).blocks;
    let found = page_spans(&blocks);
    assert_eq!(found.len(), 55);
    assert_eq!(found[0].text, "Informe Trimestral");
    assert_eq!(found[0].font.as_deref(), Some("Helvetica-Bold"));
    assert_eq!(found[0].size, 18.0);
    // MuPDF finds "ventas" twice, "Norte" twice, and "ZZZ" not at all.
    assert_eq!(page_search(&blocks, "ventas", false).len(), 2);
    assert_eq!(page_search(&blocks, "Norte", true).len(), 2);
    assert!(page_search(&blocks, "ZZZ", false).is_empty());
    let text = document_markdown(&mut document, TableStrategy::Both)
        .remove(0)
        .text;
    assert!(text.starts_with(
        "# Informe Trimestral\n\n# Resumen ejecutivo\n\nEste informe describe las **ventas**"
    ));
    assert!(text.contains(
        "## Puntos clave\n\n- Crecimiento sostenido en el **Norte**.\n- El Sur se mantiene estable."
    ));
    assert!(text.contains(
        "| Concepto | Monto | Notas |\n| --- | --- | --- |\n| Alquiler | 1200 | mensual |"
    ));
}
