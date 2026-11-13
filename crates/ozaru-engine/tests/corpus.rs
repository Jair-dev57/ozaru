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
use ozaru_engine::content::Interpreter;
use ozaru_engine::document::Rectangle;
use ozaru_engine::fonts::read_to_unicode;
use ozaru_engine::syntax::{Object, ObjectId};
use ozaru_engine::text::document_text;
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
//   `/Encoding`, whose codes have no names (section 9.6.5.4): their glyphs are
//   found through the font program's tables, which `no-tounicode.tsv` checks.
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
            let hyphen = code == 173
                && base == Some(BaseEncoding::WinAnsi)
                && engine.as_deref() == Some("-");
            // pdfminer leaves the unused codes of WinAnsiEncoding undefined,
            // where the standard makes them bullets.
            let bullet = code != 0x95
                && base == Some(BaseEncoding::WinAnsi)
                && font.glyph_name(code) == Some(b"bullet");
            let win_ansi_hyphen = hyphen || bullet;
            // no-tounicode.tsv checks the text of these fonts instead.
            let unnamed = font.encoding().is_none();
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

#[test]
fn every_composite_font_matches_pdfminer() {
    use ozaru_engine::fonts::{Code, read_composite_font};
    let rows = rows("composite.tsv");
    assert!(rows.len() >= 100, "{} CIDs", rows.len());
    let mut differences = Vec::new();
    for row in &rows {
        let (name, number, cmap) = (&row[0], row[1].parse().unwrap(), &row[2]);
        let cid: u32 = row[3].parse().unwrap();
        let text = (row[4] != "-").then(|| {
            row[4]
                .split(' ')
                .map(|point| char::from_u32(u32::from_str_radix(point, 16).unwrap()).unwrap())
                .collect::<String>()
        });
        let width: f64 = row[5].parse().unwrap();
        assert!(
            cmap == "Identity-H" || cmap == "Identity-V",
            "the corpus only has the Identity CMaps"
        );

        let file = corpus(name);
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
        let dictionary = resolver
            .resolve(ObjectId {
                number,
                generation: 0,
            })
            .unwrap();
        let mut diagnostics = Diagnostics::new(10);
        let font = read_composite_font(
            &mut resolver,
            dictionary.as_dictionary().unwrap(),
            Limits::default(),
            0,
            &mut diagnostics,
        )
        .unwrap();
        assert!(diagnostics.is_empty(), "{name} {number}: {diagnostics:?}");
        // With Identity-H and Identity-V, the code of a CID is its two bytes.
        let code = Code {
            value: cid,
            length: 2,
        };
        assert_eq!(font.cid(code), Some(cid));
        let engine = font.text(code).map(|text| text.into_owned());
        // pdfminer gives the .notdef glyph U+FFFF, which ends the last segment of
        // a format 4 cmap subtable and isn't a character.
        let notdef = cid == 0 && text.as_deref() == Some("\u{FFFF}") && engine.is_none();
        if engine != text && !notdef {
            differences.push(format!(
                "{name} font {number} CID {cid}: text {engine:?}, pdfminer {text:?}"
            ));
        }
        // pdfminer gives the vertical width of the CIDs of vertical fonts.
        let engine_width = if font.cmap().is_vertical() {
            font.vertical_metrics(code).width
        } else {
            font.width(code)
        };
        if (engine_width - width).abs() > 1e-9 {
            differences.push(format!(
                "{name} font {number} CID {cid}: width {engine_width}, pdfminer {width}"
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

// fonts-no-tounicode.pdf has no ToUnicode CMaps: the text is only in the tables
// of its embedded TrueType programs. pdfminer reads the CIDFont's, and PyMuPDF
// and pypdf neither. no-tounicode.tsv has the real text of each code.
#[test]
fn the_text_of_fonts_without_to_unicode_comes_from_their_programs() {
    use ozaru_engine::fonts::{Code, read_composite_font, read_simple_font};
    let file = corpus("fonts-no-tounicode.pdf");
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let mut font = |number| {
        let id = ObjectId {
            number,
            generation: 0,
        };
        resolver.resolve(id).unwrap()
    };
    let (composite, simple) = (font(5), font(8));
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let mut diagnostics = Diagnostics::new(10);
    let limits = Limits::default();
    let composite = read_composite_font(
        &mut resolver,
        composite.as_dictionary().unwrap(),
        limits,
        0,
        &mut diagnostics,
    )
    .unwrap();
    let simple = read_simple_font(
        &mut resolver,
        simple.as_dictionary().unwrap(),
        limits,
        0,
        &mut diagnostics,
    )
    .unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(composite.to_unicode().is_none() && simple.to_unicode().is_none());
    assert!(simple.encoding().is_none());

    let rows = rows("no-tounicode.tsv");
    assert!(rows.len() > 30);
    for row in rows {
        let code: u32 = row[1].parse().unwrap();
        let expected = char::from_u32(u32::from_str_radix(&row[2], 16).unwrap())
            .unwrap()
            .to_string();
        let text = match row[0].as_str() {
            "5" => composite.text(Code {
                value: code,
                length: 2,
            }),
            _ => simple.text(u8::try_from(code).unwrap()),
        };
        assert_eq!(
            text.as_deref(),
            Some(expected.as_str()),
            "font {} code {code}",
            row[0]
        );
    }
}

#[test]
fn every_font_of_the_corpus_reads_without_diagnostics() {
    use ozaru_engine::fonts::{read_composite_font, read_simple_font};
    use ozaru_engine::syntax::Object;
    let (mut simple, mut composite) = (0, 0);
    for entry in std::fs::read_dir(root().join("crates/ozaru-engine/tests/corpus")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "pdf") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let file = std::fs::read(&path).unwrap();
        let mut document = match special_password(&name) {
            Some(password) => {
                let options = OpenOptions {
                    password: Some(password),
                    ..OpenOptions::default()
                };
                Document::open_with(&file, options).unwrap()
            }
            None => open(&file).unwrap(),
        };
        // The font dictionary of each page, directly or through a reference.
        let categories: Vec<Object> = document
            .pages()
            .iter()
            .filter_map(|page| page.resources().category("Font").cloned())
            .collect();
        let mut fonts = Vec::new();
        for category in categories {
            let category = match category {
                Object::Reference(id) => (*document.resolver().resolve(id).unwrap()).clone(),
                category => category,
            };
            let dictionary = category.as_dictionary().unwrap();
            fonts.extend(
                dictionary
                    .iter()
                    .filter_map(|(_, font)| font.as_reference()),
            );
        }
        fonts.sort_by_key(|id| id.number);
        fonts.dedup();
        let resolver = document.resolver();
        for id in fonts {
            let font = resolver.resolve(id).unwrap();
            let dictionary = font.as_dictionary().unwrap();
            let mut diagnostics = Diagnostics::new(10);
            let limits = Limits::default();
            if read_simple_font(resolver, dictionary, limits, 0, &mut diagnostics).is_some() {
                simple += 1;
            } else if read_composite_font(resolver, dictionary, limits, 0, &mut diagnostics)
                .is_some()
            {
                composite += 1;
            } else {
                panic!("{name} {id:?} is neither");
            }
            assert!(diagnostics.is_empty(), "{name} {id:?}: {diagnostics:?}");
        }
    }
    assert!(
        simple >= 40 && composite >= 5,
        "{simple} simple, {composite} composite"
    );
}

// Opens a file and unlocks it with its user password, if it needs one.
fn open_unlocked<'a>(name: &str, file: &'a [u8]) -> Option<Document<'a>> {
    let mut document = open(file)?;
    if document.needs_password() {
        let file_name = name.rsplit('/').next().unwrap();
        let password = special_password(file_name).unwrap_or(PASSWORD);
        if !document.unlock(password) {
            return None;
        }
    }
    Some(document)
}

// A glyph as glyphs.tsv lists it: its font's /BaseFont, the font size, its
// origin and its width.
#[derive(Debug, Clone, PartialEq)]
struct PlacedGlyph {
    font: String,
    size: f64,
    x: f64,
    y: f64,
    width: f64,
}

// Where pdfminer gives a glyph another width than the engine, it departs from
// the standard, as the phase 6 review documents: it has no `Euro` in the
// metrics of the standard 14 fonts, reads Symbol and ZapfDingbats with the
// standard encoding, and gives WinAnsi code 173 the width of a space.
fn documented_width_difference(font: &str, text: Option<&str>, code: u32) -> bool {
    text == Some("€") || font == "Symbol" || font == "ZapfDingbats" || code == 173
}

#[test]
fn every_glyph_is_where_pdfminer_places_it() {
    // The glyphs of each page of each file, as pdfminer places them.
    let mut expected: HashMap<(String, usize), Vec<PlacedGlyph>> = HashMap::new();
    let mut files: Vec<String> = Vec::new();
    for row in rows("glyphs.tsv") {
        if files.last() != Some(&row[0]) {
            files.push(row[0].clone());
        }
        expected
            .entry((row[0].clone(), row[1].parse().unwrap()))
            .or_default()
            .push(PlacedGlyph {
                font: row[3].clone(),
                size: row[4].parse().unwrap(),
                x: row[5].parse().unwrap(),
                y: row[6].parse().unwrap(),
                width: row[7].parse().unwrap(),
            });
    }
    assert!(files.len() >= 35, "{} files", files.len());

    let mut glyphs = 0;
    let mut width_differences = 0;
    let mut differences = Vec::new();
    for name in &files {
        let file = std::fs::read(root().join(name)).unwrap();
        let mut document = open_unlocked(name, &file).unwrap_or_else(|| panic!("{name}"));
        let pages = document.pages().to_vec();
        let mut interpreter = Interpreter::new(Limits::default());
        for (index, page) in pages.iter().take(10).enumerate() {
            let content = interpreter.run_page(document.resolver(), page);
            let wanted = expected
                .get(&(name.clone(), index + 1))
                .cloned()
                .unwrap_or_default();
            let place = format!("{name} page {}", index + 1);
            // A page the engine can't read, as ozaru doesn't, while pdfminer
            // reads around the damaged byte in its /MediaBox.
            if page.is_damaged() {
                if !wanted.is_empty() {
                    assert_eq!(place, "tests/page_error.pdf page 3");
                }
                continue;
            }
            if content.glyphs.len() != wanted.len() {
                differences.push(format!(
                    "{place}: {} glyphs, pdfminer {}",
                    content.glyphs.len(),
                    wanted.len()
                ));
                continue;
            }
            // How far a documented width difference earlier on the line has
            // moved pdfminer's glyphs from the engine's.
            let mut shift = (0.0, 0.0);
            for (number, (glyph, theirs)) in content.glyphs.iter().zip(&wanted).enumerate() {
                glyphs += 1;
                let font = glyph.font.base_font().map_or("-".to_string(), |name| {
                    String::from_utf8_lossy(name.as_bytes()).into_owned()
                });
                // pdfminer places vertical glyphs at their vertical origin.
                let (x, y) = match &glyph.vertical {
                    Some(v) => glyph.matrix.apply(v.origin_x, v.origin_y),
                    None => glyph.origin(),
                };
                let at = |dx: f64, dy: f64| {
                    (x - theirs.x - dx).abs() < 0.01 && (y - theirs.y - dy).abs() < 0.01
                };
                if at(0.0, 0.0) {
                    shift = (0.0, 0.0);
                } else if !at(shift.0, shift.1) {
                    differences.push(format!(
                        "{place} glyph {number}: at ({x:.3}, {y:.3}), pdfminer {theirs:?}"
                    ));
                }
                if font != theirs.font || (glyph.font_size - theirs.size).abs() > 1e-6 {
                    differences.push(format!(
                        "{place} glyph {number}: {font} {}, pdfminer {theirs:?}",
                        glyph.font_size
                    ));
                }
                // pdfminer gives vertical glyphs their vertical width.
                let width = glyph.vertical.map_or(glyph.width, |v| v.width);
                if (width - theirs.width).abs() > 1e-6 {
                    width_differences += 1;
                    if !documented_width_difference(&font, glyph.text.as_deref(), glyph.code.value)
                    {
                        differences.push(format!(
                            "{place} glyph {number}: width {}, pdfminer {theirs:?}",
                            glyph.width
                        ));
                    }
                    let gap = width - theirs.width;
                    let (dx, dy) = glyph.matrix.apply(gap, 0.0);
                    let (ox, oy) = glyph.matrix.apply(0.0, 0.0);
                    shift = (shift.0 + dx - ox, shift.1 + dy - oy);
                }
            }
        }
    }
    eprintln!("{glyphs} glyphs, {width_differences} with documented width differences");
    assert!(glyphs > 5000, "{glyphs} glyphs");
    assert!(
        differences.is_empty(),
        "{} differences:\n{}",
        differences.len(),
        differences[..differences.len().min(30)].join("\n")
    );
}

// Text with each run of whitespace as one space, and none at the ends, as the
// migration criteria compare texts.
fn normalized(text: &str) -> Vec<char> {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect()
}

// How alike two texts are, from 0 to 1: twice the characters they have in
// common, in order, over the characters of both, as Python's difflib reports
// it for most texts. The common characters come from the edit distance,
// with insertions and deletions only, found with Myers' algorithm, which is
// fast when the texts are alike.
fn similarity(a: &[char], b: &[char]) -> f64 {
    let (n, m) = (a.len(), b.len());
    if n + m == 0 {
        return 1.0;
    }
    let max = n + m;
    let offset = max as isize;
    let mut furthest = vec![0usize; 2 * max + 2];
    for distance in 0..=max as isize {
        let mut k = -distance;
        while k <= distance {
            let index = (k + offset) as usize;
            let mut x =
                if k == -distance || (k != distance && furthest[index - 1] < furthest[index + 1]) {
                    furthest[index + 1]
                } else {
                    furthest[index - 1] + 1
                };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && a[x] == b[y] {
                x += 1;
                y += 1;
            }
            furthest[index] = x;
            if x >= n && y >= m {
                return 1.0 - distance as f64 / max as f64;
            }
            k += 2;
        }
    }
    0.0
}

#[test]
fn similarity_counts_the_characters_in_common() {
    let text = |s: &str| s.chars().collect::<Vec<_>>();
    assert_eq!(similarity(&text("abc"), &text("abc")), 1.0);
    assert_eq!(similarity(&text("abcd"), &text("abxd")), 0.75);
    assert_eq!(similarity(&text("ab"), &text("cd")), 0.0);
    assert_eq!(normalized(" a \n\n b\x0cc "), text("a b c"));
}

// Where the engine's text differs from every reference with a reason: the
// references are wrong there.
fn known_text_difference(name: &str) -> Option<&'static str> {
    match name {
        // The references read the codes of the symbolic font, whose real text
        // only the font program gives, as no-tounicode.tsv lists it.
        "crates/ozaru-engine/tests/corpus/fonts-no-tounicode.pdf" => {
            Some("the references can't read fonts without ToUnicode")
        }
        // The references put the pieces of lines that mix right-to-left and
        // left-to-right text in another order, and pypdf reads the letters of
        // right-to-left words backwards.
        "crates/ozaru-engine/tests/corpus/fonts-pango-rtl.pdf" => {
            Some("the references read lines that mix directions in another order")
        }
        _ => None,
    }
}

#[test]
fn the_text_of_every_pdf_is_like_the_references() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/text");
    let readers = ["ozaru-0.1.7", "pypdf", "pymupdf"];
    let mut compared = 0;
    let mut alike = 0;
    let mut repaired = 0;
    let mut failures = Vec::new();
    for row in rows("expected.tsv") {
        let name = &row[0];
        let file_name = format!("{}.txt", name.replace('/', "__"));
        let references: Vec<(&str, String)> = readers
            .iter()
            .filter_map(|reader| {
                let text = std::fs::read_to_string(folder.join(reader).join(&file_name)).ok()?;
                Some((*reader, text))
            })
            .collect();
        if references.is_empty() {
            continue;
        }
        let file = std::fs::read(root().join(name)).unwrap();
        let Some(mut document) = open_unlocked(name, &file) else {
            failures.push(format!("{name}: the engine can't open it"));
            continue;
        };
        let pages = document_text(&mut document);
        let text = pages
            .iter()
            .map(|page| page.text.as_str())
            .collect::<Vec<_>>()
            .join("\u{c}");
        let ours = normalized(&text);
        // A file whose text no reference finds, but the engine does, is a
        // repair, as for the page counts.
        if references.iter().all(|(_, r)| normalized(r).is_empty()) && !ours.is_empty() {
            repaired += 1;
            continue;
        }
        compared += 1;
        let best = references
            .iter()
            .map(|(reader, reference)| (similarity(&ours, &normalized(reference)), *reader))
            .fold(
                (0.0, ""),
                |best, next| if next.0 > best.0 { next } else { best },
            );
        if best.0 >= 0.98 {
            alike += 1;
        } else if known_text_difference(name).is_none() {
            failures.push(format!("{name}: {:.3} at best, with {}", best.0, best.1));
        }

        // No page that ozaru 0.1.7 reads loses its text.
        if let Some((_, published)) = references.iter().find(|(r, _)| *r == "ozaru-0.1.7") {
            for (index, page) in published.split('\u{c}').enumerate() {
                let ours = pages.get(index).map_or("", |page| page.text.as_str());
                if !page.trim().is_empty() && ours.trim().is_empty() {
                    failures.push(format!("{name}: page {} lost its text", index + 1));
                }
            }
        }
    }
    eprintln!(
        "{alike} of {compared} files with a similarity of 0.98 or more, {repaired} only the engine reads"
    );
    assert!(compared >= 38, "{compared} files");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The migration criterion: 95% of the documents.
    assert!(
        alike as f64 >= 0.95 * compared as f64,
        "{alike} of {compared}"
    );
}

// A word as words.tsv lists it, and as the engine finds it: its text and its
// box, in points from the top left corner of the visible area of the page.
#[derive(Debug, Clone, PartialEq)]
struct PlacedWord {
    text: String,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

// Where PyMuPDF finds other words than the engine, for reasons documented in
// the phase 6, 8 and 9 reviews: it reads Symbol and ZapfDingbats with the
// standard encoding, gives text to CIDs that have none, reads the fonts of
// fonts-no-tounicode.pdf otherwise, and reads page 3 of page_error.pdf,
// whose object is damaged.
fn known_word_difference(name: &str, page: usize, text: &str) -> bool {
    let file = name.rsplit('/').next().unwrap();
    match file {
        "fonts-no-tounicode.pdf" => true,
        "page_error.pdf" => page == 3,
        "content-operators.pdf" => text == "!",
        "fonts-standard-14.pdf" => !text.is_ascii() || text.contains(['!', '"']) || text == "h",
        _ => false,
    }
}

// Text that doesn't run left to right: PyMuPDF gives its glyphs boxes an em
// tall, and turns them otherwise, so only the place of the boxes is compared.
// Ghostscript turned page 5 of pdf-2.0.pdf, rotated 180 degrees, upside down.
fn turned(name: &str, page: usize, text: &str) -> bool {
    let file = name.rsplit('/').next().unwrap();
    (file == "content-operators.pdf" && ["Rotated", "by", "cm", "Tm"].contains(&text))
        || (file == "pdf-2.0.pdf" && page == 5)
}

#[test]
fn every_word_is_where_pymupdf_finds_it() {
    let mut expected: HashMap<(String, usize), Vec<PlacedWord>> = HashMap::new();
    let mut files: Vec<String> = Vec::new();
    for row in rows("words.tsv") {
        if files.last() != Some(&row[0]) {
            files.push(row[0].clone());
        }
        let number = |i: usize| row[i].parse::<f64>().unwrap();
        expected
            .entry((row[0].clone(), row[1].parse().unwrap()))
            .or_default()
            .push(PlacedWord {
                text: row[3].clone(),
                x0: number(4),
                y0: number(5),
                x1: number(6),
                y1: number(7),
            });
    }
    assert!(files.len() >= 35, "{} files", files.len());

    let (mut words, mut exact) = (0, 0);
    let mut differences = Vec::new();
    for name in &files {
        let file = std::fs::read(root().join(name)).unwrap();
        let mut document = open_unlocked(name, &file).unwrap_or_else(|| panic!("{name}"));
        let crops: Vec<Rectangle> = document.pages().iter().map(|p| p.crop_box()).collect();
        let pages = ozaru_engine::text::document_words(&mut document);
        for (index, page) in pages.iter().take(10).enumerate() {
            let number = index + 1;
            let crop = crops[index];
            let ours: Vec<PlacedWord> = page
                .lines
                .iter()
                .flatten()
                .map(|word| PlacedWord {
                    text: word.text.clone(),
                    x0: word.bbox.left - crop.left,
                    y0: crop.top - word.bbox.top,
                    x1: word.bbox.right - crop.left,
                    y1: crop.top - word.bbox.bottom,
                })
                .collect();
            let theirs = expected
                .get(&(name.clone(), number))
                .cloned()
                .unwrap_or_default();
            let mut used = vec![false; ours.len()];
            for word in &theirs {
                if known_word_difference(name, number, &word.text) {
                    continue;
                }
                let found = ours
                    .iter()
                    .enumerate()
                    .find(|(i, ours)| !used[*i] && ours.text == word.text);
                let Some((i, ours)) = found else {
                    differences.push(format!("{name} page {number}: no {:?}", word.text));
                    continue;
                };
                used[i] = true;
                words += 1;
                let near = |a: f64, b: f64| (a - b).abs() <= 0.5;
                let across = near(ours.x0, word.x0) && near(ours.x1, word.x1);
                let down = near(ours.y0, word.y0) && near(ours.y1, word.y1);
                // PyMuPDF makes the boxes of fonts that aren't embedded taller,
                // with the metrics of the fonts it uses instead: the engine's
                // are inside them.
                let inside = ours.x0 >= word.x0 - 0.5
                    && ours.x1 <= word.x1 + 0.5
                    && ours.y0 >= word.y0 - 0.5
                    && ours.y1 <= word.y1 + 0.5;
                if across && down {
                    exact += 1;
                }
                let overlap = ours.x0 < word.x1
                    && word.x0 < ours.x1
                    && ours.y0 < word.y1
                    && word.y0 < ours.y1;
                let placed = if turned(name, number, &word.text) {
                    overlap
                } else {
                    inside && (across || down)
                };
                if !placed {
                    differences.push(format!(
                        "{name} page {number}: {:?} at {:?}, PyMuPDF {:?}",
                        word.text,
                        [ours.x0, ours.y0, ours.x1, ours.y1],
                        [word.x0, word.y0, word.x1, word.y1]
                    ));
                }
            }
            for (i, ours) in ours.iter().enumerate() {
                if !used[i] && !known_word_difference(name, number, &ours.text) {
                    differences.push(format!(
                        "{name} page {number}: {:?} not in PyMuPDF",
                        ours.text
                    ));
                }
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    assert!(words >= 1200, "{words} words");
    eprintln!("{words} words, {exact} with the same box as PyMuPDF");
}

#[test]
fn the_blocks_of_every_page_hold_its_words_in_order() {
    let mut blocks = 0;
    for name in rows("expected.tsv").iter().map(|row| row[0].clone()) {
        let file = std::fs::read(root().join(&name)).unwrap();
        let Some(mut document) = open_unlocked(&name, &file) else {
            continue;
        };
        let words = ozaru_engine::text::document_words(&mut document);
        let pages = ozaru_engine::text::document_blocks(&mut document);
        for (number, (words, page)) in words.iter().zip(&pages).enumerate() {
            let in_blocks: Vec<Vec<String>> = page
                .blocks
                .iter()
                .flat_map(|block| block.words())
                .map(|line| line.into_iter().map(|word| word.text).collect())
                .collect();
            let in_lines: Vec<Vec<String>> = words
                .lines
                .iter()
                .map(|line| line.iter().map(|word| word.text.clone()).collect())
                .collect();
            assert_eq!(in_blocks, in_lines, "{name} page {}", number + 1);
            blocks += page.blocks.len();
        }
    }
    assert!(blocks > 1000, "{blocks} blocks");
}

#[test]
fn right_to_left_text_is_read_in_reading_order() {
    // The lines generate.py gives Pango, which lays them out as they are seen.
    let file = corpus("fonts-pango-rtl.pdf");
    let mut document = Document::open(&file).unwrap();
    let text = &document_text(&mut document)[0].text;
    assert_eq!(
        text,
        "Hello world\nעברית שלום\nالعربية مرحبا\nשלום עולם 2026\nprice: 100 שקל today"
    );
}

// The CRC-32 of zlib and PNG.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

// The samples of a PNG file the engine wrote, without alpha: its IDAT,
// inflated, without the filter byte of each row, which is always 0.
fn png_samples(png: &[u8]) -> Vec<u8> {
    png_samples_and_alpha(png).0
}

// The samples of a PNG file the engine wrote, and its alpha if it has one.
fn png_samples_and_alpha(png: &[u8]) -> (Vec<u8>, Option<Vec<u8>>) {
    let mut at = 8;
    let (mut width, mut channels, mut idat) = (0, 0, Vec::new());
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let data = &png[at + 8..at + 8 + length];
        match &png[at + 4..at + 8] {
            b"IHDR" => {
                width = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
                channels = match data[9] {
                    0 => 1,
                    2 => 3,
                    4 => 2,
                    _ => 4,
                };
            }
            b"IDAT" => idat.extend_from_slice(data),
            _ => {}
        }
        at += 12 + length;
    }
    let rows = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).unwrap();
    let color = if channels % 2 == 0 {
        channels - 1
    } else {
        channels
    };
    let pixels: Vec<&[u8]> = rows
        .chunks(width * channels + 1)
        .flat_map(|row| row[1..].chunks(channels))
        .collect();
    let samples = pixels
        .iter()
        .flat_map(|pixel| pixel[..color].to_vec())
        .collect();
    let alpha = (color != channels).then(|| pixels.iter().map(|pixel| pixel[color]).collect());
    (samples, alpha)
}

#[test]
fn every_image_has_the_pixels_pymupdf_gives_it() {
    let mut by_file: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for row in rows("images.tsv") {
        by_file.entry(row[0].clone()).or_default().push(row);
    }
    let (mut same, mut cmyk, mut misread) = (0, 0, 0);
    for (name, rows) in &by_file {
        let file = std::fs::read(root().join(name)).unwrap();
        let mut document = open_unlocked(name, &file).unwrap();
        let images: Vec<ozaru_engine::images::PageImage> =
            ozaru_engine::images::document_images(&mut document)
                .into_iter()
                .flat_map(|page| page.images)
                .collect();
        for row in rows {
            let number: u32 = row[2].parse().unwrap();
            let place = format!("{name} object {number}");
            let image = images
                .iter()
                .find(|image| image.id.is_some_and(|id| id.number == number))
                .unwrap_or_else(|| panic!("{place}: not found"));
            let description = image.description.as_ref().unwrap();
            assert_eq!(
                (
                    description.width.to_string(),
                    description.height.to_string()
                ),
                (row[3].clone(), row[4].clone()),
                "{place}"
            );
            let file = image
                .file
                .as_ref()
                .unwrap_or_else(|| panic!("{place}: no file"));
            let expected = u32::from_str_radix(&row[6], 16).unwrap();
            match row[5].as_str() {
                "jpeg" => assert_eq!(crc32(&file.data), expected, "{place}"),
                // PyMuPDF turns CMYK into RGB with its color management.
                "cmyk" => cmyk += 1,
                kind => {
                    let (mut samples, alpha) = png_samples_and_alpha(&file.data);
                    let object = document.resolver().resolve(image.id.unwrap()).unwrap();
                    let dictionary = object.as_stream().unwrap().dictionary().clone();
                    // PyMuPDF multiplies the colors of an image keyed by
                    // colors, /Mask as an array, by their alpha, 0 or 255.
                    let keyed = matches!(dictionary.get("Mask"), Some(Object::Array(_)));
                    if let Some(alpha) = alpha.filter(|_| keyed) {
                        let channels = samples.len() / alpha.len();
                        for (pixel, a) in samples.chunks_mut(channels).zip(alpha) {
                            if a == 0 {
                                pixel.fill(0);
                            }
                        }
                    }
                    // A stencil mask is black where it paints; PyMuPDF gives
                    // 255 there.
                    if kind == "mask" {
                        samples.iter_mut().for_each(|v| *v = 255 - *v);
                    }
                    // MuPDF misreads Group 3 lines aligned on bytes, as their
                    // twins of the same pixels in images.pdf show.
                    let aligned = dictionary
                        .get("DecodeParms")
                        .and_then(|parameters| parameters.as_dictionary())
                        .is_some_and(|parameters| parameters.contains_key("EncodedByteAlign"));
                    if aligned {
                        misread += 1;
                        continue;
                    }
                    assert_eq!(crc32(&samples), expected, "{place}: {kind}");
                }
            }
            same += 1;
        }
    }
    assert!(same >= 30, "{same} images");
    assert_eq!((cmyk, misread), (1, 3));
}

#[test]
fn fax_images_have_the_pixels_of_their_twins() {
    // images.pdf follows each fax image with the same pixels, with Flate.
    let file = corpus("images.pdf");
    let mut document = Document::open(&file).unwrap();
    let pages = ozaru_engine::images::document_images(&mut document);
    let images = &pages[0].images;
    for pair in images[..24].chunks(2) {
        let (fax, twin) = (&pair[0], &pair[1]);
        assert_eq!(
            png_samples(&fax.file.as_ref().unwrap().data),
            png_samples(&twin.file.as_ref().unwrap().data),
            "{:?}",
            fax.id
        );
    }
}
