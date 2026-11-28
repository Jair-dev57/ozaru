//! Redaction: what is under an area is gone from the file, and the rest stays
//! where it was.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::document::Rectangle;
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::text::document_text;
use ozaru_engine::writer::{
    Editor, GraphicsMode, ImageMode, RedactArea, RedactOptions, RedactReport,
};

mod common;
use common::{file_with, first_page, one_page, stream_object};

fn redacted(
    data: &[u8],
    page: usize,
    areas: &[RedactArea],
    options: &RedactOptions,
) -> (Vec<u8>, RedactReport) {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let report = editor.redact(page, areas, options).unwrap();
    (editor.save().unwrap(), report)
}

fn area(left: f64, bottom: f64, right: f64, top: f64) -> RedactArea {
    RedactArea::black(Rectangle::new(left, bottom, right, top))
}

fn text_of(data: &[u8]) -> String {
    let mut document = Document::open(data).unwrap();
    document_text(&mut document)
        .iter()
        .map(|page| page.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

// The glyphs of the first page as (character, x of its origin).
fn glyphs(data: &[u8]) -> Vec<(String, f64)> {
    first_page(data)
        .glyphs
        .iter()
        .map(|g| (g.text.as_deref().unwrap_or("?").to_string(), g.origin().0))
        .collect()
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

// "Hello secret world" in 12 point Helvetica from (100, 700): "secret" is
// from x = 100 + width("Hello ") on.
const LINE: &str = "BT /F1 12 Tf 100 700 Td (Hello secret world) Tj ET";

fn words_area() -> RedactArea {
    // "Hello " is 6 characters of about 5.3 points: "secret" is about 132 to 165.
    area(135.0, 695.0, 163.0, 715.0)
}

#[test]
fn the_text_under_an_area_is_taken_out_and_the_rest_keeps_its_place() {
    let data = one_page(LINE, "");
    let before = glyphs(&data);
    let (saved, report) = redacted(&data, 0, &[words_area()], &RedactOptions::default());
    assert!(report.glyphs >= 5, "{report:?}");
    let text = text_of(&saved);
    assert!(!text.contains("secret") && !text.contains("sec"), "{text}");
    assert!(text.contains("Hello") && text.contains("world"), "{text}");
    // "world" is where it was.
    let after = glyphs(&saved);
    let place =
        |glyphs: &[(String, f64)]| glyphs.iter().rev().take(5).map(|g| g.1).collect::<Vec<_>>();
    for (a, b) in place(&before).iter().zip(place(&after)) {
        assert!((a - b).abs() < 0.05, "{a} {b}");
    }
}

#[test]
fn the_text_is_not_in_any_stream_of_the_file() {
    let data = one_page(LINE, "");
    let (saved, _) = redacted(&data, 0, &[words_area()], &RedactOptions::default());
    // Not as written, and not in the decoded content either.
    assert!(!saved.windows(6).any(|w| w == b"secret"));
    let mut document = Document::open(&saved).unwrap();
    let mut found = false;
    for number in 1..40 {
        let id = ozaru_engine::syntax::ObjectId::new(number, 0);
        if let Ok(decoded) = document.resolver().stream_data(id)
            && (decoded.data.windows(2).any(|w| w == b"se")
                || decoded.data.windows(3).any(|w| w == b"ecr"))
        {
            found = true;
        }
    }
    assert!(!found, "the old content is still in the file");
}

#[test]
fn spacing_and_adjustments_keep_the_text_in_place() {
    // Character and word spacing, a TJ with numbers, and the quote operators.
    let content = "BT /F2 10 Tf 2 Tc 5 Tw 50 700 Td (AAA BBB CCC) Tj ET \
                   BT /F1 12 Tf 50 650 Td [(Hel) -300 (lo X) 120 (YZ)] TJ ET \
                   BT /F1 12 Tf 14 TL 50 600 Td (first) Tj (second HIDDEN) ' 3 1 (third HIDDEN) \" ET";
    let data = one_page(content, "");
    let before = glyphs(&data);
    let hidden = |glyphs: &[(String, f64)]| glyphs.iter().map(|g| g.0.clone()).collect::<String>();
    assert!(hidden(&before).contains("HIDDEN"));
    // Everything at the right of x = 200 on the last two lines, and "BBB" and "X" and "Z".
    let areas = [
        area(77.0, 690.0, 87.0, 712.0), // somewhere in the Courier line: the second B group
        area(80.0, 560.0, 400.0, 596.0), // HIDDEN of the quote lines
    ];
    let (saved, report) = redacted(&data, 0, &areas, &RedactOptions::default());
    assert!(report.glyphs > 0);
    let after = glyphs(&saved);
    let text: String = after.iter().map(|g| g.0.clone()).collect();
    assert!(!text.contains("HIDDEN"), "{text}");
    // Every glyph that is left is exactly where it was.
    for (name, x) in &after {
        let found = before
            .iter()
            .any(|(n, bx)| n == name && (bx - x).abs() < 0.05);
        assert!(found, "{name} moved to {x}");
    }
    assert!(text.contains("first"), "{text}");
}

#[test]
fn nothing_under_the_areas_changes_nothing() {
    let data = one_page(LINE, "");
    let (saved, report) = redacted(
        &data,
        0,
        &[RedactArea {
            rect: Rectangle::new(300.0, 100.0, 400.0, 200.0),
            fill: None,
        }],
        &RedactOptions::default(),
    );
    assert!(report.is_empty());
    assert_eq!(glyphs(&saved), glyphs(&data));
}

#[test]
fn the_box_is_drawn_over_the_area() {
    let data = one_page(LINE, "");
    let (saved, _) = redacted(&data, 0, &[words_area()], &RedactOptions::default());
    // Pixel (150, 792 - 705) is inside the box.
    assert_eq!(pixel(&saved, 150, 792 - 705), [0, 0, 0]);
    let red = RedactArea {
        rect: Rectangle::new(135.0, 695.0, 163.0, 715.0),
        fill: Some([200, 0, 0]),
    };
    let (saved, _) = redacted(&data, 0, &[red], &RedactOptions::default());
    assert_eq!(pixel(&saved, 150, 792 - 705), [200, 0, 0]);
}

fn image_page() -> Vec<u8> {
    // A 10 by 10 gray image, 200 everywhere but a darker column, painted over
    // the square from (100, 100) to (200, 200).
    let row = "C8".repeat(10);
    let hex = std::iter::repeat_n(row, 10).collect::<String>();
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /XObject << /Im 5 0 R >> >> /Contents 4 0 R >>"
                .to_string(),
        ),
        (4, stream_object("", "q 100 0 0 100 100 100 cm /Im Do Q")),
        (
            5,
            stream_object(
                "/Type /XObject /Subtype /Image /Width 10 /Height 10 /ColorSpace /DeviceGray \
                 /BitsPerComponent 8 /Filter /ASCIIHexDecode",
                &format!("{hex}>"),
            ),
        ),
    ])
}

#[test]
fn the_pixels_under_an_area_are_blanked() {
    let data = image_page();
    assert_eq!(pixel(&data, 120, 792 - 150), [200, 200, 200]);
    let no_box = RedactArea {
        rect: Rectangle::new(150.0, 100.0, 200.0, 200.0),
        fill: None,
    };
    let (saved, report) = redacted(&data, 0, &[no_box], &RedactOptions::default());
    assert_eq!((report.images_blanked, report.images_removed), (1, 0));
    // The left half is as it was, the right half black.
    assert_eq!(pixel(&saved, 120, 792 - 150), [200, 200, 200]);
    assert_eq!(pixel(&saved, 180, 792 - 150), [0, 0, 0]);
    // The picture is still there, its size too.
    assert_eq!(pixel(&saved, 120, 792 - 120), [200, 200, 200]);
}

#[test]
fn an_image_can_be_removed_whole_or_kept() {
    let data = image_page();
    let no_box = RedactArea {
        rect: Rectangle::new(150.0, 100.0, 200.0, 200.0),
        fill: None,
    };
    let remove = RedactOptions {
        images: ImageMode::Remove,
        ..RedactOptions::default()
    };
    let (saved, report) = redacted(&data, 0, &[no_box], &remove);
    assert_eq!(report.images_removed, 1);
    assert_eq!(pixel(&saved, 120, 792 - 150), [255, 255, 255]);
    let keep = RedactOptions {
        images: ImageMode::Keep,
        ..RedactOptions::default()
    };
    let (saved, report) = redacted(&data, 0, &[no_box], &keep);
    assert!(report.is_empty());
    assert_eq!(pixel(&saved, 180, 792 - 150), [200, 200, 200]);
}

#[test]
fn an_image_the_area_misses_is_left_alone() {
    let data = image_page();
    let elsewhere = RedactArea {
        rect: Rectangle::new(300.0, 300.0, 400.0, 400.0),
        fill: None,
    };
    let (saved, report) = redacted(&data, 0, &[elsewhere], &RedactOptions::default());
    assert!(report.is_empty());
    assert_eq!(pixel(&saved, 120, 792 - 150), [200, 200, 200]);
}

#[test]
fn shapes_are_removed_when_covered_or_touched() {
    // A blue square from (100, 600) to (150, 650), and a big yellow one.
    let content = "0 0 1 rg 100 600 50 50 re f 1 1 0 rg 300 300 200 200 re f";
    let data = one_page(content, "");
    let no_box = |l, b, r, t| RedactArea {
        rect: Rectangle::new(l, b, r, t),
        fill: None,
    };
    let covering = no_box(90.0, 590.0, 160.0, 660.0);
    let (saved, report) = redacted(&data, 0, &[covering], &RedactOptions::default());
    assert_eq!(report.paths, 1);
    assert_eq!(pixel(&saved, 120, 792 - 620), [255, 255, 255]);
    assert_eq!(pixel(&saved, 400, 792 - 400), [255, 255, 0]);
    // A smaller area only touches the shape: kept by default, removed when asked.
    let touching = no_box(120.0, 620.0, 130.0, 630.0);
    let (saved, report) = redacted(&data, 0, &[touching], &RedactOptions::default());
    assert_eq!(report.paths, 0);
    assert_eq!(pixel(&saved, 140, 792 - 640), [0, 0, 255]);
    let touched = RedactOptions {
        graphics: GraphicsMode::Touched,
        ..RedactOptions::default()
    };
    let (saved, report) = redacted(&data, 0, &[touching], &touched);
    assert_eq!(report.paths, 1);
    assert_eq!(pixel(&saved, 140, 792 - 640), [255, 255, 255]);
    let keep = RedactOptions {
        graphics: GraphicsMode::Keep,
        ..RedactOptions::default()
    };
    let (_, report) = redacted(&data, 0, &[covering], &keep);
    assert_eq!(report.paths, 0);
}

// Two pages that paint the same form, which has text in it.
fn pages_with_a_form() -> Vec<u8> {
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Fm 5 0 R >> >> /Contents 6 0 R >>".to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Fm 5 0 R >> >> /Contents 6 0 R >>".to_string(),
        ),
        (
            5,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 300 100] /Matrix [1 0 0 1 0 0] \
                 /Resources << /Font << /F1 7 0 R >> >>",
                "BT /F1 12 Tf 10 50 Td (Account 12345 in the form) Tj ET",
            ),
        ),
        (6, stream_object("", "q 1 0 0 1 100 600 cm /Fm Do Q")),
        (7, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
    ])
}

#[test]
fn text_in_a_form_is_taken_out_of_a_copy_of_the_form() {
    let data = pages_with_a_form();
    // The number is around x = 100 + 10 + 55 = 165 to 195, y = 650.
    let (saved, report) = redacted(
        &data,
        0,
        &[area(168.0, 645.0, 192.0, 665.0)],
        &RedactOptions::default(),
    );
    assert_eq!(report.forms, 1);
    assert!(report.glyphs >= 4);
    let mut document = Document::open(&saved).unwrap();
    let pages = document_text(&mut document);
    assert!(
        !pages[0].text.contains("12345") && !pages[0].text.contains("1234"),
        "{}",
        pages[0].text
    );
    assert!(pages[0].text.contains("Account") && pages[0].text.contains("in the form"));
    // The other page paints the original form, which still has it.
    assert!(pages[1].text.contains("12345"), "{}", pages[1].text);
}

#[test]
fn nested_forms_are_rewritten_too() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Outer 5 0 R /Inner 6 0 R >> >> /Contents 4 0 R >>".to_string()),
        (4, stream_object("", "/Outer Do")),
        // The outer form has no resources of its own: it shares the page's.
        (5, stream_object("/Type /XObject /Subtype /Form /BBox [0 0 612 792]", "q 1 0 0 1 100 600 cm /Inner Do Q")),
        (6, stream_object("/Type /XObject /Subtype /Form /BBox [0 0 300 100] /Resources << /Font << /F1 7 0 R >> >>", "BT /F1 12 Tf 10 50 Td (Secret inside) Tj ET")),
        (7, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
    ]);
    let (saved, report) = redacted(
        &data,
        0,
        &[area(100.0, 640.0, 300.0, 670.0)],
        &RedactOptions::default(),
    );
    assert!(report.forms >= 1 && report.glyphs > 0, "{report:?}");
    assert!(!text_of(&saved).contains("Secret"), "{}", text_of(&saved));
}

#[test]
fn an_actual_text_that_spells_what_went_is_removed() {
    let content = "/Span << /ActualText (secret) /MCID 0 >> BDC BT /F1 12 Tf 100 700 Td (secret) Tj ET EMC \
                   /Span << /ActualText (keep me) >> BDC BT /F1 12 Tf 100 650 Td (other) Tj ET EMC";
    let data = one_page(content, "");
    let (saved, _) = redacted(
        &data,
        0,
        &[area(95.0, 690.0, 200.0, 715.0)],
        &RedactOptions::default(),
    );
    let page = first_page(&saved);
    let texts: Vec<_> = page
        .marked_content
        .iter()
        .filter_map(|m| m.actual_text.clone())
        .collect();
    assert_eq!(texts, ["keep me"]);
    // The sequence itself stays, only the text that spelled the glyphs goes.
    assert!(page.marked_content.iter().any(|m| m.mcid == Some(0)));
}

#[test]
fn annotations_under_an_area_are_removed_but_form_fields_stay() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [4 0 R 5 0 R 6 0 R] >>".to_string()),
        (4, "<< /Type /Annot /Subtype /Text /Rect [100 700 120 720] /Contents (secret note) >>".to_string()),
        (5, "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (secret value) /Rect [100 650 200 670] >>".to_string()),
        (6, "<< /Type /Annot /Subtype /Link /Rect [400 100 450 120] >>".to_string()),
    ]);
    let (saved, report) = redacted(
        &data,
        0,
        &[area(90.0, 640.0, 300.0, 730.0)],
        &RedactOptions::default(),
    );
    assert_eq!((report.annotations, report.widgets_left), (1, 1));
    let mut document = Document::open(&saved).unwrap();
    let subtypes: Vec<String> = document
        .annotations(0)
        .into_iter()
        .map(|a| a.subtype)
        .collect();
    assert_eq!(subtypes, ["Widget", "Link"]);
    let off = RedactOptions {
        annotations: false,
        ..RedactOptions::default()
    };
    let (saved, report) = redacted(&data, 0, &[area(90.0, 640.0, 300.0, 730.0)], &off);
    assert_eq!(report.annotations, 0);
    assert_eq!(Document::open(&saved).unwrap().annotations(0).len(), 3);
}

#[test]
fn inline_images_and_clips_are_handled() {
    // An inline 2 by 2 gray image under the area, and a clip that stays.
    let content = "q 100 100 50 50 re W n 1 g 90 90 80 80 re f Q \
                   q 40 0 0 40 300 300 cm BI /W 2 /H 2 /CS /G /BPC 8 /F /AHx ID 00 40 80 FF > EI Q";
    let data = one_page(content, "");
    let (saved, report) = redacted(
        &data,
        0,
        &[RedactArea {
            rect: Rectangle::new(290.0, 290.0, 350.0, 350.0),
            fill: None,
        }],
        &RedactOptions::default(),
    );
    assert_eq!(report.images_blanked, 1);
    assert_eq!(pixel(&saved, 320, 792 - 320), [0, 0, 0]);
}

#[test]
fn a_page_that_is_not_there_is_an_error_and_no_areas_is_fine() {
    let data = one_page(LINE, "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let options = RedactOptions::default();
    assert_eq!(
        editor
            .redact(5, &[words_area()], &options)
            .unwrap_err()
            .kind(),
        ErrorKind::PageNotFound
    );
    assert!(editor.redact(0, &[], &options).unwrap().is_empty());
    let nan = RedactArea::black(Rectangle {
        left: f64::NAN,
        bottom: 0.0,
        right: 1.0,
        top: 1.0,
    });
    assert_eq!(
        editor.redact(0, &[nan], &options).unwrap_err().kind(),
        ErrorKind::NonFiniteNumber
    );
}
