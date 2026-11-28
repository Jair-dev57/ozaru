//! Comparing two documents: which pages were added, removed or changed, which
//! words changed on each (and where), and what differs in their information,
//! bookmarks, attachments and form fields. Optionally what looks different on
//! the pages that stayed.

mod diff;
mod fingerprint;
mod visual;

pub use diff::{Hunk, hunks};
pub use visual::{VisualDifference, diff_image, visual_difference};

use std::collections::{BTreeMap, HashMap};

use crate::document::{Document, Rectangle};
use crate::render::{RenderOptions, render_page};
use crate::text::{PageWords, document_words};
use fingerprint::page_fingerprint;

/// How two documents are compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompareOptions {
    /// Whether the pages that are paired are also drawn and compared as pictures,
    /// which finds changes of images and graphics, not only of text.
    pub visual: bool,
    /// Pixels for each point when drawing pages to compare them.
    pub scale: f64,
    /// How much a color must change, from 0 to 255 in any channel, for a pixel to
    /// count as different.
    pub threshold: u8,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            visual: false,
            scale: 1.0,
            threshold: 32,
        }
    }
}

/// What became of a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStatus {
    /// It is in both documents, as it was.
    Same,
    /// It is in both documents, and something in it changed.
    Changed,
    /// It is only in the second document.
    Added,
    /// It is only in the first document.
    Removed,
}

/// A place where the words of a page changed: words of the first document that
/// aren't in the second, and words of the second that weren't in the first.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChange {
    pub removed: String,
    pub added: String,
    /// Where the removed words were on the page of the first document, one box
    /// for each run of words on a line, in the default user space.
    pub removed_boxes: Vec<Rectangle>,
    /// Where the added words are, on the page of the second document.
    pub added_boxes: Vec<Rectangle>,
}

/// A page of the first document and its pair in the second.
#[derive(Debug, Clone, PartialEq)]
pub struct PageComparison {
    /// The page of the first document, from 0.
    pub a: Option<usize>,
    /// The page of the second document, from 0.
    pub b: Option<usize>,
    pub status: PageStatus,
    pub changes: Vec<TextChange>,
    /// Whether the pages are not the same size.
    pub size_changed: bool,
    /// How the pages look different, when asked for.
    pub visual: Option<VisualDifference>,
}

/// An entry of the information of the documents that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoChange {
    pub key: String,
    pub a: Option<String>,
    pub b: Option<String>,
}

/// A form field that changed: its value in each document, or `None` if the
/// field isn't in one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub name: String,
    pub a: Option<String>,
    pub b: Option<String>,
}

/// What differs between two documents.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The pages, in order, each with its pair.
    pub pages: Vec<PageComparison>,
    pub info: Vec<InfoChange>,
    pub bookmarks_removed: Vec<String>,
    pub bookmarks_added: Vec<String>,
    pub attachments_removed: Vec<String>,
    pub attachments_added: Vec<String>,
    pub fields: Vec<FieldChange>,
}

impl Comparison {
    /// Whether the documents have nothing different that was looked at.
    pub fn is_identical(&self) -> bool {
        self.pages.iter().all(|p| p.status == PageStatus::Same)
            && self.info.is_empty()
            && self.bookmarks_removed.is_empty()
            && self.bookmarks_added.is_empty()
            && self.attachments_removed.is_empty()
            && self.attachments_added.is_empty()
            && self.fields.is_empty()
    }
}

// FNV-1a, for the hash of a word.
fn hash(text: &str) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in text.as_bytes() {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

// The words of a page in reading order, with their boxes and lines.
struct PageText {
    words: Vec<(String, Rectangle)>,
    // The line each word is on, to join the boxes of runs.
    lines: Vec<usize>,
    hashes: Vec<u64>,
    // The distinct hashes, sorted, for telling how alike two pages are.
    unique: Vec<u64>,
}

fn page_text(words: &PageWords) -> PageText {
    let mut result = PageText {
        words: Vec::new(),
        lines: Vec::new(),
        hashes: Vec::new(),
        unique: Vec::new(),
    };
    for (line, words) in words.lines.iter().enumerate() {
        for word in words {
            result.words.push((word.text.clone(), word.bbox));
            result.lines.push(line);
            result.hashes.push(hash(&word.text));
        }
    }
    result.unique = result.hashes.clone();
    result.unique.sort_unstable();
    result.unique.dedup();
    result
}

// How alike two pages are by their words, from 0 to 1. Two pages without
// words are alike, but not quite as much as two with the same words.
fn similarity(a: &PageText, b: &PageText) -> f64 {
    if a.unique.is_empty() && b.unique.is_empty() {
        return 0.9;
    }
    let (mut i, mut j, mut common) = (0, 0, 0usize);
    while i < a.unique.len() && j < b.unique.len() {
        match a.unique[i].cmp(&b.unique[j]) {
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    let total = a.unique.len() + b.unique.len() - common;
    if total == 0 {
        1.0
    } else {
        common as f64 / total as f64
    }
}

// The least a pair of pages must be alike to be the same page, changed.
const MIN_SIMILARITY: f64 = 0.3;

// The most pages of each document aligned by their text: past this, pages are
// paired by their number.
const MAX_ALIGNED: usize = 2_000;

// Which pages of the first document go with which of the second: the pairs
// that make the pages most alike, without crossing, and what is left over.
fn align(a: &[PageText], b: &[PageText]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (a.len(), b.len());
    if n > MAX_ALIGNED || m > MAX_ALIGNED || n * m > 4_000_000 {
        return (0..n.max(m))
            .map(|i| ((i < n).then_some(i), (i < m).then_some(i)))
            .collect();
    }
    // Only pages near the diagonal are paired: the documents have most pages
    // in the same order.
    let band = n.abs_diff(m) + 24;
    let near = |i: usize, j: usize| {
        let expected = if n == 0 {
            0.0
        } else {
            i as f64 * m as f64 / n as f64
        };
        (j as f64 - expected).abs() <= band as f64
    };
    let width = m + 1;
    let mut score = vec![0.0_f32; (n + 1) * width];
    let mut gain = vec![0.0_f32; (n + 1) * width];
    for i in 1..=n {
        for j in 1..=m {
            let mut best = score[(i - 1) * width + j].max(score[i * width + j - 1]);
            if near(i - 1, j - 1) {
                let s = similarity(&a[i - 1], &b[j - 1]);
                if s >= MIN_SIMILARITY {
                    let with = score[(i - 1) * width + j - 1] + s as f32;
                    gain[i * width + j] = s as f32;
                    if with >= best {
                        best = with;
                    }
                }
            }
            score[i * width + j] = best;
        }
    }
    // Back from the end: a pair if it is what made the score, else the page
    // that is left over.
    let mut pairs = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && gain[i * width + j] > 0.0
            && (score[i * width + j] - (score[(i - 1) * width + j - 1] + gain[i * width + j])).abs()
                < 1e-5
        {
            pairs.push((Some(i - 1), Some(j - 1)));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || score[i * width + j] == score[i * width + j - 1]) {
            // Going back, so the page removed comes before the one added.
            pairs.push((None, Some(j - 1)));
            j -= 1;
        } else {
            pairs.push((Some(i - 1), None));
            i -= 1;
        }
    }
    pairs.reverse();
    pair_rewritten(pairs)
}

// Pages that are in the first document only, and pages that are in the second
// only, between the same two pages that are paired, in equal number, are a page
// rewritten: they are paired in order, changed. Without a page paired near them
// there is nothing to say they go together.
fn pair_rewritten(
    pairs: Vec<(Option<usize>, Option<usize>)>,
) -> Vec<(Option<usize>, Option<usize>)> {
    let has_pair = pairs.iter().any(|(a, b)| a.is_some() && b.is_some());
    if !has_pair {
        return pairs;
    }
    let mut result = Vec::with_capacity(pairs.len());
    let mut gap: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let flush = |gap: &mut Vec<(Option<usize>, Option<usize>)>,
                 result: &mut Vec<(Option<usize>, Option<usize>)>| {
        let removed: Vec<usize> = gap.iter().filter_map(|(a, _)| *a).collect();
        let added: Vec<usize> = gap.iter().filter_map(|(_, b)| *b).collect();
        if !removed.is_empty() && removed.len() == added.len() {
            result.extend(
                removed
                    .into_iter()
                    .zip(added)
                    .map(|(a, b)| (Some(a), Some(b))),
            );
        } else {
            result.append(gap);
        }
        gap.clear();
    };
    for pair in pairs {
        if pair.0.is_some() && pair.1.is_some() {
            flush(&mut gap, &mut result);
            result.push(pair);
        } else {
            gap.push(pair);
        }
    }
    flush(&mut gap, &mut result);
    result
}

// The boxes of the words `range` of a page, one for each run on a line.
fn run_boxes(page: &PageText, range: std::ops::Range<usize>) -> Vec<Rectangle> {
    let mut boxes: Vec<Rectangle> = Vec::new();
    let mut last_line = None;
    for index in range {
        let (_, bbox) = &page.words[index];
        match boxes.last_mut() {
            Some(last) if last_line == Some(page.lines[index]) => *last = last.union(bbox),
            _ => boxes.push(*bbox),
        }
        last_line = Some(page.lines[index]);
    }
    boxes
}

fn join(page: &PageText, range: std::ops::Range<usize>) -> String {
    page.words[range]
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn text_changes(a: &PageText, b: &PageText) -> Vec<TextChange> {
    hunks(&a.hashes, &b.hashes)
        .into_iter()
        .map(|hunk| TextChange {
            removed: join(a, hunk.a_start..hunk.a_end),
            added: join(b, hunk.b_start..hunk.b_end),
            removed_boxes: run_boxes(a, hunk.a_start..hunk.a_end),
            added_boxes: run_boxes(b, hunk.b_start..hunk.b_end),
        })
        .collect()
}

// The values of the entries of the information of a document.
fn info_entries(document: &Document<'_>) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    if let Some(info) = document.info() {
        for (key, _) in info.dictionary().iter() {
            let name = String::from_utf8_lossy(key.as_bytes()).into_owned();
            if let Some(value) = info.text(&name) {
                entries.insert(name, value);
            }
        }
    }
    entries
}

fn field_text(value: &Option<crate::document::FieldValue>) -> Option<String> {
    use crate::document::FieldValue;
    match value {
        None => Some(String::new()),
        Some(FieldValue::Text(text)) => Some(text.clone()),
        Some(FieldValue::Choices(choices)) => Some(choices.join(", ")),
    }
}

/// Compares `a` and `b`, two documents already unlocked.
///
/// Pages are paired by what they say, so a page added or removed doesn't make the
/// pages after it look changed; documents of more than 2,000 pages are paired by
/// number. Words are compared one by one, so text that wraps differently isn't a
/// change. Pages that can't be read count as having no words.
pub fn compare_documents(
    a: &mut Document<'_>,
    b: &mut Document<'_>,
    options: &CompareOptions,
) -> Comparison {
    let texts_a: Vec<PageText> = document_words(a).iter().map(page_text).collect();
    let texts_b: Vec<PageText> = document_words(b).iter().map(page_text).collect();
    let pairs = align(&texts_a, &texts_b);

    let mut pages = Vec::with_capacity(pairs.len());
    for (index_a, index_b) in pairs {
        let (status, changes, size_changed, visual) = match (index_a, index_b) {
            (Some(i), Some(j)) => {
                let changes = if texts_a[i].hashes == texts_b[j].hashes {
                    Vec::new()
                } else {
                    text_changes(&texts_a[i], &texts_b[j])
                };
                let (page_a, page_b) = (a.page(i).cloned(), b.page(j).cloned());
                let size_changed = match (&page_a, &page_b) {
                    (Some(x), Some(y)) => {
                        let (ca, cb) = (x.crop_box(), y.crop_box());
                        (ca.width() - cb.width()).abs() > 0.5
                            || (ca.height() - cb.height()).abs() > 0.5
                            || x.rotation() != y.rotation()
                    }
                    _ => false,
                };
                let mut visual = None;
                if options.visual
                    && !size_changed
                    && let (Some(x), Some(y)) = (page_a, page_b)
                {
                    // Pages that draw the same, as their contents say, are not
                    // drawn: that is the slowest part, and most pages stay.
                    let same_drawing = match (
                        page_fingerprint(a.resolver(), &x),
                        page_fingerprint(b.resolver(), &y),
                    ) {
                        (Some(fa), Some(fb)) => fa == fb,
                        _ => false,
                    };
                    if same_drawing {
                        visual = Some(VisualDifference {
                            fraction: 0.0,
                            regions: Vec::new(),
                        });
                    } else {
                        let render = RenderOptions {
                            scale: options.scale,
                            ..RenderOptions::default()
                        };
                        let (limits_a, limits_b) = (a.limits(), b.limits());
                        let drawn_a = render_page(a.resolver(), &x, &render, limits_a);
                        let drawn_b = render_page(b.resolver(), &y, &render, limits_b);
                        if let (Ok(drawn_a), Ok(drawn_b)) = (drawn_a, drawn_b) {
                            visual = visual_difference(
                                &drawn_a,
                                &drawn_b,
                                &x,
                                options.scale,
                                options.threshold,
                            );
                        }
                    }
                }
                let looks_different = visual.as_ref().is_some_and(|v| v.fraction > 0.0);
                let status = if changes.is_empty() && !size_changed && !looks_different {
                    PageStatus::Same
                } else {
                    PageStatus::Changed
                };
                (status, changes, size_changed, visual)
            }
            (Some(_), None) => (PageStatus::Removed, Vec::new(), false, None),
            _ => (PageStatus::Added, Vec::new(), false, None),
        };
        pages.push(PageComparison {
            a: index_a,
            b: index_b,
            status,
            changes,
            size_changed,
            visual,
        });
    }

    // The information.
    let (info_a, info_b) = (info_entries(a), info_entries(b));
    let mut keys: Vec<&String> = info_a.keys().chain(info_b.keys()).collect();
    keys.sort();
    keys.dedup();
    let info = keys
        .into_iter()
        .filter(|key| info_a.get(*key) != info_b.get(*key))
        .map(|key| InfoChange {
            key: key.clone(),
            a: info_a.get(key).cloned(),
            b: info_b.get(key).cloned(),
        })
        .collect();

    // Bookmarks and attachments, by their titles and names.
    let titles = |document: &mut Document<'_>| -> Vec<String> {
        document.outlines().into_iter().map(|b| b.title).collect()
    };
    let (marks_a, marks_b) = (titles(a), titles(b));
    let (mut bookmarks_removed, mut bookmarks_added) = (Vec::new(), Vec::new());
    for hunk in hunks(&marks_a, &marks_b) {
        bookmarks_removed.extend(marks_a[hunk.a_start..hunk.a_end].iter().cloned());
        bookmarks_added.extend(marks_b[hunk.b_start..hunk.b_end].iter().cloned());
    }
    let names = |document: &mut Document<'_>| -> Vec<String> {
        document.attachments().into_iter().map(|f| f.name).collect()
    };
    let (files_a, files_b) = (names(a), names(b));
    let attachments_removed = files_a
        .iter()
        .filter(|n| !files_b.contains(n))
        .cloned()
        .collect();
    let attachments_added = files_b
        .iter()
        .filter(|n| !files_a.contains(n))
        .cloned()
        .collect();

    // Form fields, by name.
    let values = |document: &mut Document<'_>| -> HashMap<String, Option<String>> {
        document
            .form_fields()
            .into_iter()
            .map(|field| (field.name.clone(), field_text(&field.value)))
            .collect()
    };
    let (fields_a, fields_b) = (values(a), values(b));
    let mut names: Vec<&String> = fields_a.keys().chain(fields_b.keys()).collect();
    names.sort();
    names.dedup();
    let fields = names
        .into_iter()
        .filter(|name| fields_a.get(*name) != fields_b.get(*name))
        .map(|name| FieldChange {
            name: name.clone(),
            a: fields_a.get(name).cloned().flatten(),
            b: fields_b.get(name).cloned().flatten(),
        })
        .collect();

    Comparison {
        pages,
        info,
        bookmarks_removed,
        bookmarks_added,
        attachments_removed,
        attachments_added,
        fields,
    }
}
