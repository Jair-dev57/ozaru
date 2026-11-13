mod common;

use common::file_with;
use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::document::Rectangle;
use ozaru_engine::syntax::{Dictionary, Name, Object, ObjectId};
use ozaru_engine::writer::Editor;

// Three pages: the first two inherit their size, rotation and resources from
// the root, the third has its own, and is held by its parent directly.
fn inheriting_file() -> Vec<u8> {
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 3 /MediaBox [0 0 300 400] \
             /Rotate 90 /Resources << /Font << /F1 6 0 R >> >> >>"
                .to_string(),
        ),
        (3, "<< /Type /Page /Parent 2 0 R >>".to_string()),
        (
            4,
            "<< /Type /Pages /Parent 2 0 R /Count 2 /Kids [5 0 R \
             << /Type /Page /Parent 4 0 R /MediaBox [0 0 100 200] /Rotate 0 >>] >>"
                .to_string(),
        ),
        (5, "<< /Type /Page /Parent 4 0 R /Rotate 180 >>".to_string()),
        (
            6,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
    ])
}

fn saved_pages(saved: &[u8]) -> Vec<(Rectangle, u16)> {
    let document = Document::open(saved).unwrap();
    assert!(
        document.diagnostics().is_empty(),
        "{:?}",
        document.diagnostics()
    );
    document
        .pages()
        .iter()
        .map(|page| (page.media_box(), page.rotation()))
        .collect()
}

#[test]
fn rotating_adds_to_the_rotation_a_page_has_or_inherits() {
    let file = inheriting_file();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.rotate_page(0, 90).unwrap();
    editor.rotate_page(1, -90).unwrap();
    editor.rotate_page(2, 450).unwrap();
    let saved = editor.save().unwrap();

    let rotations: Vec<u16> = saved_pages(&saved).iter().map(|page| page.1).collect();
    assert_eq!(rotations, [180, 90, 90]);
}

#[test]
fn rotating_twice_adds_both_turns() {
    let file = inheriting_file();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.rotate_page(0, 90).unwrap();
    editor.rotate_page(0, 90).unwrap();
    editor.rotate_page(0, 90).unwrap();
    let saved = editor.save().unwrap();

    assert_eq!(saved_pages(&saved)[0].1, 0);
}

#[test]
fn rotated_pages_keep_what_they_inherit() {
    let file = inheriting_file();
    let mut document = Document::open(&file).unwrap();
    let before: Vec<(Rectangle, u16)> = document
        .pages()
        .iter()
        .map(|page| (page.media_box(), page.rotation()))
        .collect();
    let fonts_before = document
        .page(1)
        .unwrap()
        .resources()
        .category("Font")
        .cloned();
    let mut editor = Editor::new(&mut document).unwrap();

    // The third page is held directly by its parent: the tree is rebuilt to
    // give it an object of its own.
    editor.rotate_page(2, 0).unwrap();
    let saved = editor.save().unwrap();

    assert_eq!(saved_pages(&saved), before);
    let again = Document::open(&saved).unwrap();
    let fonts_after = again.page(1).unwrap().resources().category("Font").cloned();
    assert_eq!(fonts_after.is_some(), fonts_before.is_some());
    assert!(again.pages().iter().all(|page| page.id().is_some()));
}

#[test]
fn rotations_must_be_quarter_turns_of_existing_pages() {
    let file = inheriting_file();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    let error = editor.rotate_page(0, 45).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidRotation);
    let error = editor.rotate_page(3, 90).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PageNotFound);
}

#[test]
fn objects_can_be_changed_and_added() {
    let file = inheriting_file();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    let mut info = Dictionary::new();
    info.insert(
        Name::from("Title"),
        Object::String(ozaru_engine::syntax::PdfString::new(
            &b"Edited"[..],
            ozaru_engine::syntax::StringFormat::Literal,
        )),
    );
    let info = editor.add(Object::Dictionary(info));
    let mut catalog = editor
        .object(editor.catalog())
        .unwrap()
        .as_dictionary()
        .unwrap()
        .clone();
    catalog.insert(Name::from("Info"), Object::Reference(info));
    let catalog_id = editor.catalog();
    editor.set(catalog_id, Object::Dictionary(catalog));

    assert!(info.number > 6);
    let read = editor.object(catalog_id).unwrap();
    assert_eq!(
        read.as_dictionary().unwrap().get("Info"),
        Some(&Object::Reference(info))
    );
    // The document itself isn't changed.
    drop(editor);
    let catalog = document.resolver().resolve(ObjectId::new(1, 0)).unwrap();
    assert!(catalog.as_dictionary().unwrap().get("Info").is_none());
}

#[test]
fn streams_can_be_added_with_their_data() {
    let file = inheriting_file();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    let id = editor.add_stream(Dictionary::new(), b"0 0 m 10 10 l S".to_vec());

    assert_eq!(editor.stream_data(id).unwrap(), b"0 0 m 10 10 l S");
    assert!(editor.object(id).unwrap().as_stream().is_some());
}

#[test]
fn damaged_page_trees_are_saved_whole() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/damaged.pdf");
    let file = std::fs::read(root).unwrap();
    let mut document = Document::open(&file).unwrap();
    let count = document.page_count();

    let saved = Editor::new(&mut document).unwrap().save().unwrap();

    let again = Document::open(&saved).unwrap();
    assert_eq!(again.page_count(), count);
    assert!(again.pages().iter().all(|page| !page.is_damaged()));
}

// Three pages, each with its own text, an outline and a link on the first
// page to the third, and document-wide entries that point at pages.
fn three_pages() -> Vec<u8> {
    let content =
        |text: &str| common::stream_object("", &format!("BT /F1 12 Tf 72 700 Td ({text}) Tj ET"));
    file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Outlines 10 0 R /PageMode /UseOutlines \
             /OpenAction [5 0 R /Fit] /PageLabels << /Nums [0 << /S /r >>] >> \
             /Names << /Dests 12 0 R /JavaScript 13 0 R >> >>"
                .to_string(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 9 0 R >> >> >>"
                .to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Annots [<< /Type /Annot \
             /Subtype /Link /Rect [0 0 10 10] /Dest [5 0 R /Fit] >>] >>"
                .to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>".to_string(),
        ),
        (
            5,
            "<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>".to_string(),
        ),
        (6, content("Page one")),
        (7, content("Page two")),
        (8, content("Page three")),
        (
            9,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
        (
            10,
            "<< /Type /Outlines /First 11 0 R /Last 11 0 R /Count 1 >>".to_string(),
        ),
        (
            11,
            "<< /Title (Three) /Parent 10 0 R /Dest [5 0 R /Fit] >>".to_string(),
        ),
        (12, "<< /Names [(three) [5 0 R /Fit]] >>".to_string()),
        (13, "<< /Names [] >>".to_string()),
    ])
}

fn texts(saved: &[u8]) -> Vec<String> {
    let mut document = Document::open(saved).unwrap();
    assert!(
        document.diagnostics().is_empty(),
        "{:?}",
        document.diagnostics()
    );
    ozaru_engine::text::document_text(&mut document)
        .into_iter()
        .map(|page| page.text.trim().to_string())
        .collect()
}

#[test]
fn pages_can_be_kept_moved_and_repeated() {
    let file = three_pages();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.select_pages(&[2, 0, 2]).unwrap();
    assert_eq!(editor.page_count(), 3);
    let saved = editor.save().unwrap();

    assert_eq!(texts(&saved), ["Page three", "Page one", "Page three"]);
    let again = Document::open(&saved).unwrap();
    assert_ne!(again.page(0).unwrap().id(), again.page(2).unwrap().id());
    assert_eq!(
        again.page(1).unwrap().media_box(),
        Rectangle::new(0.0, 0.0, 612.0, 792.0)
    );
}

#[test]
fn pages_left_out_are_not_written() {
    let file = three_pages();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.select_pages(&[0]).unwrap();
    let saved = editor.save().unwrap();

    assert_eq!(texts(&saved), ["Page one"]);
    let written = String::from_utf8_lossy(&saved);
    // The link on the first page pointed at the third, which isn't written.
    assert!(!written.contains("Page three"), "{written}");
    assert!(!written.contains("Page two"));
    assert!(written.contains("/Dest [null /Fit]"), "{written}");
}

#[test]
fn entries_that_point_at_pages_are_removed() {
    let file = three_pages();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.select_pages(&[0, 1]).unwrap();
    let catalog_id = editor.catalog();
    let catalog = editor.object(catalog_id).unwrap();
    let catalog = catalog.as_dictionary().unwrap();

    for key in ["Outlines", "PageMode", "OpenAction", "PageLabels"] {
        assert!(catalog.get(key).is_none(), "{key}");
    }
    let names = catalog.get("Names").unwrap().as_dictionary().unwrap();
    assert!(names.get("Dests").is_none());
    assert!(names.get("JavaScript").is_some());
}

#[test]
fn selecting_a_page_that_does_not_exist_changes_nothing() {
    let file = three_pages();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    let error = editor.select_pages(&[0, 3]).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::PageNotFound);
    assert_eq!(editor.page_count(), 3);
    let saved = editor.save().unwrap();
    assert_eq!(texts(&saved), ["Page one", "Page two", "Page three"]);
}

#[test]
fn selected_pages_can_be_rotated_and_selected_again() {
    let file = three_pages();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.select_pages(&[1, 2]).unwrap();
    editor.rotate_page(0, 90).unwrap();
    editor.select_pages(&[1, 0]).unwrap();
    let saved = editor.save().unwrap();

    assert_eq!(texts(&saved), ["Page three", "Page two"]);
    let rotations: Vec<u16> = saved_pages(&saved).iter().map(|page| page.1).collect();
    assert_eq!(rotations, [0, 90]);
}

#[test]
fn pages_of_other_documents_are_added_with_what_they_use() {
    let first = inheriting_file();
    let second = three_pages();
    let mut document = Document::open(&first).unwrap();
    let mut other = Document::open(&second).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.import_pages(&mut other, &[2, 0]).unwrap();
    assert_eq!(editor.page_count(), 5);
    let saved = editor.save().unwrap();

    let texts = texts(&saved);
    assert_eq!(&texts[3..], ["Page three", "Page one"]);
    let pages = saved_pages(&saved);
    assert_eq!(pages[3].0, Rectangle::new(0.0, 0.0, 612.0, 792.0));
    // Both pages use Helvetica: it is copied once, as is the first file's.
    let written = String::from_utf8_lossy(&saved);
    assert_eq!(
        written.matches("/BaseFont /Helvetica").count(),
        2,
        "{written}"
    );
    // The link of the first page pointed at the third, imported too.
    assert!(!written.contains("/Dest [null"), "{written}");
}

#[test]
fn links_to_pages_not_imported_become_null() {
    let first = inheriting_file();
    let second = three_pages();
    let mut document = Document::open(&first).unwrap();
    let mut other = Document::open(&second).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.import_pages(&mut other, &[0]).unwrap();
    let saved = editor.save().unwrap();

    let written = String::from_utf8_lossy(&saved);
    assert!(written.contains("/Dest [null /Fit]"), "{written}");
    assert!(!written.contains("Page three"));
}

#[test]
fn a_document_can_import_its_own_pages() {
    let file = three_pages();
    let copy = file.clone();
    let mut document = Document::open(&file).unwrap();
    let mut other = Document::open(&copy).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.import_pages(&mut other, &[1, 1]).unwrap();
    let saved = editor.save().unwrap();

    assert_eq!(
        texts(&saved),
        ["Page one", "Page two", "Page three", "Page two", "Page two"]
    );
}

#[test]
fn objects_that_refer_to_each_other_are_copied_once() {
    let second = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>".to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".to_string(),
        ),
        (
            4,
            "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] /P 3 0 R /Popup 5 0 R >>".to_string(),
        ),
        (
            5,
            "<< /Type /Annot /Subtype /Popup /Rect [0 0 50 50] /Parent 4 0 R /P 3 0 R >>"
                .to_string(),
        ),
    ]);
    let first = inheriting_file();
    let mut document = Document::open(&first).unwrap();
    let mut other = Document::open(&second).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.import_pages(&mut other, &[0]).unwrap();
    let saved = editor.save().unwrap();

    let written = String::from_utf8_lossy(&saved);
    assert_eq!(written.matches("/Subtype /Text").count(), 1);
    assert_eq!(written.matches("/Subtype /Popup").count(), 1);
    let again = Document::open(&saved).unwrap();
    let page = again.page(3).unwrap().id().unwrap();
    // The annotations point at the page they were copied with.
    assert_eq!(
        written.matches(&format!("/P {} 0 R", page.number)).count(),
        2
    );
}

#[test]
fn importing_from_a_locked_document_or_past_its_pages_changes_nothing() {
    let first = inheriting_file();
    let locked_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/files/report-user-password.pdf");
    let locked = std::fs::read(locked_path).unwrap();
    let second = three_pages();
    let mut document = Document::open(&first).unwrap();
    let mut other = Document::open(&second).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    let error = editor.import_pages(&mut other, &[3]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PageNotFound);
    if let Ok(mut locked) = Document::open(&locked) {
        let error = editor.import_pages(&mut locked, &[0]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::PasswordRequired);
    }
    assert_eq!(editor.page_count(), 3);
}

#[test]
fn objects_that_can_not_be_read_are_null_where_they_are_referred_to() {
    let second = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>".to_string(),
        ),
        // Its content is object 4, which doesn't exist.
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".to_string(),
        ),
    ]);
    let first = inheriting_file();
    let mut document = Document::open(&first).unwrap();
    let mut other = Document::open(&second).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();

    editor.import_pages(&mut other, &[0]).unwrap();
    let saved = editor.save().unwrap();

    assert!(String::from_utf8_lossy(&saved).contains("/Contents null"));
}
