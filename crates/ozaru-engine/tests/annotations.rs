//! Annotations: what they say, where their links go, and how they are drawn.

use ozaru_engine::Document;
use ozaru_engine::document::Link;
use ozaru_engine::render::{RenderOptions, render_page};

mod common;
use common::{file_with, stream_object};

// Two pages. The first has annotations 5 to 11:
// 5 a URI link, 6 a link to page 2 by array, 7 a link by name (name tree),
// 8 a hidden square with an appearance, 9 a visible red square, 10 a
// checkbox widget whose state picks the form, 11 a note with text and dates.
fn pdf() -> Vec<u8> {
    file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 16 0 R >> >>".to_string(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [5 0 R 6 0 R 7 0 R \
             8 0 R 9 0 R 10 0 R 11 0 R 99 0 R (not an annotation)] >>"
                .to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
        ),
        (
            5,
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 20] \
             /A << /S /URI /URI (https://example.com/a) >> >>"
                .to_string(),
        ),
        (
            6,
            "<< /Type /Annot /Subtype /Link /Rect [10 30 50 40] \
             /A << /S /GoTo /D [4 0 R /Fit] >> >>"
                .to_string(),
        ),
        (
            7,
            "<< /Type /Annot /Subtype /Link /Rect [10 50 50 60] /Dest (chapter) >>".to_string(),
        ),
        (
            8,
            "<< /Type /Annot /Subtype /Square /Rect [0 0 20 20] /F 2 /AP << /N 12 0 R >> >>"
                .to_string(),
        ),
        (
            9,
            "<< /Type /Annot /Subtype /Square /Rect [60 60 90 90] /AP << /N 12 0 R >> >>"
                .to_string(),
        ),
        (
            10,
            "<< /Type /Annot /Subtype /Widget /Rect [0 80 20 100] /AS /On \
             /AP << /N << /On 13 0 R /Off 14 0 R >> >> >>"
                .to_string(),
        ),
        (
            11,
            "<< /Type /Annot /Subtype /Highlight /Rect [10 70 50 80] /Contents (Una nota) \
             /T (Ana) /Subj (Tema) /NM (n1) /M (D:20240131143000Z) /C [1 1 0] /F 4 \
             /QuadPoints [10 80 50 80 10 70 50 70] >>"
                .to_string(),
        ),
        // The form of the squares: red, its box twice its rectangle.
        (
            12,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 10 10]",
                "1 0 0 rg 0 0 10 10 re f",
            ),
        ),
        (
            13,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 20 20]",
                "0 0 1 rg 0 0 20 20 re f",
            ),
        ),
        (
            14,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 20 20]",
                "0 1 0 rg 0 0 20 20 re f",
            ),
        ),
        (16, "<< /Kids [17 0 R] >>".to_string()),
        (
            17,
            "<< /Names [(chapter) [4 0 R /XYZ 0 0 0]] /Limits [(chapter) (chapter)] >>".to_string(),
        ),
    ])
}

#[test]
fn annotations_are_read_with_their_fields() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let annotations = document.annotations(0);
    assert_eq!(annotations.len(), 7);
    assert_eq!(annotations[0].subtype, "Link");
    assert_eq!(
        annotations[0].link,
        Some(Link::Uri("https://example.com/a".to_string()))
    );
    assert_eq!(annotations[1].link, Some(Link::Page(1)));
    assert_eq!(annotations[2].link, Some(Link::Page(1)));
    assert!(annotations[3].is_hidden());
    assert!(!annotations[4].is_hidden());
    assert!(annotations[4].appearance.is_some());
    // The state /On picks the blue form, object 13.
    assert_eq!(annotations[5].appearance.unwrap().number, 13);
    let note = &annotations[6];
    assert_eq!(note.subtype, "Highlight");
    assert_eq!(note.contents.as_deref(), Some("Una nota"));
    assert_eq!(note.author.as_deref(), Some("Ana"));
    assert_eq!(note.subject.as_deref(), Some("Tema"));
    assert_eq!(note.name.as_deref(), Some("n1"));
    assert_eq!(note.modified.unwrap().year, 2024);
    assert_eq!(note.color, vec![1.0, 1.0, 0.0]);
    assert_eq!(note.flags, 4);
    assert_eq!(
        note.quad_points,
        vec![[10.0, 80.0, 50.0, 80.0, 10.0, 70.0, 50.0, 70.0]]
    );
    assert!(document.annotations(1).is_empty());
    assert!(document.annotations(9).is_empty());
}

#[test]
fn appearances_are_drawn_fitted_to_their_rectangles() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let limits = document.limits();
    let page = document.page(0).unwrap().clone();
    let with = render_page(
        document.resolver(),
        &page,
        &RenderOptions::default(),
        limits,
    )
    .unwrap();
    // The red square: 10 by 10 stretched to [60 60 90 90], so y 10..40 from the top.
    assert_eq!(with.pixel(75, 25), Some([255, 0, 0, 255]));
    assert_eq!(with.pixel(59, 25), Some([255, 255, 255, 255]));
    // The hidden square isn't drawn.
    assert_eq!(with.pixel(10, 90), Some([255, 255, 255, 255]));
    // The checkbox shows the form of its state.
    assert_eq!(with.pixel(10, 10), Some([0, 0, 255, 255]));
    let options = RenderOptions {
        annotations: false,
        ..RenderOptions::default()
    };
    let without = render_page(document.resolver(), &page, &options, limits).unwrap();
    assert_eq!(without.pixel(75, 25), Some([255, 255, 255, 255]));
    assert_eq!(without.pixel(10, 10), Some([255, 255, 255, 255]));
}

// The annotations of the corpus, as pypdf reads them (`annotations.tsv`).
#[test]
fn annotations_of_the_corpus_are_read_as_pypdf_reads_them() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let table = std::fs::read_to_string(corpus.join("annotations.tsv")).unwrap();
    let rows: Vec<Vec<&str>> = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .skip(1)
        .map(|line| line.split('\t').collect())
        .collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut names: Vec<&str> = rows.iter().map(|row| row[0]).collect();
    names.dedup();
    for name in names {
        let data = std::fs::read(root.join(name)).unwrap();
        let mut document = Document::open(&data).unwrap();
        let of_file: Vec<&Vec<&str>> = rows.iter().filter(|r| r[0] == name).collect();
        let pages = document.page_count();
        for page in 1..=pages {
            let expected: Vec<&&Vec<&str>> = of_file
                .iter()
                .filter(|row| row[1] == page.to_string())
                .collect();
            let found = document.annotations(page - 1);
            assert_eq!(found.len(), expected.len(), "{name} page {page}");
            for (annotation, row) in found.iter().zip(expected) {
                assert_eq!(annotation.subtype, row[2], "{row:?}");
                let rect = [
                    annotation.rect.left,
                    annotation.rect.bottom,
                    annotation.rect.right,
                    annotation.rect.top,
                ];
                for (ours, theirs) in rect.iter().zip(&row[3..7]) {
                    let theirs: f64 = theirs.parse().unwrap();
                    assert!((ours - theirs).abs() < 0.01, "{row:?}");
                }
                assert_eq!(annotation.flags.to_string(), row[7], "{row:?}");
                assert_eq!(annotation.contents.clone().unwrap_or_default(), row[8]);
                let (uri, target) = match &annotation.link {
                    Some(Link::Uri(uri)) => (uri.clone(), String::new()),
                    Some(Link::Page(page)) => (String::new(), (page + 1).to_string()),
                    _ => (String::new(), String::new()),
                };
                assert_eq!(uri, row[9], "{row:?}");
                assert_eq!(target, row.get(10).copied().unwrap_or(""), "{row:?}");
            }
        }
    }
}
