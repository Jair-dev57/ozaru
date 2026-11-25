//! Tables: grids of cells found from the lines a page draws, and, where it
//! draws none, from the way its text lines up in columns.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::extract::TextBlock;
use super::line::TextLine;
use super::word::{TextWord, line_words};
use crate::content::PaintedPath;
use crate::document::Rectangle;
use crate::render::Segment;

/// How tables are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableStrategy {
    /// Only grids made of lines the page draws.
    Lines,
    /// Only text that lines up in columns and rows.
    Text,
    /// Lines first, and text that lines up where there are no lines.
    Both,
}

/// A cell of a table, with the text inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    /// Its words, line by line, a space between the words of a line and a line
    /// feed between lines.
    pub text: String,
    pub bbox: Rectangle,
}

/// A table of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub bbox: Rectangle,
    /// Row by row, from the top. Every row has a place for each column; it is
    /// `None` where a cell of another row spans it, or the row has no cell.
    pub rows: Vec<Vec<Option<TableCell>>>,
    /// Whether it is made of lines the page draws, and not found from its text.
    pub ruled: bool,
}

// Lines closer than this, in points, are one line; so are the ends of lines
// that come this close.
const SNAP: f64 = 3.0;
// A line shorter than this isn't one of a table.
const MIN_EDGE: f64 = 3.0;
// A filled rectangle this thin or thinner is a line.
const THIN: f64 = 2.5;
// The most edges, cells and rows kept from a page.
const MAX_EDGES: usize = 20_000;
const MAX_CELLS: usize = 20_000;

/// The tables of a page: `paths` are what it paints, `blocks` its text.
pub fn page_tables(
    paths: &[PaintedPath],
    blocks: &[TextBlock],
    strategy: TableStrategy,
) -> Vec<Table> {
    let words: Vec<TextWord> = blocks
        .iter()
        .flat_map(|block| block.lines.iter().flat_map(line_words))
        .collect();
    let mut tables = Vec::new();
    if strategy != TableStrategy::Text {
        tables.extend(ruled_tables(paths, &words));
    }
    if strategy != TableStrategy::Lines {
        let taken: Vec<Rectangle> = tables.iter().map(|table| table.bbox).collect();
        let lines: Vec<&TextLine> = blocks.iter().flat_map(|block| block.lines.iter()).collect();
        for table in text_tables(&lines) {
            let inside = taken.iter().any(|bbox| overlaps(bbox, &table.bbox));
            if !inside {
                tables.push(table);
            }
        }
    }
    tables.sort_by(|a, b| b.bbox.top.total_cmp(&a.bbox.top));
    tables
}

fn overlaps(a: &Rectangle, b: &Rectangle) -> bool {
    a.left < b.right && b.left < a.right && a.bottom < b.top && b.bottom < a.top
}

// ---------------------------------------------------------------------------
// Tables drawn with lines.

// A straight line along an axis: at `pos` across, from `lo` to `hi` along.
#[derive(Debug, Clone, Copy)]
struct Edge {
    pos: f64,
    lo: f64,
    hi: f64,
}

// The horizontal and vertical lines a page paints: the straight parts of the
// strokes, and filled rectangles so thin that they are lines. Positions are
// in the default user space, with y turned upside down so that rows go down.
fn edges_of(paths: &[PaintedPath]) -> (Vec<Edge>, Vec<Edge>) {
    let mut horizontal = Vec::new();
    let mut vertical = Vec::new();
    let mut add = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return;
        }
        if (y1 - y0).abs() < 0.5 {
            horizontal.push(Edge {
                pos: (y0 + y1) / 2.0,
                lo: x0.min(x1),
                hi: x0.max(x1),
            });
        } else if (x1 - x0).abs() < 0.5 {
            vertical.push(Edge {
                pos: (x0 + x1) / 2.0,
                lo: y0.min(y1),
                hi: y0.max(y1),
            });
        }
    };
    for painted in paths.iter().take(MAX_EDGES) {
        let matrix = &painted.matrix;
        let mut start = (0.0, 0.0);
        let mut last = (0.0, 0.0);
        // The corners of the subpath, to see if it is a rectangle.
        let mut corners: Vec<(f64, f64)> = Vec::new();
        let flush = |corners: &mut Vec<(f64, f64)>, add: &mut dyn FnMut(f64, f64, f64, f64)| {
            if painted.fill.is_some()
                && !painted.stroke
                && let Some((x0, y0, x1, y1)) = thin_rectangle(corners)
            {
                add(x0, y0, x1, y1);
            }
            corners.clear();
        };
        for segment in &painted.path.segments {
            match *segment {
                Segment::Move(x, y) => {
                    flush(&mut corners, &mut add);
                    last = matrix.apply(x, y);
                    start = last;
                    corners.push(last);
                }
                Segment::Line(x, y) => {
                    let point = matrix.apply(x, y);
                    if painted.stroke {
                        add(last.0, -last.1, point.0, -point.1);
                    }
                    last = point;
                    corners.push(point);
                }
                Segment::Curve(_, _, _, _, x, y) => {
                    last = matrix.apply(x, y);
                    corners.push((f64::NAN, f64::NAN));
                }
                Segment::Close => {
                    if painted.stroke {
                        add(last.0, -last.1, start.0, -start.1);
                    }
                    last = start;
                }
            }
        }
        flush(&mut corners, &mut add);
    }
    (horizontal, vertical)
}

// The line a thin filled rectangle stands for: from the corners of a subpath
// that is an axis-aligned rectangle no thicker than `THIN`. Coordinates are
// returned with y turned upside down.
fn thin_rectangle(corners: &[(f64, f64)]) -> Option<(f64, f64, f64, f64)> {
    let mut points = corners.to_vec();
    if points.len() == 5 && points[0] == points[4] {
        points.pop();
    }
    if points.len() != 4 || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return None;
    }
    let left = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let right = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let bottom = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let top = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    // Each corner is a corner of the box: an axis-aligned rectangle.
    let on_box = points.iter().all(|p| {
        ((p.0 - left).abs() < 0.01 || (p.0 - right).abs() < 0.01)
            && ((p.1 - bottom).abs() < 0.01 || (p.1 - top).abs() < 0.01)
    });
    if !on_box {
        return None;
    }
    let (width, height) = (right - left, top - bottom);
    if height <= THIN && width > height {
        let y = -(bottom + top) / 2.0;
        Some((left, y, right, y))
    } else if width <= THIN && height > width {
        let x = (left + right) / 2.0;
        Some((x, -bottom, x, -top))
    } else {
        None
    }
}

// Lines whose positions are within `SNAP` of each other are put at the
// average of their group; lines on the same position that overlap or nearly
// touch are joined; short ones are dropped.
fn tidy(mut edges: Vec<Edge>) -> Vec<Edge> {
    edges.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let mut groups: Vec<Vec<Edge>> = Vec::new();
    for edge in edges {
        match groups.last_mut() {
            Some(group) if edge.pos - group[0].pos <= SNAP => group.push(edge),
            _ => groups.push(vec![edge]),
        }
    }
    let mut result = Vec::new();
    for mut group in groups {
        let pos = group.iter().map(|e| e.pos).sum::<f64>() / group.len() as f64;
        group.sort_by(|a, b| a.lo.total_cmp(&b.lo));
        let mut current: Option<Edge> = None;
        for edge in group {
            current = match current {
                Some(mut joined) if edge.lo <= joined.hi + SNAP => {
                    joined.hi = joined.hi.max(edge.hi);
                    Some(joined)
                }
                other => {
                    result.extend(other.filter(|e| e.hi - e.lo >= MIN_EDGE));
                    Some(Edge { pos, ..edge })
                }
            };
        }
        result.extend(current.filter(|e| e.hi - e.lo >= MIN_EDGE));
    }
    result
}

// A position as a whole number, to compare points exactly.
fn key(value: f64) -> i64 {
    (value * 100.0).round() as i64
}

type Point = (i64, i64);

fn ruled_tables(paths: &[PaintedPath], words: &[TextWord]) -> Vec<Table> {
    let (horizontal, vertical) = edges_of(paths);
    let horizontal = tidy(horizontal);
    let vertical = tidy(vertical);
    if horizontal.len() < 2 || vertical.len() < 2 {
        return Vec::new();
    }
    // Where lines cross, with the lines that cross there.
    let mut points: BTreeSet<Point> = BTreeSet::new();
    for h in &horizontal {
        for v in &vertical {
            if v.pos >= h.lo - SNAP
                && v.pos <= h.hi + SNAP
                && h.pos >= v.lo - SNAP
                && h.pos <= v.hi + SNAP
            {
                points.insert((key(v.pos), key(h.pos)));
            }
        }
    }
    if points.len() < 4 {
        return Vec::new();
    }
    // Whether a line joins two points, along a row or down a column.
    let joined_across = |a: Point, b: Point| {
        horizontal.iter().any(|e| {
            key(e.pos) == a.1
                && key(e.lo - SNAP) <= a.0.min(b.0)
                && key(e.hi + SNAP) >= a.0.max(b.0)
        })
    };
    let joined_down = |a: Point, b: Point| {
        vertical.iter().any(|e| {
            key(e.pos) == a.0
                && key(e.lo - SNAP) <= a.1.min(b.1)
                && key(e.hi + SNAP) >= a.1.max(b.1)
        })
    };
    let mut by_row: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut by_column: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for &(x, y) in &points {
        by_row.entry(y).or_default().push(x);
        by_column.entry(x).or_default().push(y);
    }
    // Cells as (left, top, right, bottom) in keys, y growing downwards.
    let mut cells: Vec<(i64, i64, i64, i64)> = Vec::new();
    'points: for &(x, y) in &points {
        let rights = by_row[&y].iter().filter(|&&r| r > x);
        let belows: Vec<i64> = by_column[&x].iter().copied().filter(|&b| b > y).collect();
        for &right in rights {
            for &below in &belows {
                let corner = (right, below);
                if points.contains(&corner)
                    && joined_across((x, y), (right, y))
                    && joined_down((x, y), (x, below))
                    && joined_down((right, y), corner)
                    && joined_across((x, below), corner)
                {
                    cells.push((x, y, right, below));
                    if cells.len() >= MAX_CELLS {
                        break 'points;
                    }
                    continue 'points;
                }
            }
        }
    }
    // Cells that share a corner are of the same table.
    let mut owner: Vec<usize> = (0..cells.len()).collect();
    fn find(owner: &mut [usize], mut i: usize) -> usize {
        while owner[i] != i {
            owner[i] = owner[owner[i]];
            i = owner[i];
        }
        i
    }
    let mut at_corner: HashMap<Point, usize> = HashMap::new();
    for (index, &(l, t, r, b)) in cells.iter().enumerate() {
        for corner in [(l, t), (r, t), (l, b), (r, b)] {
            match at_corner.get(&corner) {
                Some(&other) => {
                    let (a, c) = (find(&mut owner, index), find(&mut owner, other));
                    owner[a] = c;
                }
                None => {
                    at_corner.insert(corner, index);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<(i64, i64, i64, i64)>> = BTreeMap::new();
    for (index, &cell) in cells.iter().enumerate() {
        let root = find(&mut owner, index);
        groups.entry(root).or_default().push(cell);
    }
    let mut used: HashSet<usize> = HashSet::new();
    let mut tables = Vec::new();
    for group in groups.into_values() {
        if group.len() < 2 {
            continue;
        }
        tables.push(table_of_cells(&group, words, &mut used));
    }
    tables
}

fn unkey(value: i64) -> f64 {
    value as f64 / 100.0
}

// The table of cells in keys with y downwards, its words taken from `words`.
fn table_of_cells(
    cells: &[(i64, i64, i64, i64)],
    words: &[TextWord],
    used: &mut HashSet<usize>,
) -> Table {
    let mut tops: Vec<i64> = cells.iter().map(|c| c.1).collect();
    tops.sort_unstable();
    tops.dedup();
    let mut lefts: Vec<i64> = cells.iter().map(|c| c.0).collect();
    lefts.sort_unstable();
    lefts.dedup();
    let rectangle = |&(l, t, r, b): &(i64, i64, i64, i64)| {
        Rectangle::new(unkey(l), -unkey(b), unkey(r), -unkey(t))
    };
    let mut bbox = rectangle(&cells[0]);
    for cell in cells {
        bbox = bbox.union(&rectangle(cell));
    }
    let mut rows = Vec::new();
    for &top in &tops {
        let mut row = Vec::new();
        for &left in &lefts {
            let cell = cells.iter().find(|c| c.0 == left && c.1 == top);
            row.push(cell.map(|cell| {
                let area = rectangle(cell);
                TableCell {
                    text: text_in(&area, words, used),
                    bbox: area,
                }
            }));
        }
        rows.push(row);
    }
    Table {
        bbox,
        rows,
        ruled: true,
    }
}

// The words whose centers are inside `area`, line by line.
fn text_in(area: &Rectangle, words: &[TextWord], used: &mut HashSet<usize>) -> String {
    let mut inside: Vec<(usize, &TextWord)> = words
        .iter()
        .enumerate()
        .filter(|(index, word)| {
            let x = (word.bbox.left + word.bbox.right) / 2.0;
            let y = (word.bbox.bottom + word.bbox.top) / 2.0;
            !used.contains(index)
                && x >= area.left
                && x <= area.right
                && y >= area.bottom
                && y <= area.top
        })
        .collect();
    used.extend(inside.iter().map(|(index, _)| *index));
    // Top to bottom, then left to right; words within half a height of each
    // other are on the same line.
    inside.sort_by(|a, b| b.1.bbox.top.total_cmp(&a.1.bbox.top));
    let mut lines: Vec<Vec<&TextWord>> = Vec::new();
    for (_, word) in inside {
        let height = word.bbox.top - word.bbox.bottom;
        match lines.last_mut() {
            Some(line) if (line[0].bbox.top - word.bbox.top).abs() < 0.5 * height.max(1.0) => {
                line.push(word);
            }
            _ => lines.push(vec![word]),
        }
    }
    lines
        .into_iter()
        .map(|mut line| {
            line.sort_by(|a, b| a.bbox.left.total_cmp(&b.bbox.left));
            line.iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Tables found from their text.

// A group of words of a row that are close together.
struct RowCell {
    text: String,
    bbox: Rectangle,
    words: usize,
}

// The cells of a line that runs from left to right: its words, put together
// when less than an em apart. `None` for any other line.
fn row_cells(line: &TextLine) -> Option<Vec<RowCell>> {
    let direction = line.direction();
    if direction.0 < 0.999 {
        return None;
    }
    let size = line.size().max(1.0);
    let mut words = line_words(line);
    words.sort_by(|a, b| a.bbox.left.total_cmp(&b.bbox.left));
    let mut cells: Vec<RowCell> = Vec::new();
    for word in words {
        match cells.last_mut() {
            Some(cell) if word.bbox.left - cell.bbox.right <= size => {
                cell.text.push(' ');
                cell.text.push_str(&word.text);
                cell.bbox = cell.bbox.union(&word.bbox);
                cell.words += 1;
            }
            _ => cells.push(RowCell {
                text: word.text,
                bbox: word.bbox,
                words: 1,
            }),
        }
    }
    Some(cells)
}

// A run of consecutive lines with several cells each that make a table when
// they line up: at least three rows, two columns, and cells that are short,
// unlike the lines of columns of prose.
fn text_tables(lines: &[&TextLine]) -> Vec<Table> {
    let rows: Vec<Option<Vec<RowCell>>> = lines
        .iter()
        .map(|line| row_cells(line).filter(|cells| cells.len() >= 2))
        .collect();
    let mut tables = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        if rows[start].is_none() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < rows.len() && rows[end].is_some() && close_rows(lines[end - 1], lines[end]) {
            end += 1;
        }
        let run: Vec<&Vec<RowCell>> = rows[start..end].iter().flatten().collect();
        if let Some(table) = table_of_rows(&run) {
            tables.push(table);
        }
        start = end;
    }
    tables
}

// Whether a line is just under another, a row of the same table.
fn close_rows(above: &TextLine, below: &TextLine) -> bool {
    let size = above.size().max(below.size()).max(1.0);
    let gap = above.bbox().bottom - below.bbox().top;
    gap < 1.5 * size && above.bbox().bottom > below.bbox().bottom
}

fn table_of_rows(rows: &[&Vec<RowCell>]) -> Option<Table> {
    if rows.len() < 3 || rows.len() > MAX_CELLS {
        return None;
    }
    let cells: usize = rows.iter().map(|row| row.len()).sum();
    let words: usize = rows
        .iter()
        .flat_map(|row| row.iter())
        .map(|c| c.words)
        .sum();
    // Prose in columns has many words in each cell.
    if words as f64 / cells as f64 > 4.0 {
        return None;
    }
    // A column of bullets or numbers is a list, not a table.
    let columns_count = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    for column in 0..columns_count {
        let marks = rows
            .iter()
            .all(|row| row.get(column).is_none_or(|cell| is_marker(&cell.text)));
        let any = rows.iter().any(|row| row.get(column).is_some());
        if marks && any {
            return None;
        }
    }
    // The columns: the places along the line that some cell covers, joined
    // when cells overlap.
    let mut spans: Vec<(f64, f64)> = rows
        .iter()
        .flat_map(|row| row.iter().map(|c| (c.bbox.left, c.bbox.right)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut columns: Vec<(f64, f64)> = Vec::new();
    for (left, right) in spans {
        match columns.last_mut() {
            Some(column) if left <= column.1 => column.1 = column.1.max(right),
            _ => columns.push((left, right)),
        }
    }
    if columns.len() < 2 {
        return None;
    }
    // Most rows must have cells in several columns.
    let wide = rows.iter().filter(|row| row.len() >= 2).count();
    if (wide as f64) < 0.6 * rows.len() as f64 {
        return None;
    }
    let mut table_rows = Vec::new();
    let mut bbox: Option<Rectangle> = None;
    for row in rows {
        let mut filled: Vec<Option<TableCell>> = vec![None; columns.len()];
        for cell in row.iter() {
            let center = (cell.bbox.left + cell.bbox.right) / 2.0;
            let column = columns
                .iter()
                .position(|(left, right)| center >= *left && center <= *right)?;
            filled[column] = Some(match filled[column].take() {
                // Two cells of a row in one column, because other rows join
                // their columns: they are one cell.
                Some(other) => TableCell {
                    text: format!("{} {}", other.text, cell.text),
                    bbox: other.bbox.union(&cell.bbox),
                },
                None => TableCell {
                    text: cell.text.clone(),
                    bbox: cell.bbox,
                },
            });
            bbox = Some(bbox.map_or(cell.bbox, |b| b.union(&cell.bbox)));
        }
        table_rows.push(filled);
    }
    Some(Table {
        bbox: bbox?,
        rows: table_rows,
        ruled: false,
    })
}

// Whether text is a bullet or a number that starts a list item.
fn is_marker(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(c), None, _) => !c.is_alphanumeric(),
        (Some(a), Some('.' | ')'), None) => a.is_ascii_digit(),
        _ => {
            let digits = text.chars().take_while(char::is_ascii_digit).count();
            digits > 0 && matches!(&text[digits..], "." | ")")
        }
    }
}
