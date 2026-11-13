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
