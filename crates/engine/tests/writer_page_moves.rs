//! Inserting, replacing and moving pages.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::text::document_text;
use ozaru_engine::writer::Editor;

mod common;
use common::{file_with, stream_object};

// A document of `count` pages: page `i` says "<label><i>" in Helvetica, and the
// outline has a bookmark for each page. The page labels count them as `i`, `ii`...
fn doc(label: &str, count: usize) -> Vec<u8> {
    let mut objects: Vec<(u32, String)> = Vec::new();
    let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", 10 + i)).collect();
    objects.push((
        1,
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R /PageLabels << /Nums [0 << /S /r >>] >> >>"
            .to_string(),
    ));
    objects.push((
        2,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {count} >>",
            kids.join(" ")
        ),
    ));
    objects.push((
        3,
        format!(
            "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {count} >>",
            100,
            100 + count - 1
        ),
    ));
    objects.push((
        4,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ));
    for i in 0..count {
        objects.push((
            10 + i as u32,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents {} 0 R >>",
                50 + i
            ),
        ));
        objects.push((
            50 + i as u32,
            stream_object(
                "",
                &format!("BT /F1 12 Tf 20 100 Td ({label}{}) Tj ET", i + 1),
            ),
        ));
        let prev = if i > 0 {
            format!("/Prev {} 0 R", 100 + i - 1)
        } else {
            String::new()
        };
        let next = if i + 1 < count {
            format!("/Next {} 0 R", 100 + i + 1)
        } else {
            String::new()
        };
        objects.push((
            100 + i as u32,
            format!(
                "<< /Title ({label}{}) /Parent 3 0 R {prev} {next} /Dest [{} 0 R /Fit] >>",
                i + 1,
                10 + i
            ),
        ));
    }
    file_with(&objects)
}

fn pages_text(data: &[u8]) -> Vec<String> {
    let mut document = Document::open(data).unwrap();
    document_text(&mut document)
        .iter()
        .map(|p| p.text.trim().to_string())
        .collect()
}

fn bookmarks(data: &[u8]) -> Vec<(String, Option<usize>)> {
    let mut document = Document::open(data).unwrap();
    document
        .outlines()
        .into_iter()
        .map(|b| (b.title, b.page))
        .collect()
}

fn edit(data: &[u8], change: impl FnOnce(&mut Editor<'_, '_>)) -> Vec<u8> {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    change(&mut editor);
    editor.save().unwrap()
}

#[test]
fn pages_of_another_document_are_inserted_where_asked() {
    let a = doc("A", 3);
    let b = doc("B", 3);
    for (at, expected) in [
        (0, vec!["B2", "B3", "A1", "A2", "A3"]),
        (1, vec!["A1", "B2", "B3", "A2", "A3"]),
        (3, vec!["A1", "A2", "A3", "B2", "B3"]),
    ] {
        let mut other = Document::open(&b).unwrap();
        let saved = edit(&a, |editor| {
            editor.insert_pages(at, &mut other, &[1, 2]).unwrap()
        });
        assert_eq!(pages_text(&saved), expected, "at {at}");
    }
}

#[test]
fn what_was_there_keeps_its_bookmarks_and_the_labels_go_when_pages_shift() {
    let a = doc("A", 3);
    let b = doc("B", 2);
    let mut other = Document::open(&b).unwrap();
    let saved = edit(&a, |editor| {
        editor.insert_pages(1, &mut other, &[0]).unwrap()
    });
    // The bookmarks of A still go to the pages they went to, which moved.
    let marks = bookmarks(&saved);
    assert_eq!(
        marks,
        [
            ("A1".into(), Some(0)),
            ("A2".into(), Some(2)),
            ("A3".into(), Some(3))
        ]
    );
    assert!(Document::open(&saved).unwrap().page_labels().is_none());
    // At the end nothing shifts: the labels stay.
    let mut other = Document::open(&b).unwrap();
    let saved = edit(&a, |editor| {
        editor.insert_pages(3, &mut other, &[0]).unwrap()
    });
    assert_eq!(
        Document::open(&saved).unwrap().page_labels().unwrap().len(),
        4
    );
    assert_eq!(bookmarks(&saved).len(), 3);
}

#[test]
fn pages_are_replaced_by_pages_of_another_document() {
    let a = doc("A", 4);
    let b = doc("B", 3);
    let mut other = Document::open(&b).unwrap();
    // Page 2 of A is replaced by two pages.
    let saved = edit(&a, |editor| {
        editor.replace_pages(1, 1, &mut other, &[0, 2]).unwrap()
    });
    assert_eq!(pages_text(&saved), ["A1", "B1", "B3", "A3", "A4"]);
    // The page that went isn't in the file.
    assert!(!saved.windows(2).any(|w| w == b"A2"));
    // Two pages for one, and a replacement of the first and the last pages.
    let mut other = Document::open(&b).unwrap();
    let saved = edit(&a, |editor| {
        editor.replace_pages(0, 2, &mut other, &[1]).unwrap();
        let mut again = Document::open(&b).unwrap();
        editor.replace_pages(2, 1, &mut again, &[2]).unwrap();
    });
    assert_eq!(pages_text(&saved), ["B2", "A3", "B3"]);
}

#[test]
fn replacing_nothing_is_inserting_and_every_page_can_go() {
    let a = doc("A", 2);
    let b = doc("B", 2);
    let mut other = Document::open(&b).unwrap();
    let saved = edit(&a, |editor| {
        editor.replace_pages(1, 0, &mut other, &[0]).unwrap()
    });
    assert_eq!(pages_text(&saved), ["A1", "B1", "A2"]);
    let mut other = Document::open(&b).unwrap();
    let saved = edit(&a, |editor| {
        editor.replace_pages(0, 2, &mut other, &[0, 1]).unwrap()
    });
    assert_eq!(pages_text(&saved), ["B1", "B2"]);
}

#[test]
fn pages_are_moved_before_another_and_the_bookmarks_follow_them() {
    let a = doc("A", 5);
    for (indexes, to, expected) in [
        (vec![4], 0, vec!["A5", "A1", "A2", "A3", "A4"]),
        (vec![0], 5, vec!["A2", "A3", "A4", "A5", "A1"]),
        (vec![0, 1], 4, vec!["A3", "A4", "A1", "A2", "A5"]),
        (vec![3, 1], 0, vec!["A4", "A2", "A1", "A3", "A5"]),
        // To before a page that moves: before the next one that stays.
        (vec![1, 2], 2, vec!["A1", "A2", "A3", "A4", "A5"]),
        (vec![1, 1], 4, vec!["A1", "A3", "A4", "A2", "A5"]),
    ] {
        let saved = edit(&a, |editor| editor.move_pages(&indexes, to).unwrap());
        assert_eq!(pages_text(&saved), expected, "{indexes:?} to {to}");
        // Each bookmark still goes to the page with its text.
        for (title, page) in bookmarks(&saved) {
            assert_eq!(pages_text(&saved)[page.unwrap()], title);
        }
    }
}

#[test]
fn bad_places_are_errors_and_leave_the_editor_as_it_was() {
    let a = doc("A", 3);
    let b = doc("B", 2);
    let mut document = Document::open(&a).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let mut other = Document::open(&b).unwrap();
    assert_eq!(
        editor.insert_pages(4, &mut other, &[0]).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(
        editor.insert_pages(0, &mut other, &[5]).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(
        editor
            .replace_pages(2, 2, &mut other, &[0])
            .unwrap_err()
            .kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(
        editor
            .replace_pages(0, 1, &mut other, &[])
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidBox
    );
    assert_eq!(
        editor.move_pages(&[3], 0).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(
        editor.move_pages(&[0], 4).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(editor.page_count(), 3);
    assert_eq!(pages_text(&editor.save().unwrap()), ["A1", "A2", "A3"]);
}
