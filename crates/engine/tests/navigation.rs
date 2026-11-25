//! Bookmarks, page labels, named destinations and attached files.

use ozaru_engine::Document;

mod common;
use common::{file_with, stream_object};

// Five pages (3 to 7). Bookmarks: 9 "Chapter 1" (open) with 10 "Section 1.1"
// inside, and 11 "Chapter 2", a link; 12 points back to 9, a cycle. Page
// labels i, ii, 1, 2, then App-A. Names: `/Dests` in the catalog and in the
// name tree, one attachment.
fn pdf() -> Vec<u8> {
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string();
    file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Outlines 8 0 R /Dests 15 0 R \
             /Names << /Dests 13 0 R /EmbeddedFiles 18 0 R >> \
             /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >> 4 << /S /A /P (App-) /St 1 >>] >> >>"
                .to_string(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R 7 0 R] /Count 5 >>".to_string(),
        ),
        (3, page.clone()),
        (4, page.clone()),
        (5, page.clone()),
        (6, page.clone()),
        (7, page),
        (
            8,
            "<< /Type /Outlines /First 9 0 R /Last 11 0 R /Count 3 >>".to_string(),
        ),
        (
            9,
            "<< /Title (Chapter 1) /Parent 8 0 R /Next 11 0 R /First 10 0 R /Last 10 0 R \
             /Count 1 /Dest [3 0 R /Fit] >>"
                .to_string(),
        ),
        (
            10,
            "<< /Title <FEFF00530065006300E7> /Parent 9 0 R /Dest [4 0 R /XYZ 0 0 0] >>"
                .to_string(),
        ),
        (
            11,
            "<< /Title (Chapter 2) /Parent 8 0 R /Prev 9 0 R /Next 9 0 R \
             /A << /S /URI /URI (https://example.com/two) >> >>"
                .to_string(),
        ),
        (13, "<< /Kids [14 0 R] >>".to_string()),
        (
            14,
            "<< /Names [(alpha) [5 0 R /Fit] (beta) << /D [6 0 R /Fit] >> (old) [3 0 R /Fit]] \
             /Limits [(alpha) (old)] >>"
                .to_string(),
        ),
        (
            15,
            "<< /old [4 0 R /Fit] /gone [99 0 R /Fit] >>".to_string(),
        ),
        (18, "<< /Names [(notes.txt) 19 0 R] >>".to_string()),
        (
            19,
            "<< /Type /Filespec /F (notes.txt) /UF (notes.txt) /Desc (Some notes) \
             /EF << /F 20 0 R >> >>"
                .to_string(),
        ),
        (
            20,
            stream_object(
                "/Type /EmbeddedFile /Subtype /text#2Fplain \
                 /Params << /Size 5 /CreationDate (D:20260102030405Z) >>",
                "hello",
            ),
        ),
    ])
}

#[test]
fn bookmarks_come_in_order_with_their_levels() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let bookmarks = document.outlines();
    let summary: Vec<_> = bookmarks
        .iter()
        .map(|b| (b.title.as_str(), b.level, b.page, b.open))
        .collect();
    assert_eq!(
        summary,
        [
            ("Chapter 1", 1, Some(0), true),
            ("Secç", 2, Some(1), false),
            ("Chapter 2", 1, None, false),
        ]
    );
    assert_eq!(bookmarks[2].uri.as_deref(), Some("https://example.com/two"));
}

#[test]
fn a_document_without_an_outline_has_no_bookmarks() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
    ]);
    let mut document = Document::open(&data).unwrap();
    assert!(document.outlines().is_empty());
    assert_eq!(document.page_labels(), None);
    assert!(document.named_destinations().is_empty());
    assert!(document.attachments().is_empty());
}

#[test]
fn page_labels_follow_their_ranges() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    assert_eq!(
        document.page_labels().unwrap(),
        ["i", "ii", "1", "2", "App-A"]
    );
}

#[test]
fn named_destinations_come_from_both_places_once() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let found: Vec<_> = document
        .named_destinations()
        .into_iter()
        .map(|d| (d.name, d.page))
        .collect();
    assert_eq!(
        found,
        [
            ("old".to_string(), Some(1)),
            ("gone".to_string(), None),
            ("alpha".to_string(), Some(2)),
            ("beta".to_string(), Some(3)),
        ]
    );
}

#[test]
fn attachments_have_their_data() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let attachments = document.attachments();
    assert_eq!(attachments.len(), 1);
    let file = &attachments[0];
    assert_eq!(file.name, "notes.txt");
    assert_eq!(file.filename.as_deref(), Some("notes.txt"));
    assert_eq!(file.description.as_deref(), Some("Some notes"));
    assert_eq!(file.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(file.size, Some(5));
    assert_eq!(
        file.created.map(|d| (d.year, d.month, d.day)),
        Some((2026, 1, 2))
    );
    let stream = file.stream.unwrap();
    assert_eq!(
        document.resolver().stream_data(stream).unwrap().data,
        b"hello"
    );
}

#[test]
fn a_loop_in_the_outline_ends() {
    let data = file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R >>".to_string(),
        ),
        (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
        (3, "<< /Type /Outlines /First 4 0 R >>".to_string()),
        (
            4,
            "<< /Title (a) /Next 4 0 R /First 4 0 R /Parent 3 0 R >>".to_string(),
        ),
    ]);
    let mut document = Document::open(&data).unwrap();
    assert_eq!(document.outlines().len(), 1);
}

#[test]
fn a_page_label_tree_that_names_itself_ends() {
    let data = file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /PageLabels 3 0 R >>".to_string(),
        ),
        (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
        (3, "<< /Kids [3 0 R] >>".to_string()),
    ]);
    let mut document = Document::open(&data).unwrap();
    assert_eq!(document.page_labels(), None);
}

// `corpus/navigation.pdf` was made with pypdf; what MuPDF reads from it
// (`get_toc`, `get_label`, `resolve_names` and `embfile_get`) is here.
#[test]
fn the_corpus_file_is_read_as_mupdf_reads_it() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/navigation.pdf");
    let data = std::fs::read(path).unwrap();
    let mut document = Document::open(&data).unwrap();
    let toc: Vec<_> = document
        .outlines()
        .into_iter()
        .map(|b| (b.level, b.title, b.page.map(|page| page + 1)))
        .collect();
    let expected = [
        (1, "Chapter 1", 1),
        (2, "Section 1.1", 2),
        (2, "Section 1.2 é ü 日本", 3),
        (3, "Deep", 4),
        (1, "Chapter 2", 5),
        (1, "Chapter 3", 7),
    ];
    let expected: Vec<_> = expected
        .iter()
        .map(|&(level, title, page)| (level, title.to_string(), Some(page)))
        .collect();
    assert_eq!(toc, expected);
    assert_eq!(
        document.page_labels().unwrap(),
        ["i", "ii", "1", "2", "3", "App-A", "App-B"]
    );
    let mut names: Vec<_> = document
        .named_destinations()
        .into_iter()
        .map(|d| (d.name, d.page))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [("end".to_string(), Some(6)), ("intro".to_string(), Some(0))]
    );
    let mut files: Vec<_> = document
        .attachments()
        .into_iter()
        .map(|file| {
            let stream = file.stream.unwrap();
            let data = document.resolver().stream_data(stream).unwrap().data;
            (file.name, data)
        })
        .collect();
    files.sort();
    assert_eq!(
        files[0],
        ("data.bin".to_string(), (0..=255).collect::<Vec<u8>>())
    );
    assert_eq!(files[1], ("notes.txt".to_string(), b"hello world".to_vec()));
}
