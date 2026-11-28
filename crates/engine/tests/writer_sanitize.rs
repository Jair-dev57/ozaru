//! Sanitizing: what a document carries besides its pages is taken out.

use ozaru_engine::Document;
use ozaru_engine::document::{FieldValue, Link};
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::writer::{Editor, SanitizeOptions, SanitizeReport};

mod common;
use common::{file_with, stream_object};

// A document with something secret in every place the sanitizer knows, each
// marked with a word that says where it is. Objects: 20 XMP, 21 and 25 scripts
// and XFA, 22 an attached file, 23 an outline, 24 the structure tree, 26 a form,
// 27 a thumbnail, 28 a metadata stream, 30 a text field, 40 to 44 annotations.
fn base() -> Vec<u8> {
    file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Metadata 20 0 R /PieceInfo << /App << /Data (private-data) >> >> \
             /OpenAction << /S /JavaScript /JS (open-js) >> /AA << /WC << /S /JavaScript /JS (close-js) >> >> \
             /Names << /JavaScript << /Names [(init) 21 0 R] >> /EmbeddedFiles << /Names [(f.txt) 22 0 R] >> >> \
             /AF [22 0 R] /Outlines 23 0 R /StructTreeRoot 24 0 R /MarkInfo << /Marked true >> \
             /AcroForm << /Fields [30 0 R] /XFA [(xfa) 25 0 R] /CO [30 0 R] /DA (/Helv 0 Tf 0 g) \
             /DR << /Font << /Helv 4 0 R >> >> >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 5 0 R \
             /Resources << /Font << /F1 4 0 R >> /XObject << /Fm 26 0 R >> >> \
             /AA << /O << /S /JavaScript /JS (page-js) >> >> /Thumb 27 0 R /Metadata 28 0 R /StructParents 0 \
             /Annots [40 0 R 41 0 R 42 0 R 43 0 R 44 0 R 30 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
        (5, stream_object("", "BT /F1 14 Tf 20 250 Td (Visible text) Tj ET /Fm Do 0 0 1 rg 20 20 40 40 re f")),
        (20, stream_object("/Type /Metadata /Subtype /XML", "<x:xmpmeta>xmp-author</x:xmpmeta>")),
        (21, "<< /S /JavaScript /JS (init-js) >>".to_string()),
        (
            22,
            "<< /Type /Filespec /F (f.txt) /EF << /F 29 0 R >> >>".to_string(),
        ),
        (23, "<< /Type /Outlines /First 31 0 R /Last 31 0 R /Count 1 >>".to_string()),
        (24, "<< /Type /StructTreeRoot /K [] /Alt (struct-alt) >>".to_string()),
        (25, stream_object("", "<xfa>xfa-data</xfa>")),
        (
            26,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Metadata 28 0 R /PieceInfo << /X (form-piece) >>",
                "1 0 0 rg 150 20 30 30 re f",
            ),
        ),
        (27, stream_object("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8", "x")),
        (28, stream_object("/Type /Metadata /Subtype /XML", "<x:xmpmeta>page-xmp</x:xmpmeta>")),
        (29, stream_object("/Type /EmbeddedFile", "attached-secret")),
        (
            30,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (field-secret) /DV (default-secret) \
             /Rect [100 200 250 220] /MK << /BG [1 1 0.8] >> /AA << /K << /S /JavaScript /JS (field-js) >> >> >>"
                .to_string(),
        ),
        (31, "<< /Title (bookmark-secret) /Parent 23 0 R /Dest [3 0 R /Fit] >>".to_string()),
        (
            40,
            "<< /Type /Annot /Subtype /FileAttachment /Rect [10 100 30 120] /FS 22 0 R /AF [22 0 R] >>".to_string(),
        ),
        (41, "<< /Type /Annot /Subtype /Movie /Rect [40 100 60 120] /Movie << /F (movie-secret) >> >>".to_string()),
        (
            42,
            "<< /Type /Annot /Subtype /Text /Rect [70 100 90 120] /Contents (note-secret) /T (author-secret) >>".to_string(),
        ),
        (
            43,
            "<< /Type /Annot /Subtype /Link /Rect [100 100 150 120] /A << /S /JavaScript /JS (link-js) >> >>".to_string(),
        ),
        (
            44,
            "<< /Type /Annot /Subtype /Link /Rect [160 100 210 120] /A << /S /URI /URI (https://keep.me) >> >>".to_string(),
        ),
    ])
}

// The base document with an information dictionary.
fn with_info() -> Vec<u8> {
    let data = base();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    editor.set_info_text("Title", Some("title-secret")).unwrap();
    editor.set_info_text("Author", Some("author-name")).unwrap();
    editor
        .set_info_text("Producer", Some("producer-tool"))
        .unwrap();
    editor.save().unwrap()
}

fn sanitized(data: &[u8], options: &SanitizeOptions) -> (Vec<u8>, SanitizeReport) {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let report = editor.sanitize(options).unwrap();
    (editor.save().unwrap(), report)
}

fn has(data: &[u8], word: &str) -> bool {
    data.windows(word.len()).any(|w| w == word.as_bytes())
}

fn render(data: &[u8]) -> Vec<[u8; 4]> {
    let mut document = Document::open(data).unwrap();
    let page = document.page(0).unwrap().clone();
    let limits = document.limits();
    let pixmap = render_page(
        document.resolver(),
        &page,
        &RenderOptions::default(),
        limits,
    )
    .unwrap();
    let mut pixels = Vec::new();
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            pixels.push(pixmap.pixel(x, y).unwrap());
        }
    }
    pixels
}

#[test]
fn the_defaults_take_out_metadata_scripts_and_attachments_but_not_the_rest() {
    let data = with_info();
    let (saved, report) = sanitized(&data, &SanitizeOptions::default());
    for word in [
        "title-secret",
        "author-name",
        "producer-tool",
        "xmp-author",
        "page-xmp",
        "private-data",
        "form-piece",
        "open-js",
        "close-js",
        "init-js",
        "page-js",
        "field-js",
        "link-js",
        "xfa-data",
        "movie-secret",
        "attached-secret",
        "f.txt",
    ] {
        assert!(!has(&saved, word), "{word} is still in the file");
    }
    // What the defaults leave.
    for word in [
        "bookmark-secret",
        "note-secret",
        "field-secret",
        "struct-alt",
        "https://keep.me",
    ] {
        assert!(has(&saved, word), "{word} should stay");
    }
    assert_eq!(report.info_entries, 3);
    assert!(report.metadata_streams >= 3 && report.scripts >= 6 && report.attachments >= 3);
    assert_eq!(
        (
            report.bookmarks,
            report.comments,
            report.form_values,
            report.structure
        ),
        (0, 0, 0, 0)
    );
    // The pages look the same and the file opens.
    assert_eq!(render(&saved), render(&data));
    let mut reopened = Document::open(&saved).unwrap();
    assert_eq!(reopened.page_count(), 1);
    assert!(reopened.info().is_none_or(|info| info.title().is_none()));
    let links: Vec<_> = reopened
        .annotations(0)
        .into_iter()
        .filter_map(|a| a.link)
        .collect();
    assert_eq!(links, [Link::Uri("https://keep.me".to_string())]);
}

#[test]
fn what_is_asked_for_is_taken_out_too() {
    let data = with_info();
    let (saved, report) = sanitized(&data, &SanitizeOptions::everything());
    for word in [
        "bookmark-secret",
        "note-secret",
        "author-secret",
        "field-secret",
        "default-secret",
        "struct-alt",
    ] {
        assert!(!has(&saved, word), "{word} is still in the file");
    }
    assert_eq!(
        (
            report.bookmarks,
            report.comments,
            report.form_values,
            report.structure
        ),
        (1, 1, 1, 2)
    );
    let mut reopened = Document::open(&saved).unwrap();
    assert!(reopened.outlines().is_empty());
    let field = reopened.form_fields().remove(0);
    assert!(field.value.is_none() || field.value == Some(FieldValue::Text(String::new())));
    // The field is still there, empty, and the links that work too.
    assert_eq!(reopened.form_fields().len(), 1);
    assert!(reopened.annotations(0).iter().any(|a| a.link.is_some()));
}

#[test]
fn entries_of_the_information_dictionary_can_be_kept() {
    let data = with_info();
    let options = SanitizeOptions {
        keep_info: vec!["Title".into()],
        ..SanitizeOptions::default()
    };
    let (saved, report) = sanitized(&data, &options);
    assert!(
        has(&saved, "title-secret") && !has(&saved, "author-name") && !has(&saved, "producer-tool")
    );
    assert_eq!(report.info_entries, 2);
}

#[test]
fn each_option_only_takes_what_it_names() {
    let data = with_info();
    let only = |options: SanitizeOptions| sanitized(&data, &options).0;
    let nothing = SanitizeOptions {
        metadata: false,
        scripts: false,
        attachments: false,
        ..SanitizeOptions::default()
    };
    assert!(sanitized(&data, &nothing).1.is_empty());
    let scripts = only(SanitizeOptions {
        scripts: true,
        ..nothing.clone()
    });
    assert!(
        !has(&scripts, "open-js")
            && !has(&scripts, "movie-secret")
            && has(&scripts, "xmp-author")
            && has(&scripts, "attached-secret")
    );
    let attachments = only(SanitizeOptions {
        attachments: true,
        ..nothing.clone()
    });
    assert!(
        !has(&attachments, "attached-secret")
            && has(&attachments, "open-js")
            && has(&attachments, "title-secret")
    );
    let metadata = only(SanitizeOptions {
        metadata: true,
        ..nothing.clone()
    });
    assert!(
        !has(&metadata, "xmp-author")
            && has(&metadata, "open-js")
            && has(&metadata, "attached-secret")
    );
    let comments = only(SanitizeOptions {
        comments: true,
        ..nothing
    });
    assert!(
        !has(&comments, "note-secret")
            && has(&comments, "movie-secret")
            && has(&comments, "https://keep.me")
    );
}

#[test]
fn a_document_without_any_of_it_is_left_as_it_was() {
    let plain = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
        ),
    ]);
    let (saved, report) = sanitized(&plain, &SanitizeOptions::everything());
    assert!(report.is_empty());
    assert_eq!(Document::open(&saved).unwrap().page_count(), 1);
}

#[test]
fn a_chain_of_actions_with_a_script_at_its_end_is_taken_out() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [4 0 R] >>".to_string(),
        ),
        (
            4,
            "<< /Type /Annot /Subtype /Link /Rect [0 0 50 50] /A << /S /GoTo /D [3 0 R /Fit] \
             /Next << /S /URI /URI (https://a.b) /Next << /S /Launch /F (calc.exe) >> >> >> >>"
                .to_string(),
        ),
    ]);
    let (saved, report) = sanitized(&data, &SanitizeOptions::default());
    assert!(!has(&saved, "calc.exe"));
    assert_eq!(report.scripts, 1);
}
