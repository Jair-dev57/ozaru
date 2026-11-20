use super::glyph::TextChar;
use super::line::TextLine;
use crate::document::Rectangle;
use unicode_bidi::{BidiClass, Level, ParagraphBidiInfo, bidi_class};

// How wide a gap between two characters, in ems of the larger one, makes a
// space. Kerning, even tight `TJ` adjustments, stays well under it, and the
// space of most fonts is a quarter of an em or more.
const SPACE: f64 = 0.15;

/// Whether a space goes between `previous` and `next`, two characters of a
/// line running in `direction`: when neither is whitespace and the gap
/// between them, along the line, is at least 0.15 em. Many PDFs leave a gap,
/// often with a `TJ` adjustment, instead of painting a space.
///
/// It also goes where `next` starts more than an em behind where `previous`
/// starts, as when a PDF paints the end of a line before its start. An accent
/// painted over the letter before it starts within it, and so does a letter
/// of a word painted right to left.
pub fn needs_space(previous: &TextChar, next: &TextChar, direction: (f64, f64)) -> bool {
    if previous.is_whitespace() || next.is_whitespace() {
        return false;
    }
    let along = |from: (f64, f64), to: (f64, f64)| {
        direction.0 * (to.0 - from.0) + direction.1 * (to.1 - from.1)
    };
    let size = previous.size.max(next.size);
    along(previous.end, next.start) >= SPACE * size || along(previous.start, next.start) < -size
}

/// The text of a line, with a space wherever [`needs_space`] finds one, in
/// reading order: see [`reading_order`].
pub fn line_text(line: &TextLine) -> String {
    reading_order(line)
        .into_iter()
        .map(|piece| piece.map_or(" ", |character| character.text.as_str()))
        .collect()
}

/// A word: characters of a line between two spaces, painted or found by
/// [`needs_space`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextWord {
    pub text: String,
    /// The box around its characters, in the default user space of the page.
    pub bbox: Rectangle,
}

/// The words of a line, in reading order. Whitespace is left out.
pub fn line_words(line: &TextLine) -> Vec<TextWord> {
    let mut words: Vec<TextWord> = Vec::new();
    let mut joins = false;
    for piece in reading_order(line) {
        let Some(character) = piece.filter(|character| !character.is_whitespace()) else {
            joins = false;
            continue;
        };
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
        joins = true;
    }
    words
}

/// The characters of a line in reading order, with `None` for each space
/// that [`needs_space`] finds between two of them.
///
/// That is the order they were painted in, except on a line with letters of a
/// script written right to left, such as Arabic or Hebrew. Its characters are
/// taken from left to right, as they are seen, and put in reading order with
/// the Unicode Bidirectional Algorithm (UAX #9, rule L2): runs of right-to-left
/// text are turned around, and numbers and left-to-right words inside them
/// keep their order. The line reads right to left if most of its letters do.
pub fn reading_order(line: &TextLine) -> Vec<Option<&TextChar>> {
    let direction = line.direction();
    let right_to_left = line
        .chars
        .iter()
        .any(|c| c.text.chars().any(is_strong_right_to_left));
    let mut seen: Vec<&TextChar> = line.chars.iter().collect();
    if right_to_left {
        let along = |c: &TextChar| direction.0 * c.start.0 + direction.1 * c.start.1;
        seen.sort_by(|a, b| along(a).total_cmp(&along(b)));
    }
    let mut pieces: Vec<Option<&TextChar>> = Vec::with_capacity(seen.len());
    for character in seen {
        if let Some(Some(previous)) = pieces.last()
            && needs_space(previous, character, direction)
        {
            pieces.push(None);
        }
        pieces.push(Some(character));
    }
    if right_to_left {
        reorder(&mut pieces);
    }
    pieces
}

fn is_strong_right_to_left(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
}

// Puts `pieces`, seen from left to right, in reading order (UAX #9, rule L2).
// Each piece is classified by the first character of its text that has a
// strong direction, or else its first character, and an inferred space is a
// space.
fn reorder(pieces: &mut [Option<&TextChar>]) {
    let representative = |piece: &Option<&TextChar>| match piece {
        None => ' ',
        Some(character) => {
            let mut chars = character.text.chars();
            let first = chars.clone().next().unwrap_or(' ');
            chars
                .find(|&c| matches!(bidi_class(c), BidiClass::L | BidiClass::R | BidiClass::AL))
                .unwrap_or(first)
        }
    };
    let text: String = pieces.iter().map(representative).collect();
    let strong = |rtl: bool| {
        text.chars()
            .filter(|&c| match bidi_class(c) {
                BidiClass::R | BidiClass::AL => rtl,
                BidiClass::L => !rtl,
                _ => false,
            })
            .count()
    };
    let paragraph = if strong(true) > strong(false) {
        Level::rtl()
    } else {
        Level::ltr()
    };
    let info = ParagraphBidiInfo::new(&text, Some(paragraph));
    let levels: Vec<u8> = text
        .char_indices()
        .map(|(byte, _)| info.levels[byte].number())
        .collect();
    // From the highest level down to the lowest odd one, every run of pieces
    // at that level or higher is turned around.
    let highest = levels.iter().copied().max().unwrap_or(0);
    let lowest_odd = levels
        .iter()
        .copied()
        .filter(|l| l % 2 == 1)
        .min()
        .unwrap_or(highest + 1);
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    for level in (lowest_odd..=highest).rev() {
        let mut start = 0;
        while start < order.len() {
            if levels[order[start]] < level {
                start += 1;
                continue;
            }
            let mut end = start;
            while end < order.len() && levels[order[end]] >= level {
                end += 1;
            }
            order[start..end].reverse();
            start = end;
        }
    }
    let original: Vec<Option<&TextChar>> = pieces.to_vec();
    for (slot, index) in pieces.iter_mut().zip(order) {
        *slot = original[index];
    }
}
