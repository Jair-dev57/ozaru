use super::glyph::{TextChar, characters};
use super::line::{TextLine, lines};
use super::word::{TextWord, line_text, line_words};
use crate::base::{DiagnosticKind, Diagnostics};
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
/// are left out, and so is the whitespace at the start and end of each line.
pub fn page_text(content: &PageContent, visible: &Rectangle) -> String {
    let lines: Vec<(TextLine, String)> = visible_lines(content, visible)
        .into_iter()
        .map(|line| {
            let text = line_text(&line).trim().to_string();
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

/// The words of a page whose visible area is `visible`, line by line, in the
/// order the lines were painted, as [`page_text`] puts its lines. Lines
/// without words are left out.
pub fn page_words(content: &PageContent, visible: &Rectangle) -> Vec<Vec<TextWord>> {
    visible_lines(content, visible)
        .iter()
        .map(line_words)
        .filter(|words| !words.is_empty())
        .collect()
}

// The lines of the characters of `content` that are at least partly inside
// `visible`.
fn visible_lines(content: &PageContent, visible: &Rectangle) -> Vec<TextLine> {
    let mut characters = characters(content);
    characters.retain(|character| is_visible(character, visible));
    lines(characters)
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
    each_page(document, page_text)
        .into_iter()
        .map(|(text, problem)| PageText { text, problem })
        .collect()
}

/// The words of one page of a document, line by line.
#[derive(Debug, Clone, PartialEq)]
pub struct PageWords {
    pub lines: Vec<Vec<TextWord>>,
    /// What kept some or all of its text from being read, if anything.
    pub problem: Option<PageProblem>,
}

/// The words of every page of `document`, as [`page_words`] gives them.
/// Fonts are read once for the whole document.
///
/// A locked document has pages without words: unlock it first.
pub fn document_words(document: &mut Document<'_>) -> Vec<PageWords> {
    each_page(document, page_words)
        .into_iter()
        .map(|(lines, problem)| PageWords { lines, problem })
        .collect()
}

// `read` of the content and the visible area, the crop box, of every page, and
// what kept it from reading the page in full. A damaged page gives what
// `read` gives for no content.
fn each_page<T>(
    document: &mut Document<'_>,
    read: impl Fn(&PageContent, &Rectangle) -> T,
) -> Vec<(T, Option<PageProblem>)> {
    let pages = document.pages().to_vec();
    let mut interpreter = Interpreter::new(document.limits());
    pages
        .iter()
        .map(|page| {
            if page.is_damaged() {
                let nothing = PageContent {
                    glyphs: Vec::new(),
                    marked_content: Vec::new(),
                    diagnostics: Diagnostics::new(0),
                };
                return (read(&nothing, &page.crop_box()), Some(PageProblem::Damaged));
            }
            let content = interpreter.run_page(document.resolver(), page);
            let unreadable = content
                .diagnostics
                .iter()
                .any(|d| d.kind() == DiagnosticKind::InvalidContents);
            (
                read(&content, &page.crop_box()),
                unreadable.then_some(PageProblem::UnreadableContent),
            )
        })
        .collect()
}
