//! Comparing documents: the differences between two sequences, pages paired by
//! what they say, words that changed, and what looks different.

use ozaru_engine::Document;
use ozaru_engine::compare::{CompareOptions, PageStatus, compare_documents, diff_image, hunks};
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::writer::{Editor, NewField, NewFieldKind};

mod common;
use common::{file_with, stream_object};

fn spans(text: &str) -> Vec<char> {
    text.chars().collect()
}

#[test]
fn hunks_find_the_shortest_differences() {
    let h = |a: &str, b: &str| {
        hunks(&spans(a), &spans(b))
            .into_iter()
            .map(|h| (h.a_start, h.a_end, h.b_start, h.b_end))
            .collect::<Vec<_>>()
    };
    assert!(h("abc", "abc").is_empty());
    assert!(h("", "").is_empty());
    assert_eq!(h("abc", "abXc"), [(2, 2, 2, 3)]);
    assert_eq!(h("abXc", "abc"), [(2, 3, 2, 2)]);
    assert_eq!(h("abc", "aXc"), [(1, 2, 1, 2)]);
    assert_eq!(h("", "abc"), [(0, 0, 0, 3)]);
    assert_eq!(h("abc", ""), [(0, 3, 0, 0)]);
    assert_eq!(h("abcdef", "aXcdYf"), [(1, 2, 1, 2), (4, 5, 4, 5)]);
    assert_eq!(h("abcdefgh", "abgh"), [(2, 6, 2, 2)]);
    // A move is a removal and an addition.
    assert_eq!(h("abcd", "acbd"), [(1, 2, 1, 1), (3, 3, 2, 3)]);
}

#[test]
fn hunks_apply_back_to_the_second_sequence() {
    // A pseudo-random pair of long sequences: applying the hunks to the first
    // gives the second.
    let mut seed = 12345_u64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as u32
    };
    for _ in 0..20 {
        let a: Vec<u32> = (0..400).map(|_| next() % 7).collect();
        let mut b = a.clone();
        for _ in 0..30 {
            let at = (next() as usize) % b.len().max(1);
            match next() % 3 {
                0 => {
                    b.remove(at);
                }
                1 => b.insert(at, next() % 7),
                _ => b[at] = next() % 7,
            }
        }
        let mut rebuilt = Vec::new();
        let mut at = 0;
        for h in hunks(&a, &b) {
            rebuilt.extend_from_slice(&a[at..h.a_start]);
            rebuilt.extend_from_slice(&b[h.b_start..h.b_end]);
            at = h.a_end;
        }
        rebuilt.extend_from_slice(&a[at..]);
        assert_eq!(rebuilt, b);
    }
}

#[test]
fn very_different_or_very_long_sequences_still_give_a_result() {
    let a: Vec<u32> = (0..6000).collect();
    let b: Vec<u32> = (6000..12000).collect();
    let h = hunks(&a, &b);
    assert_eq!(h.len(), 1);
    assert_eq!((h[0].a_end, h[0].b_end), (6000, 6000));
}

// A document whose pages say what `pages` says, in Helvetica, with `extra`
// operators drawn after the text.
fn doc(pages: &[(&str, &str)]) -> Vec<u8> {
    let mut objects = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            format!(
                "<< /Type /Pages /Kids [{}] /Count {} >>",
                (0..pages.len())
                    .map(|i| format!("{} 0 R", 10 + i))
                    .collect::<Vec<_>>()
                    .join(" "),
                pages.len()
            ),
        ),
        (
            4,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
    ];
    for (i, (text, extra)) in pages.iter().enumerate() {
        objects.push((
            10 + i as u32,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /F1 4 0 R >> >> /Contents {} 0 R >>",
                50 + i
            ),
        ));
        objects.push((
            50 + i as u32,
            stream_object(
                "",
                &format!("BT /F1 12 Tf 20 250 Td ({text}) Tj ET {extra}"),
            ),
        ));
    }
    file_with(&objects)
}

fn compare(a: &[u8], b: &[u8], options: &CompareOptions) -> ozaru_engine::compare::Comparison {
    let mut a = Document::open(a).unwrap();
    let mut b = Document::open(b).unwrap();
    compare_documents(&mut a, &mut b, options)
}

fn plain(a: &[u8], b: &[u8]) -> ozaru_engine::compare::Comparison {
    compare(a, b, &CompareOptions::default())
}

#[test]
fn the_same_document_is_identical() {
    let a = doc(&[("one two three", ""), ("four five six", "")]);
    let c = plain(&a, &a);
    assert!(c.is_identical());
    assert_eq!(c.pages.len(), 2);
    assert!(
        c.pages
            .iter()
            .all(|p| p.status == PageStatus::Same && p.changes.is_empty())
    );
}

#[test]
fn a_changed_word_is_one_change_with_where_it_was_and_is() {
    let a = doc(&[("the price is 100 dollars", "")]);
    let b = doc(&[("the price is 250 dollars", "")]);
    let c = plain(&a, &b);
    assert!(!c.is_identical());
    let page = &c.pages[0];
    assert_eq!(
        (page.a, page.b, page.status),
        (Some(0), Some(0), PageStatus::Changed)
    );
    assert_eq!(page.changes.len(), 1);
    let change = &page.changes[0];
    assert_eq!(
        (change.removed.as_str(), change.added.as_str()),
        ("100", "250")
    );
    // Both are on the same line, after "the price is ".
    assert_eq!(change.removed_boxes.len(), 1);
    let (old, new) = (change.removed_boxes[0], change.added_boxes[0]);
    assert!(old.left > 60.0 && old.left < 120.0 && (old.left - new.left).abs() < 0.01);
    assert!((old.bottom - 247.0).abs() < 4.0);
}

#[test]
fn several_changes_and_added_or_removed_words() {
    let a = doc(&[("alpha beta gamma delta epsilon zeta", "")]);
    let b = doc(&[("alpha BETA gamma delta zeta eta", "")]);
    let page = &plain(&a, &b).pages[0];
    let found: Vec<(&str, &str)> = page
        .changes
        .iter()
        .map(|c| (c.removed.as_str(), c.added.as_str()))
        .collect();
    assert_eq!(found, [("beta", "BETA"), ("epsilon", ""), ("", "eta")]);
    assert!(page.changes[1].added_boxes.is_empty() && page.changes[2].removed_boxes.is_empty());
}

#[test]
fn text_that_wraps_differently_is_not_a_change() {
    let a = doc(&[("one two three four", "")]);
    let b = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string()),
        (4, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
        (5, stream_object("", "BT /F1 12 Tf 20 250 Td (one two) Tj 0 -30 Td (three) Tj 0 -30 Td (four) Tj ET")),
    ]);
    assert!(plain(&a, &b).is_identical());
}

#[test]
fn an_added_or_removed_page_does_not_change_the_pages_after_it() {
    let pages = [
        ("chapter one has these words in it", ""),
        ("chapter two has other words entirely", ""),
        ("chapter three says something else", ""),
        ("chapter four closes the book", ""),
    ];
    let a = doc(&pages);
    let with_new = doc(&[
        pages[0],
        pages[1],
        ("a brand new page nobody had", ""),
        pages[2],
        pages[3],
    ]);
    let c = plain(&a, &with_new);
    let status: Vec<_> = c.pages.iter().map(|p| (p.a, p.b, p.status)).collect();
    assert_eq!(
        status,
        [
            (Some(0), Some(0), PageStatus::Same),
            (Some(1), Some(1), PageStatus::Same),
            (None, Some(2), PageStatus::Added),
            (Some(2), Some(3), PageStatus::Same),
            (Some(3), Some(4), PageStatus::Same),
        ]
    );
    let c = plain(&with_new, &a);
    assert_eq!(c.pages[2].status, PageStatus::Removed);
    assert_eq!(
        c.pages
            .iter()
            .filter(|p| p.status == PageStatus::Same)
            .count(),
        4
    );
}

#[test]
fn a_page_with_many_edits_is_still_the_same_page_changed() {
    let a = doc(&[
        ("first page", ""),
        ("one two three four five six seven eight nine ten", ""),
        ("last page", ""),
    ]);
    let b = doc(&[
        ("first page", ""),
        (
            "one two THREE four five SIX seven eight NINE ten eleven",
            "",
        ),
        ("last page", ""),
    ]);
    let c = plain(&a, &b);
    let status: Vec<_> = c.pages.iter().map(|p| p.status).collect();
    assert_eq!(
        status,
        [PageStatus::Same, PageStatus::Changed, PageStatus::Same]
    );
    assert_eq!(c.pages[1].changes.len(), 4);
}

#[test]
fn documents_with_nothing_in_common_have_their_pages_removed_and_added() {
    let a = doc(&[("aardvark badger civet dingo", "")]);
    let b = doc(&[("xylophone yodel zither quartz", "")]);
    let statuses: Vec<_> = plain(&a, &b).pages.iter().map(|p| p.status).collect();
    assert_eq!(statuses, [PageStatus::Removed, PageStatus::Added]);
}

#[test]
fn a_page_rewritten_between_pages_that_stay_is_the_same_page_changed() {
    let a = doc(&[
        ("chapter one is here", ""),
        ("aardvark badger civet dingo", ""),
        ("chapter three is here", ""),
    ]);
    let b = doc(&[
        ("chapter one is here", ""),
        ("xylophone yodel zither quartz", ""),
        ("chapter three is here", ""),
    ]);
    let c = plain(&a, &b);
    let status: Vec<_> = c.pages.iter().map(|p| (p.a, p.b, p.status)).collect();
    assert_eq!(
        status,
        [
            (Some(0), Some(0), PageStatus::Same),
            (Some(1), Some(1), PageStatus::Changed),
            (Some(2), Some(2), PageStatus::Same)
        ]
    );
    assert_eq!(c.pages[1].changes.len(), 1);
    // One page for two is not a rewrite: the pages that don't pair stay removed or added.
    let two = doc(&[
        ("chapter one is here", ""),
        ("aardvark badger civet dingo", ""),
        ("ghi jkl mno pqr", ""),
        ("chapter three is here", ""),
    ]);
    let statuses: Vec<_> = plain(&two, &b).pages.iter().map(|p| p.status).collect();
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == PageStatus::Removed)
            .count(),
        2
    );
}

#[test]
fn information_bookmarks_and_fields_are_compared() {
    let base = doc(&[("same text", "")]);
    let edit = |change: &dyn Fn(&mut Editor<'_, '_>)| {
        let mut document = Document::open(&base).unwrap();
        let mut editor = Editor::new(&mut document).unwrap();
        change(&mut editor);
        editor.save().unwrap()
    };
    let a = edit(&|e| {
        e.set_info_text("Title", Some("Old title")).unwrap();
        e.set_info_text("Author", Some("Ana")).unwrap();
        e.add_field(
            0,
            &NewField::new(
                "name",
                [10.0, 10.0, 100.0, 30.0],
                NewFieldKind::Text {
                    value: "Ana".into(),
                    multiline: false,
                    password: false,
                    max_length: None,
                },
            ),
        )
        .unwrap();
        e.add_field(
            0,
            &NewField::new(
                "gone",
                [10.0, 40.0, 100.0, 60.0],
                NewFieldKind::Checkbox { checked: false },
            ),
        )
        .unwrap();
    });
    let b = edit(&|e| {
        e.set_info_text("Title", Some("New title")).unwrap();
        e.set_info_text("Author", Some("Ana")).unwrap();
        e.set_info_text("Subject", Some("Added")).unwrap();
        e.add_field(
            0,
            &NewField::new(
                "name",
                [10.0, 10.0, 100.0, 30.0],
                NewFieldKind::Text {
                    value: "Luis".into(),
                    multiline: false,
                    password: false,
                    max_length: None,
                },
            ),
        )
        .unwrap();
        e.add_field(
            0,
            &NewField::new(
                "new",
                [10.0, 40.0, 100.0, 60.0],
                NewFieldKind::Checkbox { checked: true },
            ),
        )
        .unwrap();
    });
    let c = plain(&a, &b);
    let info: Vec<_> = c
        .info
        .iter()
        .map(|i| (i.key.as_str(), i.a.as_deref(), i.b.as_deref()))
        .collect();
    assert_eq!(
        info,
        [
            ("Subject", None, Some("Added")),
            ("Title", Some("Old title"), Some("New title"))
        ]
    );
    let fields: Vec<_> = c
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.a.as_deref(), f.b.as_deref()))
        .collect();
    assert_eq!(
        fields,
        [
            ("gone", Some("Off"), None),
            ("name", Some("Ana"), Some("Luis")),
            ("new", None, Some("Yes"))
        ]
    );
    assert!(!c.is_identical());
}

#[test]
fn what_only_looks_different_is_found_by_drawing_the_pages() {
    let a = doc(&[("same words", "1 0 0 rg 100 100 50 50 re f")]);
    let b = doc(&[("same words", "0 0 1 rg 100 100 50 50 re f")]);
    // By text they are the same.
    assert!(plain(&a, &b).is_identical());
    let options = CompareOptions {
        visual: true,
        ..CompareOptions::default()
    };
    let c = compare(&a, &b, &options);
    let page = &c.pages[0];
    assert_eq!(page.status, PageStatus::Changed);
    let visual = page.visual.as_ref().unwrap();
    // 2,500 of 90,000 pixels.
    assert!(
        (visual.fraction - 2500.0 / 90000.0).abs() < 0.003,
        "{}",
        visual.fraction
    );
    assert_eq!(visual.regions.len(), 1);
    let r = visual.regions[0];
    assert!(r.left <= 100.0 && r.right >= 150.0 && r.bottom <= 100.0 && r.top >= 150.0);
    assert!(r.width() < 70.0 && r.height() < 70.0);
    // The same pages drawn are the same.
    assert!(compare(&a, &a, &options).is_identical());
}

#[test]
fn pages_of_other_sizes_are_changed_without_comparing_pixels() {
    let a = doc(&[("same words", "")]);
    let b = String::from_utf8_lossy(&a)
        .replace("[0 0 300 300]", "[0 0 300 400]")
        .into_bytes();
    let options = CompareOptions {
        visual: true,
        ..CompareOptions::default()
    };
    let c = compare(&a, &b, &options);
    assert!(c.pages[0].size_changed && c.pages[0].visual.is_none());
    assert_eq!(c.pages[0].status, PageStatus::Changed);
}

#[test]
fn a_picture_of_the_differences_paints_what_each_has() {
    let a = doc(&[("same words", "0 g 100 100 50 50 re f")]);
    let b = doc(&[("same words", "0 g 200 100 50 50 re f")]);
    let draw = |data: &[u8]| {
        let mut document = Document::open(data).unwrap();
        let page = document.page(0).unwrap().clone();
        let limits = document.limits();
        render_page(
            document.resolver(),
            &page,
            &RenderOptions::default(),
            limits,
        )
        .unwrap()
    };
    let (pa, pb) = (draw(&a), draw(&b));
    let (width, height, rgb) = diff_image(&pa, &pb, 32).unwrap();
    assert_eq!((width, height), (300, 300));
    let at = |x: usize, y: usize| {
        [
            rgb[(y * width + x) * 3],
            rgb[(y * width + x) * 3 + 1],
            rgb[(y * width + x) * 3 + 2],
        ]
    };
    // The square only A has is red (point 125, 125 is pixel y = 175), the one only B has is green.
    assert_eq!(at(125, 175), [220, 40, 40]);
    assert_eq!(at(225, 175), [30, 160, 60]);
    assert!(at(10, 10).iter().all(|&c| c > 240));
}

#[test]
fn pages_that_look_the_same_with_different_contents_are_the_same() {
    // The same square drawn with other operators.
    let a = doc(&[("same words", "1 0 0 rg 100 100 50 50 re f")]);
    let b = doc(&[(
        "same words",
        "1 0 0 rg 100 100 m 150 100 l 150 150 l 100 150 l f",
    )]);
    let options = CompareOptions {
        visual: true,
        ..CompareOptions::default()
    };
    let c = compare(&a, &b, &options);
    assert_eq!(c.pages[0].status, PageStatus::Same);
    assert!(
        c.pages[0]
            .visual
            .as_ref()
            .is_some_and(|v| v.fraction == 0.0)
    );
}
