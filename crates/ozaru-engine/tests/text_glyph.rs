use ozaru_engine::text::{TextChar, characters};

mod common;
use common::{first_page, one_page};

fn read(content: &str) -> Vec<TextChar> {
    characters(&first_page(&one_page(content, "")))
}

fn texts(chars: &[TextChar]) -> Vec<&str> {
    chars.iter().map(|c| c.text.as_str()).collect()
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

#[test]
fn each_glyph_gives_its_text_and_place() {
    let chars = read("BT /F1 10 Tf 100 700 Td (Hi there) Tj ET");
    assert_eq!(texts(&chars), ["H", "i", " ", "t", "h", "e", "r", "e"]);
    let first = &chars[0];
    assert!(close(first.start, (100.0, 700.0)));
    assert!(close(first.end, (107.22, 700.0)));
    assert!(close(first.direction, (1.0, 0.0)));
    assert!((first.size - 10.0).abs() < 1e-9);
    assert!(chars[2].is_whitespace());
    assert!(!first.is_whitespace());
}

#[test]
fn the_size_includes_the_matrices() {
    let chars = read("q 2 0 0 2 0 0 cm BT /F1 10 Tf 0 1 -1 0 300 100 Tm (A) Tj ET Q");
    let a = &chars[0];
    assert!((a.size - 20.0).abs() < 1e-9);
    assert!(close(a.direction, (0.0, 1.0)));
    assert!(close(a.start, (600.0, 200.0)));
}

#[test]
fn actual_text_replaces_the_glyphs_of_its_sequence() {
    let chars = read(
        "BT /F1 10 Tf (a) Tj /Span << /ActualText (fi) >> BDC (XY) Tj /Span << /ActualText (no) >> BDC \
         (Z) Tj EMC EMC (b) Tj ET",
    );
    // The outermost /ActualText wins, once, from the first glyph to the last.
    assert_eq!(texts(&chars), ["a", "fi", "b"]);
    assert!(close(chars[1].start, (5.56, 0.0)));
    // X, Y and Z of Helvetica: 667 + 667 + 611.
    assert!(close(chars[1].end, (5.56 + 19.45, 0.0)));
}

#[test]
fn two_sequences_with_actual_text_in_a_row_stay_apart() {
    let chars = read(
        "BT /F1 10 Tf /Span << /ActualText (1) >> BDC (a) Tj EMC \
         /Span << /ActualText (2) >> BDC (b) Tj EMC ET",
    );
    assert_eq!(texts(&chars), ["1", "2"]);
}

#[test]
fn empty_actual_text_hides_the_glyphs() {
    let chars = read("BT /F1 10 Tf /Artifact << /ActualText () >> BDC (page 3) Tj EMC (x) Tj ET");
    assert_eq!(texts(&chars), ["x"]);
}

#[test]
fn glyphs_without_text_are_left_out() {
    // Code 0 has no glyph name in the standard encoding of Helvetica.
    let chars = read("BT /F1 10 Tf <00410042> Tj ET");
    assert_eq!(texts(&chars), ["A", "B"]);
}

#[test]
fn a_glyph_painted_over_itself_is_kept_once() {
    // Fake bold: each glyph twice, the second a hair to the right.
    let chars = read("BT /F1 10 Tf [(A) 667 -3 (A) -664 (B) 667 -3 (B)] TJ ET");
    assert_eq!(texts(&chars), ["A", "B"]);
    // A repeated letter is two letters.
    let chars = read("BT /F1 10 Tf (ll) Tj ET");
    assert_eq!(texts(&chars), ["l", "l"]);
}

#[test]
fn invisible_text_is_kept() {
    // The text of a scanned page, under its image.
    let chars = read("BT /F1 10 Tf 3 Tr (ok) Tj ET");
    assert_eq!(texts(&chars), ["o", "k"]);
}
