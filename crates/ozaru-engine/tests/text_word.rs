use ozaru_engine::text::{characters, line_text, lines};

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
