use super::glyph::TextChar;

/// Characters on one baseline, in the order they were painted.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub chars: Vec<TextChar>,
}

impl TextLine {
    /// The direction the line runs in, that of its first character.
    pub fn direction(&self) -> (f64, f64) {
        self.chars[0].direction
    }

    /// Where the line starts: where its first character does.
    pub fn start(&self) -> (f64, f64) {
        self.chars[0].start
    }

    /// The size of its largest character.
    pub fn size(&self) -> f64 {
        self.chars.iter().map(|c| c.size).fold(0.0, f64::max)
    }

    /// How far `point` is above the line, across its direction: positive
    /// above, negative below, in default user space units.
    pub fn height_of(&self, point: (f64, f64)) -> f64 {
        let (dx, dy) = self.direction();
        let (sx, sy) = self.start();
        dx * (point.1 - sy) - dy * (point.0 - sx)
    }
}

/// Puts characters together into lines, in the order they were painted.
///
/// A character goes on the line of the one before it when it runs in the same
/// direction and its baseline is less than half an em, of the line's largest
/// characters, away from the baseline of the line's first character, so
/// superscripts and subscripts stay on their line. Otherwise it starts a new line. A
/// character on the same baseline goes on the line wherever it is along it,
/// so the cells of a table row are one line.
pub fn lines(chars: Vec<TextChar>) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = Vec::new();
    for character in chars {
        if let Some(line) = lines.last_mut()
            && on_line(line, &character)
        {
            line.chars.push(character);
            continue;
        }
        lines.push(TextLine {
            chars: vec![character],
        });
    }
    lines
}

fn on_line(line: &TextLine, next: &TextChar) -> bool {
    let (dx, dy) = line.direction();
    let parallel = dx * next.direction.0 + dy * next.direction.1 > 0.99;
    let across = line.height_of(next.start);
    let size = line.size().max(next.size);
    // Two characters of size 0 are on the same line only on the same baseline.
    parallel && across.abs() <= (0.5 * size).max(1e-6)
}
