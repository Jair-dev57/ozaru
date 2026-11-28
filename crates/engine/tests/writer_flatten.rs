//! Flattening: the drawings of widgets and annotations become the page's.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::text::document_text;
use ozaru_engine::writer::{AnnotationFilter, Editor};

mod common;
use common::{file_with, stream_object};

fn edited<T>(data: &[u8], change: impl FnOnce(&mut Editor<'_, '_>) -> T) -> (Vec<u8>, T) {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let result = change(&mut editor);
    (editor.save().unwrap(), result)
}

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 3] {
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
    let [r, g, b, a] = pixmap.pixel(x, y).unwrap();
    let over_white =
        |c: u8| ((u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a))) / 255) as u8;
    [over_white(r), over_white(g), over_white(b)]
}

fn names(data: &[u8]) -> Vec<String> {
    Document::open(data)
        .unwrap()
        .form_fields()
        .into_iter()
        .map(|f| f.name)
        .collect()
}

fn subtypes(data: &[u8]) -> Vec<String> {
    Document::open(data)
        .unwrap()
        .annotations(0)
        .into_iter()
        .map(|a| a.subtype)
        .collect()
}

// A 200 by 300 page with: 5 `name`, a text field with a value and no drawing;
// 6 `agree`, a check box that is on, whose drawing is a black square;
// 7, 8 radio buttons of `plan` (parent 9); 10 a hidden text field `secret`; 11 a
// red square annotation drawn by a form of 10 by 10 stretched over 50 by 30;
// 12 a link.
fn pdf() -> Vec<u8> {
    let widget = |rect: &str| format!("/Type /Annot /Subtype /Widget /Rect [{rect}] /P 3 0 R");
    let black = || {
        stream_object(
            "/Type /XObject /Subtype /Form /BBox [0 0 16 16]",
            "0 g 0 0 16 16 re f",
        )
    };
    file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 9 0 R 10 0 R] \
             /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 15 0 R >> >> >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R \
             /Annots [5 0 R 6 0 R 7 0 R 8 0 R 10 0 R 11 0 R 12 0 R] >>"
                .to_string(),
        ),
        (4, stream_object("", "0.9 g 0 0 5 5 re f")),
        (
            5,
            format!("<< /T (name) /FT /Tx /V (Ana Maria) {} /MK << /BG [1 1 0.8] >> >>", widget("10 250 150 270")),
        ),
        (
            6,
            format!(
                "<< /T (agree) /FT /Btn /V /Yes /AS /Yes /AP << /N << /Yes 13 0 R /Off 14 0 R >> >> {} >>",
                widget("10 200 26 216")
            ),
        ),
        (
            7,
            format!("<< /Parent 9 0 R /AS /a /AP << /N << /a 13 0 R /Off 14 0 R >> >> {} >>", widget("10 150 26 166")),
        ),
        (
            8,
            format!("<< /Parent 9 0 R /AS /Off /AP << /N << /b 13 0 R /Off 14 0 R >> >> {} >>", widget("40 150 56 166")),
        ),
        (9, "<< /T (plan) /FT /Btn /Ff 32768 /V /a /Kids [7 0 R 8 0 R] >>".to_string()),
        (
            10,
            format!("<< /T (secret) /FT /Tx /V (hidden) /F 2 {} /MK << /BG [0 0 1] >> >>", widget("100 150 190 170")),
        ),
        (
            11,
            "<< /Type /Annot /Subtype /Square /Rect [100 50 150 80] /AP << /N 16 0 R >> /F 4 >>".to_string(),
        ),
        (12, "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A << /S /URI /URI (https://a.b) >> >>".to_string()),
        (13, black()),
        (14, stream_object("/Type /XObject /Subtype /Form /BBox [0 0 16 16]", "")),
        (15, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string()),
        (16, stream_object("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "1 0 0 rg 0 0 10 10 re f")),
    ])
}

#[test]
fn flattening_the_form_draws_its_widgets_on_the_page_and_removes_the_form() {
    let data = pdf();
    assert_eq!(names(&data), ["name", "agree", "plan", "secret"]);
    let (saved, report) = edited(&data, |editor| editor.flatten_widgets(0, None).unwrap());
    // The hidden one is removed without being drawn.
    assert_eq!((report.painted, report.removed), (4, 5));
    assert_eq!(report.fields, 4);
    assert!(names(&saved).is_empty());
    assert_eq!(subtypes(&saved), ["Square", "Link"]);
    let document = Document::open(&saved).unwrap();
    assert!(document.catalog().dictionary().get("AcroForm").is_none());
    // The check box's square is on the page now: pixel (18, 300 - 208).
    assert_eq!(pixel(&saved, 18, 300 - 208), [0, 0, 0]);
    // The radio button that is on, too; the one that is off isn't.
    assert_eq!(pixel(&saved, 18, 300 - 158), [0, 0, 0]);
    assert_eq!(pixel(&saved, 48, 300 - 158), [255, 255, 255]);
    // The text field had no drawing: it got one from its value.
    let mut document = Document::open(&saved).unwrap();
    assert!(document_text(&mut document)[0].text.contains("Ana Maria"));
    assert_eq!(pixel(&saved, 100, 300 - 255), [255, 255, 204]);
    // The hidden widget isn't there.
    assert_eq!(pixel(&saved, 150, 300 - 160), [255, 255, 255]);
    assert!(!document_text(&mut document)[0].text.contains("hidden"));
}

#[test]
fn some_fields_can_be_flattened_and_the_rest_stay_in_the_form() {
    let data = pdf();
    let (saved, report) = edited(&data, |editor| {
        editor
            .flatten_widgets(0, Some(&["name".to_string(), "plan".to_string()]))
            .unwrap()
    });
    assert_eq!(report.fields, 2);
    assert_eq!(names(&saved), ["agree", "secret"]);
    assert_eq!(pixel(&saved, 18, 300 - 158), [0, 0, 0]);
    // The check box is still a widget that draws itself.
    let kinds = subtypes(&saved);
    assert_eq!(kinds.iter().filter(|k| *k == "Widget").count(), 2);
    assert_eq!(pixel(&saved, 18, 300 - 208), [0, 0, 0]);
}

#[test]
fn flattening_annotations_leaves_links_and_widgets() {
    let data = pdf();
    let (saved, report) = edited(&data, |editor| {
        editor
            .flatten_annotations(0, &AnnotationFilter::default())
            .unwrap()
    });
    assert_eq!((report.painted, report.removed), (1, 1));
    assert_eq!(
        subtypes(&saved),
        ["Widget", "Widget", "Widget", "Widget", "Widget", "Link"]
    );
    // The red form of 10 by 10 is stretched over the 50 by 30 rectangle.
    assert_eq!(pixel(&saved, 105, 300 - 55), [255, 0, 0]);
    assert_eq!(pixel(&saved, 145, 300 - 75), [255, 0, 0]);
    assert_eq!(pixel(&saved, 155, 300 - 65), [255, 255, 255]);
    // Naming the link flattens it too: it draws nothing and goes.
    let filter = AnnotationFilter {
        subtypes: vec!["Link".into()],
        ..Default::default()
    };
    let (saved, report) = edited(&data, |editor| {
        editor.flatten_annotations(0, &filter).unwrap()
    });
    assert_eq!((report.painted, report.removed), (0, 1));
    assert!(!subtypes(&saved).contains(&"Link".to_string()));
}

#[test]
fn nothing_to_flatten_changes_nothing() {
    let data = pdf();
    let (saved, report) = edited(&data, |editor| {
        editor
            .flatten_widgets(0, Some(&["no such field".to_string()]))
            .unwrap()
    });
    assert_eq!((report.painted, report.removed, report.fields), (0, 0, 0));
    assert_eq!(names(&saved).len(), 4);
}

#[test]
fn a_page_that_is_not_there_is_an_error() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    assert_eq!(
        editor.flatten_widgets(4, None).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(
        editor
            .flatten_annotations(4, &AnnotationFilter::default())
            .unwrap_err()
            .kind(),
        ErrorKind::PageNotFound
    );
}
