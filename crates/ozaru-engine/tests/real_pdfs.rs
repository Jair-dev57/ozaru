//! Reads every indirect object, cross-reference table and compressed stream in the
//! real PDF files of the repository: the fixtures of the Python tests, the samples
//! of the documentation and the engine's own files.
//!
//! `tests/files/report-flate.pdf` is `docs/assets/report.pdf` rewritten by
//! Ghostscript 10.06 (`gs -sDEVICE=pdfwrite -dWriteXRefStm=false
//! -dWriteObjStms=false`), so it has Flate-compressed streams.

use std::path::{Path, PathBuf};

use ozaru_engine::base::{DiagnosticKind, Limits};
use ozaru_engine::filters::decode_flate;
use ozaru_engine::syntax::{Object, ObjectId, parse_indirect_object};
use ozaru_engine::xref::{XrefEntry, find_startxref, parse_xref_table};

// Valid files, written by a PDF library. The others in the folders are damaged on
// purpose.
const VALID: &[&str] = &[
    "factura.pdf",
    "owner_password.pdf",
    "password.pdf",
    "report.pdf",
    "report-flate.pdf",
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
        files.len() >= 9,
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

#[test]
fn the_xref_table_of_each_real_pdf_points_at_its_objects() {
    for path in pdf_files() {
        let file = std::fs::read(&path).expect("file should be readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let valid = VALID.contains(&name.as_str());

        let table = find_startxref(&file, Limits::default())
            .and_then(|found| parse_xref_table(&file, found.offset, Limits::default()));
        let parsed = match table {
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
            let XrefEntry::InUse { offset, generation } = entry else {
                continue;
            };
            let object = parse_indirect_object(&file, offset, Limits::default());
            match object {
                Ok(object) => assert_eq!(
                    object.object.id,
                    ObjectId::new(number, generation),
                    "{name} @ {offset}"
                ),
                Err(error) => assert!(!valid, "{name}, object {number}: {error}"),
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
