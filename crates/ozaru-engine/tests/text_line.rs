use ozaru_engine::text::{TextLine, characters, lines};

mod common;
use common::{first_page, one_page};

fn read(content: &str) -> Vec<TextLine> {
    lines(characters(&first_page(&one_page(content, ""))))
}

fn texts(lines: &[TextLine]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.chars.iter().map(|c| c.text.as_str()).collect())
        .collect()
}

#[test]
fn each_baseline_is_a_line() {
    let lines = read("BT /F1 10 Tf 72 700 Td (First) Tj 0 -12 Td (Second) Tj T* ET");
    assert_eq!(texts(&lines), ["First", "Second"]);
    assert_eq!(lines[1].start(), (72.0, 688.0));
    assert_eq!(lines[0].direction(), (1.0, 0.0));
    assert_eq!(lines[0].size(), 10.0);
}

#[test]
fn pieces_on_the_same_baseline_are_one_line() {
    // The cells of a table row, each in its own text object.
    let lines = read(
        "BT /F1 10 Tf 72 700 Td (Name) Tj ET BT /F1 10 Tf 200 700 Td (Price) Tj ET \
         BT /F1 10 Tf 72 682 Td (Tea) Tj ET BT /F1 10 Tf 200 682 Td (3) Tj ET",
    );
    assert_eq!(texts(&lines), ["NamePrice", "Tea3"]);
}

#[test]
fn superscripts_and_subscripts_stay_on_their_line() {
    let lines = read(
        "BT /F1 10 Tf 72 700 Td (x) Tj 4 Ts /F1 6 Tf (2) Tj -2 Ts (i) Tj 0 Ts /F1 10 Tf (+1) Tj ET",
    );
    assert_eq!(texts(&lines), ["x2i+1"]);
}

#[test]
fn a_change_of_direction_starts_a_line() {
    let lines =
        read("BT /F1 10 Tf 72 700 Td (Across) Tj ET BT /F1 10 Tf 0 1 -1 0 300 700 Tm (Up) Tj ET");
    assert_eq!(texts(&lines), ["Across", "Up"]);
    assert_eq!(lines[1].direction(), (0.0, 1.0));
}

#[test]
fn rotated_lines_are_lines_too() {
    // Two lines running up the page, one to the left of the other.
    let lines = read("BT /F1 10 Tf 0 1 -1 0 300 100 Tm (One) Tj 0 -12 Td (Two) Tj ET");
    assert_eq!(texts(&lines), ["One", "Two"]);
    // The second line is below the first, seen along its direction.
    assert!((lines[0].height_of(lines[1].start()) + 12.0).abs() < 1e-9);
}

#[test]
fn no_text_is_no_lines() {
    assert!(read("q 1 0 0 1 5 5 cm Q").is_empty());
}
