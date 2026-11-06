//! Reads every indirect object, cross-reference table and compressed stream in the
//! real PDF files of the repository: the fixtures of the Python tests, the samples
//! of the documentation and the engine's own files.
//!
//! `tests/files/report-flate.pdf` is `docs/assets/report.pdf` rewritten by
//! Ghostscript 10.06 (`gs -sDEVICE=pdfwrite -dWriteXRefStm=false
//! -dWriteObjStms=false`), so it has Flate-compressed streams.
//! `tests/files/report-xrefstm.pdf` is the same with `-dWriteXRefStm=true
//! -dWriteObjStms=true`, so it has a cross-reference stream and an object stream.
//! The three encrypted ones add `-sOwnerPassword=owner`:
//! - `report-rc4-40.pdf`: `-dEncryptionR=2 -dKeyLength=40`.
//! - `report-rc4-128.pdf`: `-dEncryptionR=3 -dKeyLength=128`, with an xref stream
//!   and an object stream.
//! - `report-user-password.pdf`: `-dEncryptionR=3 -dKeyLength=128
//!   -sUserPassword=user`.

use std::path::{Path, PathBuf};

use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limits, Result};
use ozaru_engine::filters::decode_flate;
use ozaru_engine::syntax::{Object, ObjectId, parse_indirect_object};
use ozaru_engine::xref::{
    ParsedXrefTable, Resolver, XrefEntry, find_startxref, load_xref, parse_object_stream,
    parse_xref_section, rebuild_xref,
};

// Valid files, written by a PDF library. The others in the folders are damaged on
// purpose.
const VALID: &[&str] = &[
    "factura.pdf",
    "owner_password.pdf",
    "password.pdf",
    "report.pdf",
    "report-flate.pdf",
    "report-xrefstm.pdf",
    "report-rc4-40.pdf",
    "report-rc4-128.pdf",
    "report-user-password.pdf",
];

fn pdf_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<PathBuf> = ["tests", "docs/assets", "crates/ozaru-engine/tests/files"]
        .iter()
        .flat_map(|folder| std::fs::read_dir(root.join(folder)).expect("folder should exist"))
        .map(|entry| entry.expect("entry should be readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "pdf"))
        .collect();
    files.sort();
    files
}

// Offsets where an `n g obj` header starts: two integers and `obj`, after
// whitespace or at the start of the file.
fn object_headers(file: &[u8]) -> Vec<usize> {
    let mut headers = Vec::new();
    for keyword in file
        .windows(3)
        .enumerate()
        .filter(|(_, w)| *w == b"obj")
        .map(|(i, _)| i)
    {
        let mut position = keyword;
        let mut valid = true;
        for _ in 0..2 {
            let before_spaces = position;
            while position > 0 && file[position - 1].is_ascii_whitespace() {
                position -= 1;
            }
            let after_digits = position;
            while position > 0 && file[position - 1].is_ascii_digit() {
                position -= 1;
            }
            if position == after_digits || before_spaces == after_digits {
                valid = false;
                break;
            }
        }
        let at_boundary = position == 0 || file[position - 1].is_ascii_whitespace();
        if valid && at_boundary {
            headers.push(position);
        }
    }
    headers
}

#[test]
fn every_object_in_the_real_pdfs_is_read_or_reported() {
    let files = pdf_files();
    assert!(
        files.len() >= 10,
        "expected the repository PDFs, found {files:?}"
    );

    for path in files {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let valid = VALID.contains(&name.as_str());
        let headers = object_headers(&file);
        assert!(!headers.is_empty(), "{name} has no objects");

        for offset in headers {
            match parse_indirect_object(&file, offset, Limits::default()) {
                Ok(parsed) => {
                    assert!(
                        parsed.end > offset && parsed.end <= file.len(),
                        "{name} @ {offset}"
                    );
                    // An indirect `/Length` can't be resolved until phase 3 has
                    // a resolver, so valid files may report it.
                    if valid {
                        assert!(
                            parsed
                                .diagnostics
                                .iter()
                                .all(|d| d.kind() == DiagnosticKind::UnresolvedStreamLength),
                            "{name} @ {offset}: {:?}",
                            parsed.diagnostics
                        );
                    }
                    if let Object::Stream(stream) = &parsed.object.object {
                        let data = stream.raw_data(&file).expect("data should be in the file");
                        let length = stream.dictionary().get("Length");
                        if valid && length.is_some_and(|length| length.as_reference().is_none()) {
                            assert_eq!(
                                length.and_then(Object::as_integer),
                                Some(data.len() as i64),
                                "{name} @ {offset}"
                            );
                        }
                    }
                }
                Err(error) => {
                    assert!(!valid, "{name} @ {offset}: {error}");
                    assert!(error.offset() <= file.len(), "{name} @ {offset}");
                }
            }
        }
    }
}

#[test]
fn the_sample_report_has_the_objects_a_pdf_needs() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/assets/report.pdf");
    let file = std::fs::read(path).unwrap();
    let objects: Vec<_> = object_headers(&file)
        .into_iter()
        .map(|offset| {
            parse_indirect_object(&file, offset, Limits::default())
                .unwrap()
                .object
        })
        .collect();

    let types: Vec<&[u8]> = objects
        .iter()
        .filter_map(|indirect| match &indirect.object {
            Object::Dictionary(dictionary) => {
                dictionary.get("Type")?.as_name().map(|n| n.as_bytes())
            }
            _ => None,
        })
        .collect();
    let streams = objects
        .iter()
        .filter(|indirect| indirect.object.as_stream().is_some())
        .count();

    assert!(types.contains(&&b"Catalog"[..]), "{types:?}");
    assert!(types.contains(&&b"Pages"[..]), "{types:?}");
    assert_eq!(types.iter().filter(|t| **t == b"Page").count(), 3);
    assert_eq!(streams, 3, "one content stream per page");
}

// The cross-reference section that `startxref` points at, a table or a stream.
fn last_xref_section(file: &[u8]) -> Result<ParsedXrefTable> {
    let offset = find_startxref(file, Limits::default())?.offset;
    parse_xref_section(file, offset, Limits::default())
}

#[test]
fn the_xref_section_of_each_real_pdf_points_at_its_objects() {
    for path in pdf_files() {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let valid = VALID.contains(&name.as_str());

        let parsed = match last_xref_section(&file) {
            Ok(parsed) => parsed,
            Err(error) => {
                assert!(!valid, "{name}: {error}");
                assert!(error.offset() <= file.len(), "{name}");
                continue;
            }
        };
        if valid {
            assert!(
                parsed.diagnostics.is_empty(),
                "{name}: {:?}",
                parsed.diagnostics
            );
            let size = parsed
                .table
                .trailer()
                .get("Size")
                .and_then(Object::as_integer);
            assert_eq!(size, Some(parsed.table.len() as i64), "{name}");
            assert!(parsed.table.trailer().get("Root").is_some(), "{name}");
        }

        for (number, entry) in parsed.table.iter() {
            match entry {
                XrefEntry::InUse { offset, generation } => {
                    match parse_indirect_object(&file, offset, Limits::default()) {
                        Ok(object) => assert_eq!(
                            object.object.id,
                            ObjectId::new(number, generation),
                            "{name} @ {offset}"
                        ),
                        Err(error) => assert!(!valid, "{name}, object {number}: {error}"),
                    }
                }
                // Reading inside object streams comes later in phase 3. For now,
                // check that the entry names an object stream with that index.
                XrefEntry::Compressed { stream, index } => {
                    let Some(XrefEntry::InUse { offset, .. }) = parsed.table.get(stream) else {
                        panic!("{name}: object {number} is in {stream}, which isn't in use");
                    };
                    let object = parse_indirect_object(&file, offset, Limits::default()).unwrap();
                    let dictionary = object.object.object.as_stream().unwrap().dictionary();
                    let kind = dictionary.get("Type").and_then(Object::as_name).unwrap();
                    let count = dictionary.get("N").and_then(Object::as_integer).unwrap();
                    assert_eq!(kind.as_bytes(), b"ObjStm", "{name}, object {stream}");
                    assert!(i64::from(index) < count, "{name}, object {number}");
                }
                XrefEntry::Free { .. } => {}
            }
        }
    }
}

#[test]
fn every_flate_stream_in_the_real_pdfs_is_decoded() {
    let mut decoded_streams = 0;
    for path in pdf_files() {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let valid = VALID.contains(&name.as_str());
        // Encrypted files are decrypted first, which needs the resolver; another
        // test reads them.
        if Resolver::open(&file, Limits::default()).is_ok_and(|r| r.is_encrypted()) {
            continue;
        }

        for offset in object_headers(&file) {
            let Ok(parsed) = parse_indirect_object(&file, offset, Limits::default()) else {
                continue;
            };
            let Object::Stream(stream) = &parsed.object.object else {
                continue;
            };
            let filter = stream.dictionary().get("Filter").and_then(Object::as_name);
            if filter.is_none_or(|filter| filter.as_bytes() != b"FlateDecode") {
                continue;
            }
            let parameters = stream
                .dictionary()
                .get("DecodeParms")
                .and_then(Object::as_dictionary);
            let range = stream.data_range();
            let data = stream.raw_data(&file).expect("data should be in the file");

            match decode_flate(data, parameters, range.start, Limits::default()) {
                Ok(decoded) => {
                    if valid {
                        assert!(decoded.diagnostics.is_empty(), "{name} @ {offset}");
                        assert!(!decoded.data.is_empty(), "{name} @ {offset}");
                    }
                    decoded_streams += 1;
                }
                Err(error) => assert!(!valid, "{name} @ {offset}: {error}"),
            }
        }
    }
    assert!(decoded_streams >= 3, "only {decoded_streams} Flate streams");
}

#[test]
fn the_compressed_report_has_the_same_text_operators() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/report-flate.pdf");
    let file = std::fs::read(path).unwrap();

    let contents: Vec<Vec<u8>> = object_headers(&file)
        .into_iter()
        .filter_map(|offset| {
            let parsed = parse_indirect_object(&file, offset, Limits::default()).ok()?;
            let stream = parsed.object.object.as_stream()?.clone();
            let data = stream.raw_data(&file)?;
            Some(decode_flate(data, None, 0, Limits::default()).ok()?.data)
        })
        .collect();

    assert_eq!(contents.len(), 3, "one content stream per page");
    for content in contents {
        let has = |operator: &[u8]| content.windows(operator.len()).any(|w| w == operator);
        assert!(has(b"BT") && has(b"ET") && has(b"Tf"), "{content:?}");
    }
}

#[test]
fn the_report_with_an_xref_stream_has_compressed_objects() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/report-xrefstm.pdf");
    let file = std::fs::read(path).unwrap();

    let parsed = last_xref_section(&file).unwrap();
    let compressed = parsed
        .table
        .iter()
        .filter(|(_, entry)| matches!(entry, XrefEntry::Compressed { .. }))
        .count();

    assert_eq!(parsed.table.len(), 21);
    assert!(compressed > 0, "{:?}", parsed.table);
    assert!(parsed.table.trailer().get("Root").is_some());
}

#[test]
fn the_cross_reference_data_of_each_valid_pdf_is_loaded() {
    for path in pdf_files() {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let loaded = load_xref(&file, Limits::default());
        if !VALID.contains(&name.as_str()) {
            if let Err(error) = loaded {
                assert!(error.offset() <= file.len(), "{name}");
            }
            continue;
        }

        let loaded = loaded.unwrap_or_else(|error| panic!("{name}: {error}"));
        // None of these files has incremental updates.
        assert_eq!(loaded.sections, 1, "{name}");
        assert!(
            loaded.diagnostics.is_empty(),
            "{name}: {:?}",
            loaded.diagnostics
        );
        assert_eq!(
            loaded.table,
            last_xref_section(&file).unwrap().table,
            "{name}"
        );
    }
}

#[test]
fn every_compressed_object_of_the_xref_stream_report_is_read() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/report-xrefstm.pdf");
    let file = std::fs::read(path).unwrap();
    let table = load_xref(&file, Limits::default()).unwrap().table;

    let mut objects = Vec::new();
    for (number, entry) in table.iter() {
        let XrefEntry::Compressed { stream, index } = entry else {
            continue;
        };
        let Some(XrefEntry::InUse { offset, .. }) = table.get(stream) else {
            panic!("object stream {stream} isn't in use");
        };
        let container = parse_indirect_object(&file, offset, Limits::default()).unwrap();
        let container = container.object.object.as_stream().unwrap().clone();
        let parsed = parse_object_stream(&container, &file, Limits::default()).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let object = parsed.stream.object(index).unwrap();
        assert_eq!(object.object.id, ObjectId::new(number, 0));
        assert!(object.diagnostics.is_empty());
        objects.push(object.object.object);
    }

    // Ghostscript packs the resource dictionaries, the fonts, the document
    // information and the values of the indirect stream lengths.
    assert_eq!(objects.len(), 9);
    let fonts = objects
        .iter()
        .filter_map(Object::as_dictionary)
        .filter(|d| {
            d.get("Type")
                .and_then(Object::as_name)
                .map(|n| n.as_bytes())
                == Some(b"Font")
        })
        .count();
    assert_eq!(fonts, 2);
    assert!(
        objects
            .iter()
            .filter_map(Object::as_dictionary)
            .any(|d| d.get("Producer").is_some())
    );
    assert!(
        objects
            .iter()
            .any(|o| o.as_integer().is_some_and(|n| n > 0))
    );
}

#[test]
fn every_object_of_the_valid_pdfs_resolves() {
    for path in pdf_files() {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let valid = VALID.contains(&name.as_str());
        let Ok(mut resolver) = Resolver::open(&file, Limits::default()) else {
            assert!(!valid, "{name} can't be opened");
            continue;
        };
        let entries: Vec<(u32, XrefEntry)> = resolver.table().iter().collect();

        for (number, entry) in entries {
            let generation = match entry {
                XrefEntry::InUse { generation, .. } => generation,
                _ => 0,
            };
            let object = resolver.resolve(ObjectId::new(number, generation));
            if valid {
                let object = object.unwrap_or_else(|error| panic!("{name}, {number}: {error}"));
                assert_eq!(
                    *object == Object::Null,
                    matches!(entry, XrefEntry::Free { .. }),
                    "{name}, {number}"
                );
            }
        }

        if valid {
            let root = resolver
                .trailer()
                .get("Root")
                .and_then(Object::as_reference);
            let catalog = resolver
                .resolve(root.expect("trailer should have /Root"))
                .unwrap();
            let kind = catalog.as_dictionary().and_then(|d| d.get("Type"));
            assert_eq!(
                kind.and_then(Object::as_name).unwrap().as_bytes(),
                b"Catalog"
            );
            // Indirect lengths, such as Ghostscript's, are resolved too.
            assert!(
                resolver.diagnostics().is_empty(),
                "{name}: {:?}",
                resolver.diagnostics()
            );
        }
    }
}

#[test]
fn rebuilding_a_valid_pdf_finds_the_same_objects() {
    for path in pdf_files() {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !VALID.contains(&name.as_str()) {
            continue;
        }
        let loaded = load_xref(&file, Limits::default()).unwrap();
        let rebuilt = rebuild_xref(&file, Limits::default()).unwrap();
        // Repair can't decrypt, so it can't read the object streams of an
        // encrypted file: compare the objects outside of them only.
        let encrypted = loaded.table.trailer().get("Encrypt").is_some();
        let used = |entry: &(u32, XrefEntry)| match entry.1 {
            XrefEntry::Free { .. } => false,
            XrefEntry::Compressed { .. } => !encrypted,
            XrefEntry::InUse { .. } => true,
        };

        let expected: Vec<_> = loaded.table.iter().filter(used).collect();
        let found: Vec<_> = rebuilt.table.iter().filter(used).collect();
        assert_eq!(found, expected, "{name}");
        assert_eq!(
            rebuilt.table.trailer().get("Root"),
            loaded.table.trailer().get("Root"),
            "{name}"
        );
    }
}

#[test]
fn the_pdfs_with_broken_cross_reference_tables_are_repaired() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Both have broken cross-reference entries. damaged.pdf also has the header of
    // its page tree, object 8, damaged into `8 . obj`, so that one can't be found.
    for (name, has_pages) in [("tests/damaged.pdf", false), ("tests/page_panic.pdf", true)] {
        let file = std::fs::read(root.join(name)).unwrap();
        assert!(load_xref(&file, Limits::default()).is_err(), "{name}");

        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

        let kinds: Vec<_> = resolver.diagnostics().iter().map(|d| d.kind()).collect();
        assert_eq!(kinds, [DiagnosticKind::XrefRebuilt], "{name}");
        let root = resolver
            .trailer()
            .get("Root")
            .and_then(Object::as_reference)
            .unwrap();
        let catalog = resolver.resolve(root).unwrap();
        let pages = catalog.as_dictionary().unwrap().get("Pages").unwrap();
        let pages = resolver.resolve(pages.as_reference().unwrap()).unwrap();
        let count = pages
            .as_dictionary()
            .and_then(|pages| pages.get("Count"))
            .and_then(Object::as_integer);
        assert_eq!(count.is_some_and(|count| count > 0), has_pages, "{name}");
    }
}

// Encrypted files: Ghostscript rewrites of the sample report, with an owner
// password "owner" and, for report-user-password.pdf, the user password "user".

fn files_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files")
}

// The decoded content streams of every page, following the page tree.
fn page_contents(resolver: &mut Resolver<'_>) -> Vec<String> {
    let root = resolver
        .trailer()
        .get("Root")
        .and_then(Object::as_reference)
        .unwrap();
    let catalog = resolver.resolve(root).unwrap();
    let pages = catalog
        .as_dictionary()
        .unwrap()
        .get("Pages")
        .and_then(Object::as_reference)
        .unwrap();
    let pages = resolver.resolve(pages).unwrap();
    let kids: Vec<ObjectId> = pages
        .as_dictionary()
        .unwrap()
        .get("Kids")
        .and_then(Object::as_array)
        .unwrap()
        .iter()
        .filter_map(Object::as_reference)
        .collect();
    kids.into_iter()
        .map(|page| {
            let page = resolver.resolve(page).unwrap();
            let contents = page
                .as_dictionary()
                .unwrap()
                .get("Contents")
                .and_then(Object::as_reference)
                .unwrap();
            let data = resolver.stream_data(contents).unwrap();
            assert!(data.diagnostics.is_empty());
            String::from_utf8_lossy(&data.data).into_owned()
        })
        .collect()
}

#[test]
fn encrypted_files_with_an_empty_user_password_are_read() {
    for name in [
        "report-rc4-40.pdf",
        "report-rc4-128.pdf",
        "../../../../tests/owner_password.pdf",
    ] {
        let file = std::fs::read(files_dir().join(name)).unwrap();
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();

        assert!(resolver.is_encrypted(), "{name}");
        assert!(!resolver.needs_password(), "{name}");
        let contents = page_contents(&mut resolver);
        assert!(!contents.is_empty(), "{name}");
        for content in &contents {
            assert!(
                content.contains("BT") && content.contains("Tj"),
                "{name}: {content}"
            );
        }
        assert!(
            resolver.diagnostics().is_empty(),
            "{name}: {:?}",
            resolver.diagnostics()
        );
    }
}

#[test]
fn the_same_report_reads_the_same_encrypted_or_not() {
    let read = |name: &str| {
        let file = std::fs::read(files_dir().join(name)).unwrap();
        let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
        page_contents(&mut resolver)
    };

    let plain = read("report-flate.pdf");

    assert_eq!(plain.len(), 3);
    assert_eq!(read("report-rc4-40.pdf"), plain);
    assert_eq!(read("report-rc4-128.pdf"), plain);
}

#[test]
fn a_file_with_a_user_password_is_locked_until_unlocked() {
    let file = std::fs::read(files_dir().join("report-user-password.pdf")).unwrap();
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    assert!(resolver.needs_password());

    // The page tree still resolves: it has no strings or streams.
    let root = resolver
        .trailer()
        .get("Root")
        .and_then(Object::as_reference)
        .unwrap();
    let catalog = resolver.resolve(root).unwrap();
    let pages = catalog
        .as_dictionary()
        .unwrap()
        .get("Pages")
        .and_then(Object::as_reference)
        .unwrap();
    let count = resolver
        .resolve(pages)
        .unwrap()
        .as_dictionary()
        .unwrap()
        .get("Count")
        .and_then(Object::as_integer);
    assert_eq!(count, Some(3));

    // Content can't be read without the password.
    let pages = resolver.resolve(pages).unwrap();
    let first = pages
        .as_dictionary()
        .unwrap()
        .get("Kids")
        .and_then(Object::as_array)
        .unwrap()[0]
        .as_reference()
        .unwrap();
    let page = resolver.resolve(first).unwrap();
    let contents = page
        .as_dictionary()
        .unwrap()
        .get("Contents")
        .and_then(Object::as_reference)
        .unwrap();
    let error = resolver.stream_data(contents).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PasswordRequired);

    assert!(!resolver.unlock(b"wrong"));
    assert!(resolver.unlock(b"user"));
    assert!(!resolver.needs_password());
    assert_eq!(page_contents(&mut resolver).len(), 3);
}

#[test]
fn strings_are_decrypted_when_resolved() {
    let file = std::fs::read(files_dir().join("report-rc4-128.pdf")).unwrap();
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let info = resolver
        .trailer()
        .get("Info")
        .and_then(Object::as_reference)
        .unwrap();

    let info = resolver.resolve(info).unwrap();

    let producer = info
        .as_dictionary()
        .unwrap()
        .get("Producer")
        .and_then(Object::as_string)
        .unwrap();
    assert!(
        producer.as_bytes().starts_with(b"GPL Ghostscript"),
        "{:?}",
        producer
    );
}

#[test]
fn a_locked_file_keeps_its_strings_encrypted() {
    let file = std::fs::read(files_dir().join("report-user-password.pdf")).unwrap();
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let info = resolver
        .trailer()
        .get("Info")
        .and_then(Object::as_reference)
        .unwrap();

    let locked = resolver.resolve(info).unwrap();
    let locked = locked
        .as_dictionary()
        .unwrap()
        .get("Producer")
        .and_then(Object::as_string)
        .unwrap()
        .clone();
    resolver.unlock(b"user");
    let unlocked = resolver.resolve(info).unwrap();
    let unlocked = unlocked
        .as_dictionary()
        .unwrap()
        .get("Producer")
        .and_then(Object::as_string)
        .unwrap()
        .clone();

    assert!(!locked.as_bytes().starts_with(b"GPL Ghostscript"));
    assert!(unlocked.as_bytes().starts_with(b"GPL Ghostscript"));
}
