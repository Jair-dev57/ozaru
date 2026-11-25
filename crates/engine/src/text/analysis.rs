//! Text for analysis: pieces of a line that look the same (spans), and a
//! search that gives the boxes of what it finds.

use super::extract::TextBlock;
use super::glyph::TextChar;
use super::word::reading_order;
use crate::document::Rectangle;

/// A run of text on a line that has one font, one size and one color, like a
/// word in bold in the middle of a sentence.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    /// The block of the page it is in, from 0.
    pub block: usize,
    /// The line of its block, from 0.
    pub line: usize,
    pub text: String,
    /// The name of the font as the PDF gives it, `ABCDEF+Arial-BoldMT`.
    pub font: Option<String>,
    /// The size of the text on the page, in points.
    pub size: f64,
    /// The color of the text, as RGB.
    pub color: [u8; 3],
    /// Whether the name of the font says it is bold.
    pub bold: bool,
    /// Whether the name of the font says it is italic or oblique.
    pub italic: bool,
    /// The box around it, in the default user space of the page.
    pub bbox: Rectangle,
}

/// Whether a font name says the font is bold: the name of the family and style
/// as writers give them (`Arial-BoldMT`, `Helvetica-Black`, `CMBX10`).
pub fn is_bold_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let name = name.rsplit('+').next().unwrap_or(&name);
    ["bold", "black", "heavy", "cmbx", "-bd", ",bd"]
        .iter()
        .any(|word| name.contains(word))
}

/// Whether a font name says the font is italic or oblique.
pub fn is_italic_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let name = name.rsplit('+').next().unwrap_or(&name);
    ["italic", "oblique", "cmti", "cmsl"]
        .iter()
        .any(|word| name.contains(word))
        || name.ends_with("-it")
        || name.ends_with(",it")
}

fn same_style(a: &TextChar, b: &TextChar) -> bool {
    a.color == b.color
        && (a.size - b.size).abs() < 0.05
        && (std::sync::Arc::ptr_eq(&a.font, &b.font)
            || a.font_name().is_some() && a.font_name() == b.font_name())
}

/// The spans of the blocks of a page, in reading order. A space the text
/// needs between two characters belongs to the span before it.
pub fn page_spans(blocks: &[TextBlock]) -> Vec<TextSpan> {
    let mut spans: Vec<TextSpan> = Vec::new();
    for (block_index, block) in blocks.iter().enumerate() {
        for (line_index, line) in block.lines.iter().enumerate() {
            // The character the span in progress started with.
            let mut first: Option<&TextChar> = None;
            let mut previous: Option<&TextChar> = None;
            let mut current: Option<TextSpan> = None;
            for piece in reading_order(line) {
                match piece {
                    None => {
                        if let Some(span) = current.as_mut() {
                            span.text.push(' ');
                        }
                    }
                    Some(character) => {
                        // A gap of more than an em, such as between the cells of
                        // a table row, starts a new span.
                        let gap = previous.is_some_and(|before| {
                            let (dx, dy) = line.direction();
                            let along = (character.start.0 - before.end.0) * dx
                                + (character.start.1 - before.end.1) * dy;
                            along > character.size.max(before.size)
                        });
                        previous = Some(character);
                        let continues =
                            !gap && first.is_some_and(|first| same_style(first, character));
                        if gap && let Some(span) = current.as_mut() {
                            let trimmed = span.text.trim_end().len();
                            span.text.truncate(trimmed);
                        }
                        match current.as_mut() {
                            Some(span) if continues => {
                                span.text.push_str(&character.text);
                                span.bbox = span.bbox.union(&character.bbox);
                            }
                            _ => {
                                spans.extend(current.take());
                                first = Some(character);
                                let font = character.font_name();
                                let bold = font.as_deref().is_some_and(is_bold_name);
                                let italic = font.as_deref().is_some_and(is_italic_name);
                                current = Some(TextSpan {
                                    block: block_index,
                                    line: line_index,
                                    text: character.text.clone(),
                                    font,
                                    size: character.size,
                                    color: character.color,
                                    bold,
                                    italic,
                                    bbox: character.bbox,
                                });
                            }
                        }
                    }
                }
            }
            // A space at the end of a line isn't part of what was written.
            if let Some(mut last) = current {
                let trimmed = last.text.trim_end().len();
                last.text.truncate(trimmed);
                spans.push(last);
            }
        }
    }
    spans.retain(|span| !span.text.is_empty());
    spans
}

/// Where a search found its text: the text as the page has it, and one box
/// for each line it is on.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMatch {
    pub text: String,
    pub boxes: Vec<Rectangle>,
}

// One character of a block's text, and the character of the page it belongs
// to: a space between lines belongs to none.
struct Unit {
    original: char,
    owner: Option<(usize, Rectangle)>,
}

/// The places `query` is found in the text of `blocks`, in reading order.
///
/// A match can go over several lines of a block, and runs of whitespace, in
/// the page and in `query`, count as one space. Letters of any case match
/// unless `case_sensitive`. An empty query finds nothing.
pub fn page_search(blocks: &[TextBlock], query: &str, case_sensitive: bool) -> Vec<TextMatch> {
    let needle = fold(
        query.chars().map(|c| (c, ())).collect::<Vec<_>>(),
        case_sensitive,
    );
    if needle.is_empty() {
        return Vec::new();
    }
    let needle: Vec<char> = needle.into_iter().map(|(c, _)| c).collect();
    let mut matches = Vec::new();
    for block in blocks {
        let units = units_of(block);
        let haystack = fold(
            units
                .iter()
                .enumerate()
                .map(|(i, u)| (u.original, i))
                .collect(),
            case_sensitive,
        );
        let chars: Vec<char> = haystack.iter().map(|(c, _)| *c).collect();
        let mut at = 0;
        while at + needle.len() <= chars.len() {
            if chars[at..at + needle.len()] != needle[..] {
                at += 1;
                continue;
            }
            let first = haystack[at].1;
            let last = haystack[at + needle.len() - 1].1;
            matches.push(match_of(&units[first..=last]));
            at += needle.len();
        }
    }
    matches
}

// The characters of `items` with whitespace runs made one space, and letters
// lowercased unless `case_sensitive`, each carrying what it came with. A
// character that lowercases to several gives each of them.
fn fold<T: Copy>(items: Vec<(char, T)>, case_sensitive: bool) -> Vec<(char, T)> {
    let mut folded: Vec<(char, T)> = Vec::with_capacity(items.len());
    for (c, tag) in items {
        if c.is_whitespace() {
            if folded.last().is_some_and(|(last, _)| *last == ' ') || folded.is_empty() {
                continue;
            }
            folded.push((' ', tag));
        } else if case_sensitive {
            folded.push((c, tag));
        } else {
            folded.extend(c.to_lowercase().map(|lower| (lower, tag)));
        }
    }
    while folded.last().is_some_and(|(c, _)| *c == ' ') {
        folded.pop();
    }
    folded
}

// The characters of a block, a space between its lines.
fn units_of(block: &TextBlock) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    for (line_index, line) in block.lines.iter().enumerate() {
        if line_index > 0 {
            units.push(Unit {
                original: ' ',
                owner: None,
            });
        }
        for piece in reading_order(line) {
            match piece {
                None => units.push(Unit {
                    original: ' ',
                    owner: None,
                }),
                Some(character) => {
                    for c in character.text.chars() {
                        units.push(Unit {
                            original: c,
                            owner: Some((line_index, character.bbox)),
                        });
                    }
                }
            }
        }
    }
    units
}

fn match_of(units: &[Unit]) -> TextMatch {
    let text: String = units.iter().map(|unit| unit.original).collect();
    let mut boxes: Vec<(usize, Rectangle)> = Vec::new();
    for (line, bbox) in units.iter().filter_map(|unit| unit.owner) {
        match boxes.last_mut() {
            Some((last, rectangle)) if *last == line => *rectangle = rectangle.union(&bbox),
            _ => boxes.push((line, bbox)),
        }
    }
    TextMatch {
        text,
        boxes: boxes.into_iter().map(|(_, bbox)| bbox).collect(),
    }
}
