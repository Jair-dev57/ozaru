//! Saving documents: every PDF of the repository is read, saved and read
//! again, and must give the same pages, text, images and information.

use std::path::{Path, PathBuf};

use ozaru_engine::base::{ErrorKind, Limits};
use ozaru_engine::document::Rectangle;
use ozaru_engine::images::document_images;
use ozaru_engine::syntax::{Object, parse_indirect_object};
use ozaru_engine::text::document_text;
use ozaru_engine::writer::Editor;
use ozaru_engine::xref::{XrefEntry, load_xref};
use ozaru_engine::{Document, OpenOptions};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn every_pdf() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = [
        "crates/ozaru-engine/tests/corpus",
        "crates/ozaru-engine/tests/files",
        "docs/assets",
        "tests",
    ]
    .iter()
    .flat_map(|folder| std::fs::read_dir(root().join(folder)).unwrap())
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|e| e == "pdf"))
    .collect();
    paths.sort();
    paths
}

// Opens a file, with its password if it needs one, as the corpus tests do.
fn open<'a>(file: &'a [u8], name: &str) -> Option<Document<'a>> {
    let password: &[u8] = match name {
        "aes-256-password-ix.pdf" => b"IX",
        "aes-256-password-space.pdf" => b"a b",
        "aes-128-password-accent.pdf" => b"a\xF1o",
        _ => b"user",
    };
    let mut document = Document::open(file).ok()?;
    if document.needs_password() && !document.unlock(password) {
        return None;
    }
    Some(document)
}

// What a reader sees of a document.
#[derive(Debug, PartialEq)]
struct Seen {
    pages: Vec<(Rectangle, Rectangle, u16)>,
    text: Vec<String>,
    images: Vec<Vec<(Rectangle, Option<String>)>>,
    info: Option<[Option<String>; 6]>,
}

fn seen(document: &mut Document<'_>) -> Seen {
    let pages = document
        .pages()
        .iter()
        .map(|page| (page.media_box(), page.crop_box(), page.rotation()))
        .collect();
    let info = document.info().map(|info| {
        [
            info.title(),
            info.author(),
            info.subject(),
            info.keywords(),
            info.creator(),
            info.producer(),
        ]
    });
    let text = document_text(document)
        .into_iter()
        .map(|page| page.text)
        .collect();
    let images = document_images(document)
        .into_iter()
        .map(|page| {
            page.images
                .into_iter()
                .map(|image| (image.bbox, image.file.map(|file| format!("{file:?}"))))
                .collect()
        })
        .collect();
    Seen {
        pages,
        text,
        images,
        info,
    }
}

fn save(document: &mut Document<'_>) -> Vec<u8> {
    Editor::new(document).unwrap().save().unwrap()
}

#[test]
fn every_pdf_reads_the_same_after_saving() {
    let mut saved_files = 0;
    for path in every_pdf() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let file = std::fs::read(&path).unwrap();
        let Some(mut document) = open(&file, &name) else {
            continue;
        };
        let before = seen(&mut document);
        let saved = save(&mut document);

        let mut again = Document::open(&saved).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!again.is_encrypted(), "{name}");
        let after = seen(&mut again);
        assert_eq!(after.pages, before.pages, "{name}");
        assert_eq!(after.text, before.text, "{name}");
        assert_eq!(after.info, before.info, "{name}");
        assert_eq!(after.images.len(), before.images.len(), "{name}");
        for (page, (a, b)) in after.images.iter().zip(&before.images).enumerate() {
            assert_eq!(a.len(), b.len(), "{name} page {page}");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                assert_eq!(a.0, b.0, "{name} page {page} image {index}");
                assert!(
                    a.1 == b.1,
                    "{name} page {page} image {index}: {:?} {:?}",
                    a.1.as_ref().map(|s| &s[..80.min(s.len())]),
                    b.1.as_ref().map(|s| &s[..80.min(s.len())])
                );
            }
        }
        saved_files += 1;
    }
    assert!(saved_files > 40, "only {saved_files} files saved");
}

#[test]
fn saved_files_need_no_repair() {
    for path in every_pdf() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let file = std::fs::read(&path).unwrap();
        let Some(mut document) = open(&file, &name) else {
            continue;
        };
        let saved = save(&mut document);

        let loaded = load_xref(&saved, Limits::default()).unwrap();
        assert!(
            loaded.diagnostics.is_empty(),
            "{name}: {:?}",
            loaded.diagnostics
        );
        // Every object is where the table says, and parses without problems.
        for (number, entry) in loaded.table.iter() {
            let XrefEntry::InUse { offset, .. } = entry else {
                continue;
            };
            let parsed = parse_indirect_object(&saved, offset, Limits::default()).unwrap();
            assert_eq!(parsed.object.id.number, number, "{name}");
            assert!(
                parsed.diagnostics.is_empty(),
                "{name} {number}: {:?}",
                parsed.diagnostics
            );
        }
        // Only the problems of the pages themselves, which saving keeps.
        let again = Document::open(&saved).unwrap();
        let before: Vec<_> = document.diagnostics().iter().map(|d| d.kind()).collect();
        for diagnostic in again.diagnostics().iter() {
            assert!(
                before.contains(&diagnostic.kind()),
                "{name}: {diagnostic:?}"
            );
        }
    }
}

#[test]
fn saving_twice_gives_the_same_file() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/object-streams.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    let first = save(&mut document);
    let mut again = Document::open(&first).unwrap();
    let second = save(&mut again);

    assert_eq!(first, second);
}

#[test]
fn encrypted_files_are_saved_decrypted() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/aes-256-user-password.pdf"))
            .unwrap();
    let options = OpenOptions {
        password: Some(b"user"),
        ..OpenOptions::default()
    };
    let mut document = Document::open_with(&file, options).unwrap();
    let saved = save(&mut document);

    let again = Document::open(&saved).unwrap();
    assert!(!again.is_encrypted());
    assert!(!String::from_utf8_lossy(&saved).contains("/Encrypt"));
}

#[test]
fn locked_files_can_not_be_edited() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/files/report-user-password.pdf"))
            .unwrap();
    let mut document = Document::open(&file).unwrap();

    let error = Editor::new(&mut document).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PasswordRequired);
}

#[test]
fn unused_objects_are_left_out() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/incremental-updates.pdf"))
            .unwrap();
    let mut document = Document::open(&file).unwrap();
    let saved = save(&mut document);

    let loaded = load_xref(&saved, Limits::default()).unwrap();
    // Only one section, numbered from 1 without gaps.
    assert!(loaded.table.trailer().get("Prev").is_none());
    let numbers: Vec<u32> = loaded.table.iter().map(|(number, _)| number).collect();
    assert_eq!(numbers, (0..numbers.len() as u32).collect::<Vec<_>>());
    assert!(matches!(
        loaded.table.trailer().get("ID"),
        Some(Object::Array(id)) if id.len() == 2
    ));
}

#[test]
fn files_with_object_streams_are_saved_with_object_streams() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/balanced-tree-compressed.pdf"))
            .unwrap();
    let mut document = Document::open(&file).unwrap();
    let saved = save(&mut document);

    assert!(saved[5..8] >= b"1.5"[..]);
    let loaded = load_xref(&saved, Limits::default()).unwrap();
    assert_eq!(
        loaded.table.trailer().get("Type"),
        Some(&Object::Name("XRef".into()))
    );
    assert!(
        loaded
            .table
            .iter()
            .any(|(_, entry)| matches!(entry, XrefEntry::Compressed { .. }))
    );
    // Smaller than one object per entry would be: about the original's size.
    assert!(saved.len() < file.len() * 2, "{} bytes", saved.len());
}

#[test]
fn files_without_object_streams_keep_a_classic_table() {
    let file = std::fs::read(root().join("crates/ozaru-engine/tests/corpus/one-page.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    let saved = save(&mut document);

    let loaded = load_xref(&saved, Limits::default()).unwrap();
    assert!(loaded.table.trailer().get("Type").is_none());
    assert!(String::from_utf8_lossy(&saved).contains("\nxref\n0 "));
}

#[test]
fn every_pdf_reads_the_same_with_its_pages_reversed() {
    for path in every_pdf() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let file = std::fs::read(&path).unwrap();
        let Some(mut document) = open(&file, &name) else {
            continue;
        };
        let mut before = seen(&mut document);
        let count = document.page_count();
        let mut editor = Editor::new(&mut document).unwrap();
        let reversed: Vec<usize> = (0..count).rev().collect();
        editor.select_pages(&reversed).unwrap();
        let saved = editor.save().unwrap();

        let mut again = Document::open(&saved).unwrap_or_else(|e| panic!("{name}: {e}"));
        let after = seen(&mut again);
        before.pages.reverse();
        before.text.reverse();
        before.images.reverse();
        assert_eq!(after.pages, before.pages, "{name}");
        assert_eq!(after.text, before.text, "{name}");
        assert_eq!(after.images, before.images, "{name}");
        assert_eq!(after.info, before.info, "{name}");
    }
}

#[test]
fn a_page_taken_alone_takes_only_what_it_uses() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/many-pages.pdf")).unwrap();
    let mut document = Document::open(&file).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    editor.select_pages(&[0]).unwrap();
    let saved = editor.save().unwrap();

    assert_eq!(Document::open(&saved).unwrap().page_count(), 1);
    assert!(
        saved.len() * 20 < file.len(),
        "{} of {} bytes",
        saved.len(),
        file.len()
    );
}

#[test]
fn every_pdf_reads_the_same_after_another_one() {
    let first_file = std::fs::read(root().join("docs/assets/report.pdf")).unwrap();
    let mut first = Document::open(&first_file).unwrap();
    let first_seen = seen(&mut first);
    for path in every_pdf() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let file = std::fs::read(&path).unwrap();
        let Some(mut document) = open(&file, &name) else {
            continue;
        };
        let other_seen = seen(&mut document);
        let mut first = Document::open(&first_file).unwrap();
        let mut editor = Editor::new(&mut first).unwrap();
        let all: Vec<usize> = (0..document.page_count()).collect();
        editor.import_pages(&mut document, &all).unwrap();
        let saved = editor.save().unwrap();

        let mut again = Document::open(&saved).unwrap_or_else(|e| panic!("{name}: {e}"));
        let after = seen(&mut again);
        let split = first_seen.pages.len();
        assert_eq!(after.pages[..split], first_seen.pages[..], "{name}");
        assert_eq!(after.pages[split..], other_seen.pages[..], "{name}");
        assert_eq!(after.text[split..], other_seen.text[..], "{name}");
        assert_eq!(after.images[split..], other_seen.images[..], "{name}");
        assert_eq!(after.info, first_seen.info, "{name}");
    }
}

fn rotated_incrementally(file: &[u8]) -> ozaru_engine::base::Result<Vec<u8>> {
    let mut document = Document::open(file).unwrap();
    let mut editor = Editor::new(&mut document)?;
    for index in 0..editor.page_count() {
        editor.rotate_page(index, 90)?;
    }
    editor.save_incremental()
}

#[test]
fn every_pdf_can_be_updated_incrementally_unless_encrypted_or_damaged() {
    let mut updated = 0;
    for path in every_pdf() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let file = std::fs::read(&path).unwrap();
        let Ok(mut document) = Document::open(&file) else {
            continue;
        };
        if document.needs_password() {
            continue;
        }
        let mut before = seen(&mut document);
        let saved = match rotated_incrementally(&file) {
            Ok(saved) => saved,
            Err(error) => {
                let expected = if document.is_encrypted() {
                    ErrorKind::EncryptedUpdate
                } else {
                    ErrorKind::DamagedUpdate
                };
                assert_eq!(error.kind(), expected, "{name}");
                continue;
            }
        };

        // The original bytes are kept, and the update reads without repairs.
        assert!(saved.starts_with(&file), "{name}");
        let loaded = load_xref(&saved, Limits::default()).unwrap();
        assert!(
            loaded.diagnostics.is_empty(),
            "{name}: {:?}",
            loaded.diagnostics
        );
        assert!(loaded.table.trailer().get("Prev").is_some(), "{name}");
        let mut again = Document::open(&saved).unwrap();
        let after = seen(&mut again);
        for page in &mut before.pages {
            page.2 = (page.2 + 90) % 360;
        }
        assert_eq!(after.pages, before.pages, "{name}");
        assert_eq!(after.text, before.text, "{name}");
        assert_eq!(after.images, before.images, "{name}");
        assert_eq!(after.info, before.info, "{name}");
        updated += 1;
    }
    assert!(updated > 25, "only {updated} files updated");
}

#[test]
fn an_update_after_a_stream_section_is_a_stream_section() {
    let file =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/object-streams.pdf")).unwrap();
    let saved = rotated_incrementally(&file).unwrap();

    let loaded = load_xref(&saved, Limits::default()).unwrap();
    let trailer = loaded.table.trailer();
    assert_eq!(trailer.get("Type"), Some(&Object::Name("XRef".into())));
    assert!(trailer.get("Prev").is_some());
}

#[test]
fn an_update_after_a_table_is_a_table_and_keeps_the_first_identifier() {
    let file = std::fs::read(root().join("crates/ozaru-engine/tests/corpus/one-page.pdf")).unwrap();
    let saved = rotated_incrementally(&file).unwrap();

    let update = String::from_utf8_lossy(&saved[file.len()..]).to_string();
    assert!(update.contains("\nxref\n"), "{update}");
    let original = load_xref(&file, Limits::default()).unwrap();
    let loaded = load_xref(&saved, Limits::default()).unwrap();
    assert_eq!(loaded.sections, original.sections + 1);
    let id = |table: &ozaru_engine::xref::XrefTable| table.trailer().get("ID").cloned();
    if let (Some(Object::Array(old)), Some(Object::Array(new))) =
        (id(&original.table), id(&loaded.table))
    {
        assert_eq!(old[0], new[0]);
        assert_ne!(old[1], new[1]);
    }
}

#[test]
fn updating_twice_adds_two_sections() {
    let file = std::fs::read(root().join("crates/ozaru-engine/tests/corpus/one-page.pdf")).unwrap();
    let once = rotated_incrementally(&file).unwrap();
    let twice = rotated_incrementally(&once).unwrap();

    let again = Document::open(&twice).unwrap();
    assert_eq!(again.page(0).unwrap().rotation(), 180);
    assert!(twice.starts_with(&once));
}

#[test]
fn encrypted_and_damaged_files_can_not_be_updated() {
    let encrypted =
        std::fs::read(root().join("crates/ozaru-engine/tests/corpus/aes-128.pdf")).unwrap();
    let damaged = std::fs::read(root().join("tests/damaged.pdf")).unwrap();

    let error = rotated_incrementally(&encrypted).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::EncryptedUpdate);
    let error = rotated_incrementally(&damaged).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::DamagedUpdate);
}
