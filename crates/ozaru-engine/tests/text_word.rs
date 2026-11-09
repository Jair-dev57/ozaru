use ozaru_engine::document::Rectangle;
use ozaru_engine::text::{characters, line_text, lines, page_words};

mod common;
use common::{first_page, one_page};

fn read(content: &str) -> Vec<String> {
    lines(characters(&first_page(&one_page(content, ""))))
        .iter()
        .map(line_text)
        .collect()
}

#[test]
fn painted_spaces_are_kept_as_they_are() {
    assert_eq!(
        read("BT /F1 10 Tf (one two  three) Tj ET"),
        ["one two  three"]
    );
}

#[test]
fn tj_gaps_become_spaces_and_kerning_does_not() {
    // -250 is a quarter of an em: a word gap. -100 and 80 are kerning.
    assert_eq!(
        read("BT /F1 10 Tf [(Kern) -250 (ing) -100 (TJ) 80 (s)] TJ ET"),
        ["Kern ingTJs"]
    );
}

#[test]
fn gaps_between_text_objects_become_spaces() {
    assert_eq!(
        read("BT /F1 10 Tf 72 700 Td (Name) Tj ET BT /F1 10 Tf 200 700 Td (Price) Tj ET"),
        ["Name Price"]
    );
    // Pieces that touch are one word.
    assert_eq!(
        read("BT /F1 10 Tf 72 700 Td (Hel) Tj ET BT /F1 10 Tf 85.56 700 Td (lo) Tj ET"),
        ["Hello"]
    );
}

#[test]
fn the_gap_is_measured_in_ems() {
    // The same 3 point gap is a space at 10 points and kerning at 30.
    assert_eq!(read("BT /F1 10 Tf [(a) -300 (b)] TJ ET"), ["a b"]);
    assert_eq!(read("BT /F1 30 Tf [(a) -100 (b)] TJ ET"), ["ab"]);
}

#[test]
fn a_gap_after_a_space_adds_nothing() {
    assert_eq!(read("BT /F1 10 Tf [(a ) -500 (b)] TJ ET"), ["a b"]);
}

#[test]
fn rotated_and_scaled_text_has_its_spaces_too() {
    assert_eq!(
        read("q 2 0 0 2 0 0 cm BT /F1 10 Tf 0 1 -1 0 300 100 Tm [(up) -300 (here)] TJ ET Q"),
        ["up here"]
    );
}

#[test]
fn wide_character_spacing_spreads_the_letters() {
    // Tc of a third of an em: each letter stands alone, as readers show it.
    assert_eq!(read("BT /F1 10 Tf 3.3 Tc (ab) Tj ET"), ["a b"]);
}

fn words(content: &str) -> Vec<(String, [f64; 4])> {
    let page = first_page(&one_page(content, ""));
    page_words(&page, &Rectangle::new(0.0, 0.0, 612.0, 792.0))
        .into_iter()
        .flatten()
        .map(|word| {
            let b = word.bbox;
            let rounded = [b.left, b.bottom, b.right, b.top].map(|v| (v * 100.0).round() / 100.0);
            (word.text, rounded)
        })
        .collect()
}

#[test]
fn words_are_split_at_spaces_and_have_boxes() {
    // Helvetica 10: H 7.22, i 2.22, space 2.78; up 7.18, down 2.07.
    assert_eq!(
        words("BT /F1 10 Tf 100 700 Td (Hi  Hi) Tj ET"),
        [
            ("Hi".to_string(), [100.0, 697.93, 109.44, 707.18]),
            ("Hi".to_string(), [115.0, 697.93, 124.44, 707.18]),
        ]
    );
}

#[test]
fn gaps_split_words_and_kerning_does_not() {
    let found: Vec<String> = words("BT /F1 10 Tf [(Kern) -250 (ing) -100 (TJ)] TJ ET")
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert_eq!(found, ["Kern", "ingTJ"]);
}

#[test]
fn words_come_line_by_line() {
    let page = first_page(&one_page(
        "BT /F1 10 Tf 72 700 Td (one two) Tj 0 -12 Td (   ) Tj 0 -12 Td (three) Tj ET",
        "",
    ));
    let lines: Vec<Vec<String>> = page_words(&page, &Rectangle::new(0.0, 0.0, 612.0, 792.0))
        .into_iter()
        .map(|line| line.into_iter().map(|word| word.text).collect())
        .collect();
    assert_eq!(lines, [vec!["one", "two"], vec!["three"]]);
}

#[test]
fn a_word_split_across_strings_is_one_word_with_one_box() {
    let found = words("BT /F1 10 Tf 100 700 Td (Hel) Tj (lo) Tj ET");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "Hello");
    assert_eq!(found[0].1[0], 100.0);
}

#[test]
fn text_painted_further_back_on_the_line_is_another_word() {
    // The end of the line is painted first, as some footers are.
    assert_eq!(
        read("BT /F1 10 Tf 300 700 Td (>) Tj -200 0 Td (Page) Tj ET"),
        ["> Page"]
    );
    // An accent painted back over its letter is part of the word.
    assert_eq!(
        read("BT /F1 10 Tf 100 700 Td (e) Tj 1 0 Td (') Tj ET"),
        ["e’"]
    );
}
