//! Reads every indirect object in the real PDF files of the repository: the
//! fixtures of the Python tests and the samples of the documentation.

use std::path::{Path, PathBuf};

use ozaru_engine::base::Limits;
use ozaru_engine::syntax::{Object, parse_indirect_object};

// Valid files, written by a PDF library. The others in the folders are damaged on
// purpose.
const VALID: &[&str] = &[
    "factura.pdf",
    "owner_password.pdf",
    "password.pdf",
    "report.pdf",
];

fn pdf_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<PathBuf> = ["tests", "docs/assets"]
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
        files.len() >= 8,
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
                    if valid {
                        assert!(
                            parsed.diagnostics.is_empty(),
                            "{name} @ {offset}: {:?}",
                            parsed.diagnostics
                        );
                    }
                    if let Object::Stream(stream) = &parsed.object.object {
                        let data = stream.raw_data(&file).expect("data should be in the file");
                        if valid {
                            let length = stream
                                .dictionary()
                                .get("Length")
                                .and_then(Object::as_integer);
                            assert_eq!(length, Some(data.len() as i64), "{name} @ {offset}");
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
