//! The engine against other readers, on every PDF of the repository.
//!
//! `tests/corpus/compare.py` reads each file with ozaru 0.1.7 (pdf_oxide), pypdf
//! and Poppler's pdfinfo, and writes what they found to `expected.tsv` and
//! `pages.tsv`. These tests check the engine against them without Python. The
//! corpus itself, and why each file is there, is in `tests/corpus/MANIFEST.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ozaru_engine::document::Rectangle;
use ozaru_engine::{Document, OpenOptions};

// The user password of the files of the corpus that have one.
const PASSWORD: &[u8] = b"user";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// The lines of a TSV file of the corpus, split, without comments and header.
fn rows(name: &str) -> Vec<Vec<String>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .skip(1)
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

// Opens a file locked, as `page_count` does, and with the password if it can't
// be read locked.
fn open(file: &[u8]) -> Option<Document<'_>> {
    Document::open(file).ok().or_else(|| {
        let options = OpenOptions {
            password: Some(PASSWORD),
            ..OpenOptions::default()
        };
        Document::open_with(file, options).ok()
    })
}

#[test]
fn every_pdf_of_the_repository_is_compared() {
    let mut listed: Vec<String> = rows("expected.tsv")
        .into_iter()
        .map(|row| row[0].clone())
        .collect();
    let mut found: Vec<String> = [
        "crates/ozaru-engine/tests/corpus",
        "crates/ozaru-engine/tests/files",
        "docs/assets",
        "tests",
    ]
    .iter()
    .flat_map(|folder| std::fs::read_dir(root().join(folder)).unwrap())
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|e| e == "pdf"))
    .map(|path| {
        path.strip_prefix(root())
            .unwrap()
            .to_string_lossy()
            // The same names on Windows.
            .replace('\\', "/")
    })
    .collect();
    listed.sort();
    found.sort();

    assert_eq!(
        listed, found,
        "run tests/corpus/compare.py after adding a PDF"
    );
}

#[test]
fn the_page_count_matches_the_other_readers() {
    for row in rows("expected.tsv") {
        let [name, ozaru, pypdf, pdfinfo] = &row[..] else {
            panic!("bad row {row:?}");
        };
        let file = std::fs::read(root().join(name)).unwrap();

        let engine = open(&file).map(|document| document.page_count().to_string());

        // ozaru 0.1.7 is the reference: the engine replaces it. Where it can't read
        // the file, the engine must agree with pypdf and pdfinfo if they agree.
        let expected = if ozaru != "error" {
            Some(ozaru.clone())
        } else if pypdf == pdfinfo && pypdf != "error" {
            Some(pypdf.clone())
        } else {
            None
        };
        assert_eq!(
            engine, expected,
            "{name}: ozaru {ozaru}, pypdf {pypdf}, pdfinfo {pdfinfo}"
        );
    }
}

fn rectangle(numbers: &[String]) -> Rectangle {
    let n: Vec<f64> = numbers.iter().map(|n| n.parse().unwrap()).collect();
    Rectangle::new(n[0], n[1], n[2], n[3])
}

fn close(a: Rectangle, b: Rectangle) -> bool {
    [
        (a.left, b.left),
        (a.bottom, b.bottom),
        (a.right, b.right),
        (a.top, b.top),
    ]
    .iter()
    .all(|(x, y)| (x - y).abs() < 0.01)
}

#[test]
fn the_boxes_and_rotation_of_each_page_match_pypdf() {
    let mut by_file: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for row in rows("pages.tsv") {
        by_file.entry(row[0].clone()).or_default().push(row);
    }
    let mut checked = 0;
    for (name, pages) in by_file {
        let file = std::fs::read(root().join(&name)).unwrap();
        let document = open(&file).unwrap_or_else(|| panic!("{name} should open"));
        // The first 50 pages are listed.
        assert_eq!(document.page_count().min(50), pages.len(), "{name}");

        for row in pages {
            let index: usize = row[1].parse().unwrap();
            let page = document.page(index).unwrap();
            let label = format!("{name}, page {}", index + 1);
            // pypdf doesn't read damaged pages either: it gives the defaults.
            if page.is_damaged() {
                continue;
            }
            let media_box = rectangle(&row[2..6]);
            assert!(
                close(page.media_box(), media_box),
                "{label}: {:?}",
                page.media_box()
            );
            // pypdf doesn't clip the crop box to the media box, so it is only
            // compared when it is inside.
            let crop_box = rectangle(&row[6..10]);
            if crop_box.intersection(&media_box) == Some(crop_box) {
                assert!(
                    close(page.crop_box(), crop_box),
                    "{label}: {:?}",
                    page.crop_box()
                );
            }
            assert_eq!(page.rotation().to_string(), row[10], "{label}");
            checked += 1;
        }
    }
    assert!(checked > 250, "only {checked} pages checked");
}

// What the corpus was made for, file by file.

fn corpus(name: &str) -> Vec<u8> {
    std::fs::read(root().join("crates/ozaru-engine/tests/corpus").join(name)).unwrap()
}

#[test]
fn the_aes_files_are_decrypted() {
    for name in ["aes-128.pdf", "aes-256.pdf"] {
        let file = corpus(name);

        let mut document = Document::open(&file).unwrap();

        assert!(document.is_encrypted(), "{name}");
        assert!(!document.needs_password(), "{name}");
        let title = document.info().and_then(|info| info.title());
        assert_eq!(title.as_deref(), Some("sizes-and-rotations.pdf"), "{name}");
        // Every page's content decrypts and decodes to the text ReportLab wrote.
        for index in 0..document.page_count() {
            let contents = document
                .page(index)
                .unwrap()
                .dictionary()
                .get("Contents")
                .cloned();
            let id = contents.and_then(|c| c.as_reference()).unwrap();
            let data = document.resolver().stream_data(id).unwrap().data;
            let text = format!("(Page {} of 6) Tj", index + 1);
            assert!(
                data.windows(text.len()).any(|w| w == text.as_bytes()),
                "{name}, page {}",
                index + 1
            );
        }
        assert!(document.diagnostics().is_empty(), "{name}");
    }
}

#[test]
fn the_aes_256_file_with_a_user_password_opens_with_it() {
    let file = corpus("aes-256-user-password.pdf");

    // Its page tree is inside an encrypted object stream, so it can't be read
    // locked, as the phase 4 review says.
    assert!(Document::open(&file).is_err());

    let options = OpenOptions {
        password: Some(PASSWORD),
        ..OpenOptions::default()
    };
    let document = Document::open_with(&file, options).unwrap();
    assert_eq!(document.page_count(), 6);
    let title = document.info().and_then(|info| info.title());
    assert_eq!(title.as_deref(), Some("sizes-and-rotations.pdf"));
}

#[test]
fn the_title_with_accents_is_decoded() {
    let file = corpus("unicode-info.pdf");

    let document = Document::open(&file).unwrap();

    let info = document.info().unwrap();
    assert_eq!(info.title().as_deref(), Some("Informe año 2026 — ñandú ✓"));
    assert_eq!(info.author().as_deref(), Some("José Núñez"));
}

#[test]
fn the_incremental_updates_are_followed() {
    let file = corpus("incremental-updates.pdf");

    let document = Document::open(&file).unwrap();

    // Page 4 was removed by the last update, and page 6 added by the first.
    let numbers: Vec<_> = document
        .pages()
        .iter()
        .map(|page| page.id().unwrap().number)
        .collect();
    assert_eq!(numbers, [5, 6]);
    assert!(document.diagnostics().is_empty());
}

#[test]
fn the_inherited_attributes_are_applied() {
    let file = corpus("inherited-attributes.pdf");

    let document = Document::open(&file).unwrap();

    let pages = document.pages();
    assert_eq!(pages[0].media_box(), Rectangle::LETTER);
    assert_eq!(pages[0].rotation(), 90);
    assert_eq!(pages[1].rotation(), 0);
    assert_eq!(pages[2].media_box(), Rectangle::new(0.0, 0.0, 595.0, 842.0));
    assert_eq!(pages[2].crop_box(), Rectangle::new(0.0, 0.0, 420.0, 595.0));
    assert_eq!(pages[2].rotation(), 270);
    assert!(
        pages
            .iter()
            .all(|page| page.resources().category("Font").is_some())
    );
}

#[test]
fn every_file_of_the_corpus_opens_without_diagnostics() {
    for entry in std::fs::read_dir(root().join("crates/ozaru-engine/tests/corpus")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "pdf") {
            continue;
        }
        let file = std::fs::read(&path).unwrap();

        let document = open(&file).unwrap();

        let diagnostics = document.diagnostics();
        assert!(
            diagnostics.is_empty(),
            "{}: {diagnostics:?}",
            path.display()
        );
    }
}
