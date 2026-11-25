//! A page as Markdown: headings by the size of their text, bold and italic by
//! the font, lists, and tables, for reading by people and language models.

use std::collections::BTreeMap;

use super::analysis::{TextSpan, page_spans};
use super::extract::TextBlock;
use super::tables::Table;
use crate::document::Rectangle;

/// What the pages of a document look like, so each page is written the same
/// way: the size of its body text, and the sizes of its headings.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownStyle {
    /// The size most of the text has, in points.
    pub body: f64,
    /// The sizes of headings, largest first: the first is a level 1 heading.
    pub headings: Vec<f64>,
}

// Sizes are compared in halves of a point.
fn bucket(size: f64) -> i64 {
    (size * 2.0).round() as i64
}

fn block_size(block: &TextBlock) -> f64 {
    block
        .lines
        .iter()
        .map(|line| line.size())
        .fold(0.0, f64::max)
}

fn block_chars(block: &TextBlock) -> usize {
    block
        .lines
        .iter()
        .map(|line| {
            line.chars
                .iter()
                .map(|c| c.text.chars().count())
                .sum::<usize>()
        })
        .sum()
}

// Whether a block could be a heading: short, and not a list.
fn is_short(block: &TextBlock) -> bool {
    let first: String = block
        .lines
        .first()
        .map(|line| line.chars.iter().map(|c| c.text.as_str()).collect())
        .unwrap_or_default();
    block.lines.len() <= 3
        && block_chars(block) <= 200
        && list_item(&format!("{} ", first.trim_start())).is_none()
        && list_item(first.trim_start()).is_none()
}

impl MarkdownStyle {
    /// The style of a document from the blocks of all its pages. Headings are
    /// short blocks in a size over 12 % larger than the body text, up to six
    /// sizes.
    pub fn of(pages: &[&[TextBlock]]) -> Self {
        let mut weight: BTreeMap<i64, usize> = BTreeMap::new();
        for block in pages.iter().flat_map(|blocks| blocks.iter()) {
            for line in &block.lines {
                for c in &line.chars {
                    *weight.entry(bucket(c.size)).or_default() += c.text.chars().count();
                }
            }
        }
        let body = weight
            .iter()
            .max_by_key(|(_, count)| **count)
            .map_or(12.0, |(bucket, _)| *bucket as f64 / 2.0);
        let mut sizes: Vec<i64> = pages
            .iter()
            .flat_map(|blocks| blocks.iter())
            .filter(|block| is_short(block))
            .map(|block| bucket(block_size(block)))
            .filter(|&size| size as f64 / 2.0 > body * 1.12)
            .collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes.dedup();
        sizes.truncate(6);
        Self {
            body,
            headings: sizes.into_iter().map(|size| size as f64 / 2.0).collect(),
        }
    }

    // The level of a heading of `size`, from 1, or `None` for body text.
    fn level(&self, size: f64) -> Option<usize> {
        let size = bucket(size) as f64 / 2.0;
        self.headings
            .iter()
            .position(|&heading| heading <= size + 0.01)
            .map(|i| i + 1)
    }
}

/// The page as Markdown: `blocks` are its text blocks, in reading order, and
/// `tables` its tables, which replace the text inside them.
pub fn page_markdown(blocks: &[TextBlock], tables: &[Table], style: &MarkdownStyle) -> String {
    let spans = page_spans(blocks);
    let inside_table = |block: &TextBlock| {
        let center = (
            (block.bbox.left + block.bbox.right) / 2.0,
            (block.bbox.bottom + block.bbox.top) / 2.0,
        );
        tables.iter().any(|table| contains(&table.bbox, center))
    };
    let mut parts: Vec<String> = Vec::new();
    let mut pending: Vec<&Table> = tables.iter().collect();
    pending.sort_by(|a, b| b.bbox.top.total_cmp(&a.bbox.top));
    let mut pending = pending.into_iter().peekable();
    for (index, block) in blocks.iter().enumerate() {
        // The tables that start above this block come first.
        while let Some(table) = pending.next_if(|table| table.bbox.top >= block.bbox.top) {
            parts.push(table_markdown(table));
        }
        if inside_table(block) {
            continue;
        }
        let block_spans: Vec<&TextSpan> = spans.iter().filter(|span| span.block == index).collect();
        parts.extend(block_markdown(block, &block_spans, style));
    }
    parts.extend(pending.map(table_markdown));
    parts.retain(|part| !part.is_empty());
    parts.join("\n\n")
}

fn contains(area: &Rectangle, (x, y): (f64, f64)) -> bool {
    x >= area.left && x <= area.right && y >= area.bottom && y <= area.top
}

// ---------------------------------------------------------------------------

const BULLETS: [char; 11] = ['•', '◦', '▪', '●', '‣', '·', '–', '—', '-', '*', '○'];

// The text a line has after a list marker, and the marker as Markdown.
fn list_item(text: &str) -> Option<(String, &str)> {
    let mut chars = text.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    if BULLETS.contains(&first) && rest.starts_with(char::is_whitespace) {
        return Some(("-".to_string(), rest.trim_start()));
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        let after = &text[digits..];
        let mut tail = after.chars();
        if matches!(tail.next(), Some('.' | ')')) && tail.as_str().starts_with(char::is_whitespace)
        {
            return Some((format!("{}.", &text[..digits]), tail.as_str().trim_start()));
        }
    }
    None
}

// The text of a line of spans with its emphasis as Markdown, and without.
fn line_text(spans: &[&TextSpan], plain: bool) -> String {
    let mut text = String::new();
    for span in spans {
        let body = escape(&span.text);
        let emphasis = match (span.bold, span.italic) {
            _ if plain => "",
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        if emphasis.is_empty() || body.trim().is_empty() {
            text.push_str(&body);
            continue;
        }
        // The marks hug the text; the spaces stay outside.
        let trimmed = body.trim();
        let start = body.len() - body.trim_start().len();
        text.push_str(&body[..start]);
        text.push_str(emphasis);
        text.push_str(trimmed);
        text.push_str(emphasis);
        text.push_str(&body[start + trimmed.len()..]);
    }
    text
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '`') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

// Joins the text of the next line to the line so far. A hyphen that ends a
// line before a lowercase letter is a word cut in two and goes; before a
// capital or a number it stays, with no space after it.
fn join(previous: &mut String, next: &str) {
    let hyphenated = previous.ends_with('-')
        && previous
            .chars()
            .rev()
            .nth(1)
            .is_some_and(char::is_alphabetic)
        && next.chars().next().is_some_and(char::is_alphanumeric);
    if hyphenated {
        if next.chars().next().is_some_and(char::is_lowercase) {
            previous.pop();
        }
    } else if !previous.is_empty() && !next.is_empty() {
        previous.push(' ');
    }
    previous.push_str(next);
}

fn block_markdown(block: &TextBlock, spans: &[&TextSpan], style: &MarkdownStyle) -> Vec<String> {
    // The spans of each line; a gap between two, which the spans don't have
    // as a space, becomes one.
    let owned: Vec<Vec<TextSpan>> = (0..block.lines.len())
        .map(|line| {
            let mut spaced: Vec<TextSpan> = Vec::new();
            for span in spans.iter().filter(|span| span.line == line) {
                let mut span = (*span).clone();
                if let Some(before) = spaced.last()
                    && !before.text.ends_with(' ')
                    && !span.text.starts_with(' ')
                    && span.bbox.left - before.bbox.right > 0.1 * span.size
                {
                    span.text.insert(0, ' ');
                }
                spaced.push(span);
            }
            spaced
        })
        .collect();
    let lines: Vec<Vec<&TextSpan>> = owned.iter().map(|line| line.iter().collect()).collect();
    let mut parts = Vec::new();
    if let Some(level) = style.level(block_size(block)).filter(|_| is_short(block)) {
        let mut heading = String::new();
        for line in &lines {
            join(&mut heading, line_text(line, true).trim());
        }
        if !heading.is_empty() {
            parts.push(format!("{} {heading}", "#".repeat(level)));
        }
        return parts;
    }
    // Paragraphs and list items, as the lines go.
    let mut paragraph = String::new();
    let mut items: Vec<String> = Vec::new();
    for line in &lines {
        let raw: String = line.iter().map(|span| span.text.as_str()).collect();
        if let Some((marker, rest)) = list_item(raw.trim_start()) {
            if !paragraph.is_empty() {
                parts.push(std::mem::take(&mut paragraph));
            }
            // The line without its marker, in the emphasis it had.
            let mut owned: Vec<TextSpan> = line.iter().map(|span| (*span).clone()).collect();
            let mut remove = raw.len() - rest.len();
            for span in &mut owned {
                let take = remove.min(span.text.len());
                span.text.drain(..take);
                remove -= take;
            }
            let refs: Vec<&TextSpan> = owned.iter().collect();
            items.push(format!("{marker} {}", line_text(&refs, false).trim()));
            continue;
        }
        let text = line_text(line, false);
        let text = text.trim();
        match items.last_mut() {
            Some(last) => join(last, text),
            None => join(&mut paragraph, text),
        }
    }
    if !paragraph.is_empty() {
        parts.push(starts_safely(paragraph));
    }
    if !items.is_empty() {
        parts.push(items.join("\n"));
    }
    parts
}

// A paragraph that starts like Markdown syntax is written so it isn't.
fn starts_safely(paragraph: String) -> String {
    let starts_like_syntax =
        paragraph.starts_with(['#', '>', '+', '-']) || list_item(&paragraph).is_some();
    if starts_like_syntax {
        format!("\\{paragraph}")
    } else {
        paragraph
    }
}

/// A table as a Markdown table, its first row as the header; cells with line
/// feeds use `<br>`.
pub fn table_markdown(table: &Table) -> String {
    let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return String::new();
    }
    let cell = |text: &str| text.replace('|', "\\|").replace('\n', "<br>");
    let row = |cells: &[Option<super::tables::TableCell>]| {
        let texts: Vec<String> = (0..columns)
            .map(|i| {
                cells
                    .get(i)
                    .and_then(Option::as_ref)
                    .map_or(String::new(), |c| cell(&c.text))
            })
            .collect();
        format!("| {} |", texts.join(" | "))
    };
    let mut lines = Vec::new();
    for (index, cells) in table.rows.iter().enumerate() {
        lines.push(row(cells));
        if index == 0 {
            lines.push(format!("|{}|", vec![" --- "; columns].join("|")));
        }
    }
    lines.join("\n")
}
