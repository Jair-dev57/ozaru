//! The engine against other readers, on every PDF of the repository.
//!
//! `tests/corpus/compare.py` reads each file with ozaru 0.1.7 (pdf_oxide), pypdf
//! and Poppler's pdfinfo, and writes what they found to `expected.tsv` and
//! `pages.tsv`, and the ToUnicode CMaps as pdfminer reads them to
//! `tounicode.tsv`. These tests check the engine against them without Python. The
//! corpus itself, and why each file is there, is in `tests/corpus/MANIFEST.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ozaru_engine::base::{Diagnostics, ErrorKind, Limits};
use ozaru_engine::document::Rectangle;
use ozaru_engine::fonts::read_to_unicode;
use ozaru_engine::syntax::ObjectId;
use ozaru_engine::xref::Resolver;
use ozaru_engine::{Document, OpenOptions};

// The user password of the files of the corpus that have one.
const PASSWORD: &[u8] = b"user";

// The files whose user password isn't "user", as the PDF stores it: UTF-8 after
// SASLprep for revision 6, a single byte per character for revision 4.
fn special_password(name: &str) -> Option<&'static [u8]> {
    match name {
        "aes-256-password-ix.pdf" => Some(b"IX"),
        "aes-256-password-space.pdf" => Some(b"a b"),
        "aes-128-password-accent.pdf" => Some(b"a\xF1o"),
        _ => None,
    }
}

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

// Files that no reference can read, with their page count checked by hand.
const ONLY_THE_ENGINE_READS: &[(&str, usize)] = &[
    // The root of its page tree is destroyed, and the second of its three page
    // objects is damaged too: the pages are rebuilt from the file.
    ("tests/damaged.pdf", 3),
];

#[test]
fn the_page_count_matches_the_other_readers() {
    for row in rows("expected.tsv") {
        let [name, ozaru, pypdf, pdfinfo] = &row[..] else {
            panic!("bad row {row:?}");
        };
        let file = std::fs::read(root().join(name)).unwrap();

        let engine = open(&file).map(|document| document.page_count().to_string());

        // ozaru 0.1.7 is the reference: the engine replaces it. Where it can't read
        // the file, the engine must agree with pypdf and pdfinfo if they agree, or
        // give the count checked by hand for the files that only it can read.
        let repaired = ONLY_THE_ENGINE_READS
            .iter()
            .find(|(file, _)| file == name)
            .map(|(_, count)| count.to_string());
        let expected = if ozaru != "error" {
            Some(ozaru.clone())
        } else if pypdf == pdfinfo && pypdf != "error" {
            Some(pypdf.clone())
        } else {
            repaired
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
    // locked, as the phase 4 review says, and the error says why.
    let error = Document::open(&file).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PasswordRequired);

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
        // The files with a special password are opened with it: locked, their
        // information can't be read, which is a diagnostic.
        let name = path.file_name().unwrap().to_str().unwrap();
        let document = match special_password(name) {
            Some(password) => {
                let options = OpenOptions {
                    password: Some(password),
                    ..OpenOptions::default()
                };
                Document::open_with(&file, options).unwrap()
            }
            None => open(&file).unwrap(),
        };

        let diagnostics = document.diagnostics();
        assert!(
            diagnostics.is_empty(),
            "{}: {diagnostics:?}",
            path.display()
        );
    }
}

#[test]
fn the_special_passwords_open_their_files() {
    for name in [
        "aes-256-password-ix.pdf",
        "aes-256-password-space.pdf",
        "aes-128-password-accent.pdf",
    ] {
        let file = corpus(name);
        let options = OpenOptions {
            password: special_password(name),
            ..OpenOptions::default()
        };

        let document = Document::open_with(&file, options).unwrap();

        assert!(!document.needs_password(), "{name}");
        assert!(document.info().is_some(), "{name}");
    }
    // "año" in UTF-8 isn't what revision 4 stores: the Python layer tries both.
    let options = OpenOptions {
        password: Some("año".as_bytes()),
        ..OpenOptions::default()
    };
    assert!(Document::open_with(&corpus("aes-128-password-accent.pdf"), options).is_err());
}

#[test]
fn every_to_unicode_cmap_matches_pdfminer() {
    // The references by file and font: each code with its text.
    let mut expected: HashMap<(String, u32), HashMap<u32, String>> = HashMap::new();
    for row in rows("tounicode.tsv") {
        let points: String = row[3]
            .split(' ')
            .filter(|point| !point.is_empty())
            .map(|point| char::from_u32(u32::from_str_radix(point, 16).unwrap()).unwrap())
            .collect();
        let font = (row[0].clone(), row[1].parse().unwrap());
        expected
            .entry(font)
            .or_default()
            .insert(row[2].parse().unwrap(), points);
    }
    assert!(expected.len() >= 10, "{} fonts", expected.len());

    for ((name, number), codes) in &expected {
        let file = std::fs::read(root().join(name)).unwrap();
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
        let id = ObjectId {
            number: *number,
            generation: 0,
        };
        let font = resolver.resolve(id).unwrap();
        let entry = font.as_dictionary().unwrap().get("ToUnicode").unwrap();
        let mut diagnostics = Diagnostics::new(10);
        let to_unicode = read_to_unicode(
            &mut resolver,
            &entry.clone(),
            Limits::default(),
            0,
            &mut diagnostics,
        )
        .unwrap();
        assert!(diagnostics.is_empty(), "{name} {number}: {diagnostics:?}");
        // Every code that pdfminer maps, the same, and no other.
        for code in 0..=0xFFFF {
            assert_eq!(
                to_unicode.get(code).as_deref(),
                codes.get(&code).map(String::as_str),
                "{name}, font {number}, code {code}"
            );
        }
    }
}

// Where pdfminer is wrong, and the engine follows the standard:
//
// - It reads the Symbol and ZapfDingbats fonts with the standard encoding instead
//   of their built-in ones, so `a` is `a` instead of `α`. pypdf reads them as
//   the engine does.
// - In WinAnsiEncoding, it gives code 173 a space instead of the hyphen of
//   Annex D, note 6.
// - It ignores `/MissingWidth` for codes outside `/Widths`, and replaces the
//   widths of 0 of the standard 14 fonts, which Ghostscript writes for unused
//   codes, with those of the font's metrics.
// - It gives standard encoding names to symbolic TrueType fonts without an
//   `/Encoding`, whose codes have no names (section 9.6.5.4).
// - Its metrics of the standard 14 fonts have no `Euro` glyph, which Adobe's
//   AFM files of 1997 have.
//
// The first two give wrong widths too, since they pick the wrong glyphs.
#[test]
fn every_simple_font_matches_pdfminer() {
    use ozaru_engine::fonts::{BaseEncoding, read_simple_font};
    // The references by file and font: each code with its text and width.
    type Codes = HashMap<u8, (Option<String>, Option<f64>)>;
    let mut expected: HashMap<(String, u32), Codes> = HashMap::new();
    for row in rows("fonts.tsv") {
        let text = (row[3] != "-").then(|| {
            row[3]
                .split(' ')
                .map(|point| char::from_u32(u32::from_str_radix(point, 16).unwrap()).unwrap())
                .collect()
        });
        let width = (row[4] != "-").then(|| row[4].parse().unwrap());
        expected
            .entry((row[0].clone(), row[1].parse().unwrap()))
            .or_default()
            .insert(row[2].parse().unwrap(), (text, width));
    }
    assert!(expected.len() >= 25, "{} fonts", expected.len());

    let mut differences = Vec::new();
    let (mut texts, mut widths) = (0, 0);
    for ((name, number), codes) in &expected {
        let file = corpus(name);
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
        let id = ObjectId {
            number: *number,
            generation: 0,
        };
        let dictionary = resolver.resolve(id).unwrap();
        let mut diagnostics = Diagnostics::new(10);
        let font = read_simple_font(
            &mut resolver,
            dictionary.as_dictionary().unwrap(),
            Limits::default(),
            0,
            &mut diagnostics,
        )
        .unwrap();
        assert!(diagnostics.is_empty(), "{name} {number}: {diagnostics:?}");
        let base = font.encoding().map(|encoding| encoding.base());
        let built_in_symbols = matches!(
            base,
            Some(BaseEncoding::Symbol | BaseEncoding::ZapfDingbats)
        );
        let has_widths = codes.values().any(|(_, width)| width.is_some());

        for code in 0..=255u8 {
            let (text, width) = codes
                .get(&code)
                .cloned()
                .unwrap_or((None, has_widths.then_some(0.0)));
            let engine = font.text(code).map(|text| text.into_owned());
            let win_ansi_hyphen = code == 173
                && base == Some(BaseEncoding::WinAnsi)
                && engine.as_deref() == Some("-");
            let unnamed = font.encoding().is_none() && engine.is_none();
            if !built_in_symbols && !win_ansi_hyphen && !unnamed {
                if engine != text {
                    differences.push(format!(
                        "{name} font {number} code {code}: text {engine:?}, pdfminer {text:?}"
                    ));
                }
                texts += 1;
            }
            let euro = font.glyph_name(code) == Some(b"Euro") && width == Some(0.0);
            if let Some(width) = width
                && font.has_width(code)
                && !built_in_symbols
                && !win_ansi_hyphen
                && !euro
            {
                let zero = font.width(code) == 0.0;
                // pdfminer scales by multiplying by 0.001, which rounds a little
                // differently.
                if (font.width(code) - width).abs() > 1e-9 && !zero {
                    differences.push(format!(
                        "{name} font {number} code {code}: width {}, pdfminer {width}",
                        font.width(code)
                    ));
                }
                widths += 1;
            }
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences:\n{}",
        differences.len(),
        differences.join("\n")
    );
    assert!(
        texts > 5_000 && widths > 4_000,
        "{texts} texts, {widths} widths"
    );
}
