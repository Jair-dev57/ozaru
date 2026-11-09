use ozaru_engine::document::Rectangle;
use ozaru_engine::text::{page_blocks, page_text, page_words};

mod common;
use common::{file_with, first_page, stream_object};

// A composite font that writes vertically, with Identity-V: each glyph is an
// em tall and half an em wide, its vertical origin 880 units up and in the
// middle of it. Codes 1 to 9 stand for the CJK characters of TEXT.
const VERTICAL: &str = "<< /Type /Font /Subtype /Type0 /BaseFont /Vertical /Encoding /Identity-V \
     /ToUnicode 5 0 R /DescendantFonts [<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Vertical \
     /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /DW 1000 >>] >>";

const TEXT: [char; 9] = ['日', '本', '語', 'の', '縦', '書', 'き', 'で', 'す'];

fn to_unicode() -> String {
    let mut map = String::from(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
         1 begincodespacerange <0000> <FFFF> endcodespacerange 9 beginbfchar ",
    );
    for (i, c) in TEXT.iter().enumerate() {
        map.push_str(&format!("<{:04X}> <{:04X}> ", i + 1, *c as u32));
    }
    map.push_str("endbfchar endcmap CMapName currentdict /CMap defineresource pop end end");
    stream_object("", &map)
}

fn page(content: &str) -> ozaru_engine::content::PageContent {
    first_page(&file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /V 4 0 R >> >> /Contents 6 0 R >>"
                .to_string(),
        ),
        (4, VERTICAL.to_string()),
        (5, to_unicode()),
        (6, stream_object("", content)),
    ]))
}

const VISIBLE: Rectangle = Rectangle {
    left: 0.0,
    bottom: 0.0,
    right: 612.0,
    top: 792.0,
};

// Columns of codes, each starting at the top at x, 12 points apart.
fn columns(columns: &[(f64, &str)]) -> String {
    let mut content = String::new();
    for (x, codes) in columns {
        content.push_str(&format!("BT /V 10 Tf {x} 700 Td <{codes}> Tj ET "));
    }
    content
}

#[test]
fn a_column_of_vertical_text_is_a_line_read_downward() {
    let text = page_text(&page(&columns(&[(300.0, "000100020003")])), &VISIBLE);
    assert_eq!(text, "日本語");
}

#[test]
fn columns_are_read_from_right_to_left() {
    // The right column first, as writers paint them.
    let content = columns(&[
        (300.0, "000100020003"),
        (288.0, "000400050006"),
        (276.0, "000700080009"),
    ]);
    assert_eq!(
        page_text(&page(&content), &VISIBLE),
        "日本語\nの縦書\nきです"
    );
}

#[test]
fn a_column_further_left_than_usual_starts_a_paragraph() {
    let content = columns(&[
        (300.0, "000100020003"),
        (288.0, "00040005"),
        (240.0, "000700080009"),
    ]);
    assert_eq!(
        page_text(&page(&content), &VISIBLE),
        "日本語\nの縦\n\nきです"
    );
}

#[test]
fn vertical_words_and_blocks_have_boxes_down_the_page() {
    let content = page(&columns(&[
        (300.0, "000100020003"),
        (288.0, "000400050006"),
    ]));
    let words = page_words(&content, &VISIBLE);
    assert_eq!(words.len(), 2);
    let word = &words[0][0];
    assert_eq!(word.text, "日本語");
    // Half an em on each side of x = 300, and three ems down from the top.
    let b = word.bbox;
    let found = [b.left, b.bottom, b.right, b.top];
    let expected = [295.0, 670.0, 305.0, 700.0];
    assert!(
        found
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{found:?}"
    );
    let blocks = page_blocks(&content, &VISIBLE);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].lines.len(), 2);
}
