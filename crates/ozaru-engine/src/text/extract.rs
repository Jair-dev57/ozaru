use super::glyph::{TextChar, characters};
use super::line::{TextLine, lines};
use super::word::line_text;
use crate::base::DiagnosticKind;
use crate::content::{Interpreter, PageContent};
use crate::document::{Document, Rectangle};

/// The text of a page whose visible area is `visible`, usually its crop box:
/// its lines, in the order they were painted, joined by line feeds, with an
/// empty line between paragraphs.
///
/// Characters wholly outside the visible area are left out, as viewers don't
/// show them. A paragraph ends where the next line, of the same size, is
/// further below than 1.3 times the usual distance between the lines of the
/// page, the lower quartile of the distances, and than 1.5 times the size of
/// the text, or where it is above the line before it, as when a new column
/// starts. A change of size, as after a heading, is a line feed only. Lines of whitespace
/// are left out, and so is the whitespace at the end of each line.
pub fn page_text(content: &PageContent, visible: &Rectangle) -> String {
    let mut characters = characters(content);
    characters.retain(|character| is_visible(character, visible));
    let lines: Vec<(TextLine, String)> = lines(characters)
        .into_iter()
        .map(|line| {
            let text = line_text(&line).trim_end().to_string();
            (line, text)
        })
        .filter(|(_, text)| !text.is_empty())
        .collect();
    let gaps: Vec<Option<f64>> = lines
        .windows(2)
        .map(|pair| line_gap(&pair[0].0, &pair[1].0))
        .collect();
    let usual = lower_quartile(gaps.iter().flatten().copied().filter(|gap| *gap > 0.0));

    let mut text = String::new();
    for (index, (line, line_text)) in lines.iter().enumerate() {
        if index > 0 {
            let previous = &lines[index - 1].0;
            let breaks = match gaps[index - 1] {
                // Text that turns, such as a label up the margin.
                None => "\n",
                Some(gap) if gap <= 0.0 => "\n\n",
                Some(gap)
                    if same_size(previous.size(), line.size())
                        && usual.is_some_and(|usual| gap > 1.3 * usual)
                        && gap > 1.5 * line.size() =>
                {
                    "\n\n"
                }
                Some(_) => "\n",
            };
            text.push_str(breaks);
        }
        text.push_str(line_text);
    }
    text
}

// How far below `previous` the line `next` starts, across their direction.
// `None` if they don't run in the same direction.
fn line_gap(previous: &TextLine, next: &TextLine) -> Option<f64> {
    let (a, b) = (previous.direction(), next.direction());
    if a.0 * b.0 + a.1 * b.1 < 0.99 {
        return None;
    }
    Some(-previous.height_of(next.start()))
}

fn same_size(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.05 * a.max(b)
}

fn lower_quartile(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut values: Vec<f64> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 4])
}

// Whether any of the box of `character` is inside `visible`: the box from its
// start to its end, an em tall above its baseline.
fn is_visible(character: &TextChar, visible: &Rectangle) -> bool {
    let (dx, dy) = character.direction;
    let up = (-dy * character.size, dx * character.size);
    let corners = [
        character.start,
        character.end,
        (character.start.0 + up.0, character.start.1 + up.1),
        (character.end.0 + up.0, character.end.1 + up.1),
    ];
    let left = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let top = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);
    left <= visible.right && right >= visible.left && bottom <= visible.top && top >= visible.bottom
}

/// Why the text of a page may be missing or incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageProblem {
    /// The page object can't be read, so its content is unknown.
    Damaged,
    /// A content stream of the page can't be decoded, so its text is missing.
    UnreadableContent,
}

/// The text of one page of a document.
#[derive(Debug, Clone, PartialEq)]
pub struct PageText {
    pub text: String,
    /// What kept some or all of its text from being read, if anything.
    pub problem: Option<PageProblem>,
}

/// The text of every page of `document`, as [`page_text`] gives it. Fonts are
/// read once for the whole document.
///
/// A locked document has pages without text: unlock it first.
pub fn document_text(document: &mut Document<'_>) -> Vec<PageText> {
    let pages = document.pages().to_vec();
    let mut interpreter = Interpreter::new(document.limits());
    pages
        .iter()
        .map(|page| {
            if page.is_damaged() {
                return PageText {
                    text: String::new(),
                    problem: Some(PageProblem::Damaged),
                };
            }
            let content = interpreter.run_page(document.resolver(), page);
            let unreadable = content
                .diagnostics
                .iter()
                .any(|d| d.kind() == DiagnosticKind::InvalidContents);
            PageText {
                text: page_text(&content, &page.crop_box()),
                problem: unreadable.then_some(PageProblem::UnreadableContent),
            }
        })
        .collect()
}
