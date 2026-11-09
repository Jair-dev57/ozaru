//! Columns of text painted row by row: the first line of each column, then
//! the second line of each, and so on. Their lines are on the same baselines,
//! so [`lines`](super::lines) puts each row on one line; here they are taken
//! apart and put in reading order, one column after the other.

use super::glyph::TextChar;
use super::line::TextLine;
use super::word::line_words;

// How wide a gap, in ems, separates the pieces of a line that may be in
// different columns. Spaces between words are a quarter of an em or so.
const PIECE_GAP: f64 = 1.0;

// How narrow, in ems, the space between two columns can become.
const NARROWEST_GUTTER: f64 = 0.5;

// How many lines must run down the columns, and how many of them must have
// text on both sides, for the space between them to be a gutter.
const LEAST_LINES: usize = 4;
const LEAST_ROWS: usize = 3;

// How many words the pieces of each column must have, at the median, to be
// prose: the cells of a table are shorter.
const PROSE_WORDS: usize = 4;

/// The lines of `lines`, with columns painted row by row put in reading
/// order: where several lines in a row run across the same empty space, a
/// gutter, and the text on each side of it reads as prose, each line is cut at
/// the gutters, and the pieces come column by column, from left to right and
/// each from the top down.
///
/// It only acts when it is sure: tables, whose cells are short, and lines
/// that cross the gutter, such as a heading over both columns, are left as
/// they are. So is text that doesn't run left to right, and text of scripts
/// written right to left.
pub fn read_columns(lines: Vec<TextLine>) -> Vec<TextLine> {
    let mut result = Vec::with_capacity(lines.len());
    let mut rest = lines.into_iter().peekable();
    while let Some(line) = rest.next() {
        let Some(mut gutters) = gaps(&line) else {
            result.push(line);
            continue;
        };
        // The lines that keep the gutters free.
        let mut run = vec![line];
        while let Some(next) = rest.peek() {
            match narrowed(&gutters, next) {
                Some(narrower) => {
                    gutters = narrower;
                    run.push(rest.next().expect("peeked"));
                }
                None => break,
            }
        }
        if is_a_set_of_columns(&run, &gutters) {
            result.extend(by_column(run, &gutters));
        } else {
            result.extend(run);
        }
    }
    result
}

// The space between two columns, from the right of one to the left of the
// next, along x.
type Gutter = (f64, f64);

// The pieces of a line: its characters split where the gap between two of
// them is an em or more. Whitespace goes with the piece before it.
fn pieces(line: &TextLine) -> Vec<Vec<&TextChar>> {
    let mut pieces: Vec<Vec<&TextChar>> = Vec::new();
    let mut last_end: Option<f64> = None;
    for character in &line.chars {
        if character.is_whitespace() {
            if let Some(piece) = pieces.last_mut() {
                piece.push(character);
            }
            continue;
        }
        let far =
            last_end.is_some_and(|end| character.bbox.left - end >= PIECE_GAP * character.size);
        if far || pieces.is_empty() {
            pieces.push(Vec::new());
        }
        pieces.last_mut().expect("pushed").push(character);
        last_end = Some(character.bbox.right);
    }
    pieces
}

// Where a piece starts and ends along x, without its whitespace.
fn extent(piece: &[&TextChar]) -> (f64, f64) {
    let ink = piece.iter().filter(|c| !c.is_whitespace());
    let left = ink
        .clone()
        .map(|c| c.bbox.left)
        .fold(f64::INFINITY, f64::min);
    let right = ink.map(|c| c.bbox.right).fold(f64::NEG_INFINITY, f64::max);
    (left, right)
}

// Whether a line may be in columns: it runs left to right, in a script
// written left to right.
fn is_left_to_right(line: &TextLine) -> bool {
    let (dx, dy) = line.direction();
    dx > 0.99
        && dy.abs() < 0.01
        && !line
            .chars
            .iter()
            .any(|c| c.text.chars().any(is_right_to_left))
}

// Hebrew, Arabic, Syriac, Thaana, NKo and the other scripts written right to
// left, and their presentation forms.
fn is_right_to_left(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

// The gaps between the pieces of `line`, if it has more than one piece and
// may be in columns.
fn gaps(line: &TextLine) -> Option<Vec<Gutter>> {
    if !is_left_to_right(line) {
        return None;
    }
    let extents: Vec<(f64, f64)> = pieces(line).iter().map(|p| extent(p)).collect();
    let gaps: Vec<Gutter> = extents
        .windows(2)
        .map(|pair| (pair[0].1, pair[1].0))
        .collect();
    (!gaps.is_empty()).then_some(gaps)
}

// The gutters, narrowed to stay clear of the pieces of `line`. `None` if a
// piece crosses one, as a heading over the columns does, if one becomes too
// narrow, or if the line doesn't run left to right.
fn narrowed(gutters: &[Gutter], line: &TextLine) -> Option<Vec<Gutter>> {
    if !is_left_to_right(line) {
        return None;
    }
    let size = line.size();
    let extents: Vec<(f64, f64)> = pieces(line).iter().map(|p| extent(p)).collect();
    gutters
        .iter()
        .map(|&(low, high)| {
            let (mut low, mut high) = (low, high);
            for &(left, right) in &extents {
                if right <= high && left < low {
                    // On the left of the gutter, maybe reaching into it.
                    low = low.max(right);
                } else if left >= low && right > high {
                    high = high.min(left);
                } else if right <= low || left >= high {
                    continue;
                } else {
                    return None;
                }
            }
            (high - low >= NARROWEST_GUTTER * size).then_some((low, high))
        })
        .collect()
}

// Which column, from 0, a piece that starts at `left` is in.
fn column_of(gutters: &[Gutter], left: f64) -> usize {
    gutters.iter().filter(|(low, _)| left >= *low).count()
}

// Whether the lines of `run`, which keep `gutters` free, are columns of prose:
// every line has text in every column, but the last ones, where the columns
// end at different heights, and the text reads as prose. In a table, a cell of
// several lines leaves the others empty beside it.
fn is_a_set_of_columns(run: &[TextLine], gutters: &[Gutter]) -> bool {
    if run.len() < LEAST_LINES {
        return false;
    }
    let columns = gutters.len() + 1;
    let mut words: Vec<Vec<usize>> = vec![Vec::new(); columns];
    let mut rows = 0;
    let mut ended = false;
    for line in run {
        let mut filled = vec![false; columns];
        for piece in pieces(line) {
            let column = column_of(gutters, extent(&piece).0);
            filled[column] = true;
            let piece = TextLine {
                chars: piece.into_iter().cloned().collect(),
            };
            words[column].push(line_words(&piece).len());
        }
        if filled.iter().all(|filled| *filled) {
            if ended {
                return false;
            }
            rows += 1;
        } else {
            ended = true;
        }
    }
    rows >= LEAST_ROWS && words.iter_mut().all(|counts| median(counts) >= PROSE_WORDS)
}

fn median(counts: &mut [usize]) -> usize {
    counts.sort_unstable();
    counts.get(counts.len() / 2).copied().unwrap_or(0)
}

// The pieces of the lines of `run`, column by column.
fn by_column(run: Vec<TextLine>, gutters: &[Gutter]) -> Vec<TextLine> {
    let mut columns: Vec<Vec<TextLine>> = vec![Vec::new(); gutters.len() + 1];
    for line in &run {
        for piece in pieces(line) {
            let column = column_of(gutters, extent(&piece).0);
            columns[column].push(TextLine {
                chars: piece.into_iter().cloned().collect(),
            });
        }
    }
    columns.into_iter().flatten().collect()
}
