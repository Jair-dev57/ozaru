use super::glyph::TextChar;
use super::line::TextLine;
use crate::document::Rectangle;

// How wide a gap between two characters, in ems of the larger one, makes a
// space. Kerning, even tight `TJ` adjustments, stays well under it, and the
// space of most fonts is a quarter of an em or more.
const SPACE: f64 = 0.15;

/// Whether a space goes between `previous` and `next`, two characters of a
/// line running in `direction`: when neither is whitespace and the gap
/// between them, along the line, is at least 0.15 em. Many PDFs leave a gap,
/// often with a `TJ` adjustment, instead of painting a space.
pub fn needs_space(previous: &TextChar, next: &TextChar, direction: (f64, f64)) -> bool {
    if previous.is_whitespace() || next.is_whitespace() {
        return false;
    }
    let gap = direction.0 * (next.start.0 - previous.end.0)
        + direction.1 * (next.start.1 - previous.end.1);
    gap >= SPACE * previous.size.max(next.size)
}

/// The text of a line, with a space wherever [`needs_space`] finds one.
pub fn line_text(line: &TextLine) -> String {
    let direction = line.direction();
    let mut text = String::new();
    let mut previous: Option<&TextChar> = None;
    for character in &line.chars {
        if previous.is_some_and(|previous| needs_space(previous, character, direction)) {
            text.push(' ');
        }
        text.push_str(&character.text);
        previous = Some(character);
    }
    text
}

/// A word: characters of a line between two spaces, painted or found by
/// [`needs_space`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextWord {
    pub text: String,
    /// The box around its characters, in the default user space of the page.
    pub bbox: Rectangle,
}

/// The words of a line, in its order. Whitespace is left out.
pub fn line_words(line: &TextLine) -> Vec<TextWord> {
    let direction = line.direction();
    let mut words: Vec<TextWord> = Vec::new();
    let mut previous: Option<&TextChar> = None;
    for character in &line.chars {
        if character.is_whitespace() {
            previous = None;
            continue;
        }
        let joins = previous.is_some_and(|previous| !needs_space(previous, character, direction));
        match words.last_mut() {
            Some(word) if joins => {
                word.text.push_str(&character.text);
                word.bbox = word.bbox.union(&character.bbox);
            }
            _ => words.push(TextWord {
                text: character.text.clone(),
                bbox: character.bbox,
            }),
        }
        previous = Some(character);
    }
    words
}
