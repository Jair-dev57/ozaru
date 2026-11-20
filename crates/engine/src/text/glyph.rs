use crate::content::{Glyph, PageContent};
use crate::document::Rectangle;

/// A piece of text placed on the page: the text of one glyph, or the
/// `/ActualText` of a marked-content sequence that stands for several.
///
/// Positions are in the default user space of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChar {
    /// One or more characters: a ligature, or a whole `/ActualText`.
    pub text: String,
    /// Where it starts: the origin of the first glyph, or its vertical
    /// origin in vertical writing.
    pub start: (f64, f64),
    /// Where it ends: the end of the last glyph.
    pub end: (f64, f64),
    /// The direction the text runs in, a unit vector: `(1, 0)` for ordinary
    /// text, `(0, -1)` for vertical text.
    pub direction: (f64, f64),
    /// The size of the text on the page, the font size times the scale of the
    /// text and the page: how tall an em is.
    pub size: f64,
    /// The box around its glyphs, from the descent of their font to its
    /// ascent: see [`Glyph::corners`].
    pub bbox: Rectangle,
}

impl TextChar {
    /// Whether it is only whitespace, like a space glyph.
    pub fn is_whitespace(&self) -> bool {
        self.text.chars().all(char::is_whitespace)
    }
}

/// The text of the glyphs of a page, in the order they were painted
/// (ISO 32000-2, section 9.10 and 14.9.4):
///
/// - each glyph gives the text its font maps it to; glyphs without text are
///   left out;
/// - a marked-content sequence with `/ActualText` gives that text once, in
///   place of the glyphs inside it, the outermost one if they are nested; an
///   empty one hides them;
/// - a glyph painted again over itself with the same text, as some writers
///   do to make text look bold, is kept once.
pub fn characters(content: &PageContent) -> Vec<TextChar> {
    let mut characters: Vec<TextChar> = Vec::new();
    // The sequence whose `/ActualText` the last character came from.
    let mut replaced: Option<usize> = None;
    for glyph in &content.glyphs {
        let replacement = actual_text_sequence(content, glyph);
        if let Some(sequence) = replacement {
            let placed = placed(glyph, String::new());
            if replaced == Some(sequence) {
                // Another glyph of the same sequence: the text runs on to it.
                if let Some(last) = characters.last_mut() {
                    last.end = placed.end;
                    last.bbox = last.bbox.union(&placed.bbox);
                }
                continue;
            }
            replaced = Some(sequence);
            let text = content.marked_content[sequence]
                .actual_text
                .clone()
                .unwrap_or_default();
            characters.push(TextChar { text, ..placed });
            continue;
        }
        replaced = None;
        let Some(text) = glyph.text.as_deref().filter(|text| !text.is_empty()) else {
            continue;
        };
        let placed = placed(glyph, text.to_string());
        if let Some(last) = characters.last()
            && last.text == placed.text
            && distance(last.start, placed.start) < 0.05 * placed.size.max(last.size)
        {
            continue;
        }
        characters.push(placed);
    }
    // An empty `/ActualText` hides its glyphs.
    characters.retain(|character| !character.text.is_empty());
    characters
}

// The outermost sequence around `glyph` that has `/ActualText`.
fn actual_text_sequence(content: &PageContent, glyph: &Glyph) -> Option<usize> {
    let mut found = None;
    let mut next = glyph.marked_content;
    while let Some(index) = next {
        let sequence = content.marked_content.get(index)?;
        if sequence.actual_text.is_some() {
            found = Some(index);
        }
        next = sequence.parent;
    }
    found
}

// `text` placed where `glyph` is: where it starts and ends, the direction it
// runs in and its size.
fn placed(glyph: &Glyph, text: String) -> TextChar {
    let matrix = &glyph.matrix;
    let start = match &glyph.vertical {
        Some(v) => matrix.apply(v.origin_x, v.origin_y),
        None => glyph.origin(),
    };
    let end = glyph.end();
    let size = (matrix.c * matrix.c + matrix.d * matrix.d).sqrt();
    let along = match &glyph.vertical {
        Some(_) => (-matrix.c, -matrix.d),
        None => (matrix.a, matrix.b),
    };
    let direction = unit((end.0 - start.0, end.1 - start.1))
        .or_else(|| unit(along))
        .unwrap_or((1.0, 0.0));
    TextChar {
        text,
        start,
        end,
        direction,
        size,
        bbox: glyph.bounding_box(),
    }
}

fn unit((x, y): (f64, f64)) -> Option<(f64, f64)> {
    let length = (x * x + y * y).sqrt();
    (length > 1e-9 && length.is_finite()).then(|| (x / length, y / length))
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
