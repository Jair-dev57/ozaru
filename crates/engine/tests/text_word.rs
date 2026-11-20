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

// Glyphs at the given x, 10 points apart in size, each standing for the text
// of its /ActualText.
fn glyphs_at(glyphs: &[(f64, &str)]) -> String {
    let mut content = String::from("BT /F1 10 Tf ");
    for (x, text) in glyphs {
        let utf16: String = text
            .encode_utf16()
            .map(|unit| format!("{unit:04X}"))
            .collect();
        content.push_str(&format!(
            "1 0 0 1 {x} 700 Tm /Span << /ActualText <FEFF{utf16}> >> BDC (o) Tj EMC "
        ));
    }
    content.push_str("ET");
    content
}

#[test]
fn hebrew_painted_from_left_to_right_reads_from_right_to_left() {
    // שלום עולם as it is seen: its first letter on the right.
    let seen = [
        (100.0, "ם"),
        (106.0, "ל"),
        (112.0, "ו"),
        (118.0, "ע"),
        (130.0, "ם"),
        (136.0, "ו"),
        (142.0, "ל"),
        (148.0, "ש"),
    ];
    assert_eq!(read(&glyphs_at(&seen)), ["שלום עולם"]);
    // Painted in reading order, from right to left, it reads the same.
    let mut painted = seen;
    painted.reverse();
    assert_eq!(read(&glyphs_at(&painted)), ["שלום עולם"]);
}

#[test]
fn latin_words_and_numbers_keep_their_order_in_hebrew() {
    // "שלום ok 12" as the Bidirectional Algorithm lays it out: the Hebrew
    // word on the right, and "ok 12" on its left, from left to right.
    let seen = [
        (100.0, "o"),
        (106.0, "k"),
        (118.0, "1"),
        (124.0, "2"),
        (136.0, "ם"),
        (142.0, "ו"),
        (148.0, "ל"),
        (154.0, "ש"),
    ];
    let words: Vec<String> = read(&glyphs_at(&seen));
    assert_eq!(words, ["שלום ok 12"]);
}

#[test]
fn lines_without_right_to_left_letters_keep_the_painted_order() {
    let painted = [(130.0, "b"), (100.0, "a")];
    // Painted backwards and more than an em apart: two words, as painted.
    assert_eq!(read(&glyphs_at(&painted)), ["b a"]);
}
