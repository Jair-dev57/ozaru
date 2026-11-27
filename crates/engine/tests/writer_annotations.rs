//! Adding annotations: what is written, what readers make of it, and how it is drawn.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::document::Date;
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::writer::{Editor, MarkupKind, NewAnnotation};

mod common;
use common::{file_with, one_page};

fn edited(data: &[u8], change: impl FnOnce(&mut Editor<'_, '_>)) -> Vec<u8> {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    change(&mut editor);
    editor.save().unwrap()
}

// The color of pixel (x, y) of the first page at scale 1, over white.
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

// Black text 100 points wide on a 612 by 792 page: a black box from (100, 700) to (200, 720).
fn page_with_a_black_box() -> Vec<u8> {
    one_page("0 g 100 700 100 20 re f", "")
}

// The quadrilateral of a box from (x0, y0) to (x1, y1), as the markup orders its corners.
fn quad(x0: f64, y0: f64, x1: f64, y1: f64) -> [f64; 8] {
    [x0, y1, x1, y1, x0, y0, x1, y0]
}

#[test]
fn a_highlight_is_read_back_with_what_it_was_given() {
    let mut note = NewAnnotation::text_markup(
        MarkupKind::Highlight,
        vec![quad(100.0, 700.0, 200.0, 720.0)],
    );
    note.contents = Some("check this: ñ".to_string());
    note.author = Some("Ana".to_string());
    note.subject = Some("review".to_string());
    note.opacity = 0.5;
    note.date = Some(Date {
        year: 2026,
        month: 11,
        day: 27,
        hour: 10,
        minute: 30,
        second: 0,
        utc_offset: Some(0),
    });
    let saved = edited(&page_with_a_black_box(), |editor| {
        editor.add_annotation(0, &note).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let found = document.annotations(0);
    assert_eq!(found.len(), 1);
    let a = &found[0];
    assert_eq!(a.subtype, "Highlight");
    assert_eq!(a.contents.as_deref(), Some("check this: ñ"));
    assert_eq!(a.author.as_deref(), Some("Ana"));
    assert_eq!(a.subject.as_deref(), Some("review"));
    assert_eq!(a.color, vec![1.0, 1.0, 0.0]);
    assert_eq!(a.quad_points, vec![quad(100.0, 700.0, 200.0, 720.0)]);
    assert_eq!(
        a.modified.as_ref().map(|d| (d.year, d.month, d.day)),
        Some((2026, 11, 27))
    );
    assert!(a.appearance.is_some());
    assert!(a.name.as_deref().is_some_and(|n| n.starts_with("ozaru-")));
    assert!(a.rect.left < 100.0 && a.rect.right > 200.0);
}

#[test]
fn a_highlight_is_yellow_and_keeps_the_text_dark() {
    let saved = edited(&one_page("0 g 100 700 40 20 re f", ""), |editor| {
        let note = NewAnnotation::text_markup(
            MarkupKind::Highlight,
            vec![quad(90.0, 695.0, 200.0, 725.0)],
        );
        editor.add_annotation(0, &note).unwrap();
    });
    // The page is 792 high: point y 710 is pixel row 82.
    assert_eq!(pixel(&saved, 150, 82), [255, 255, 0]);
    // Under the black box the band multiplies to black.
    assert_eq!(pixel(&saved, 110, 82), [0, 0, 0]);
    assert_eq!(pixel(&saved, 50, 82), [255, 255, 255]);
}

#[test]
fn underline_and_strike_out_are_lines_in_the_color() {
    for (kind, row) in [
        (MarkupKind::Underline, 792 - 700 - 1),
        (MarkupKind::StrikeOut, 792 - 710),
    ] {
        let mut note = NewAnnotation::text_markup(kind, vec![quad(100.0, 700.0, 200.0, 720.0)]);
        note.color = [0, 0, 255];
        let saved = edited(&one_page("", ""), |editor| {
            editor.add_annotation(0, &note).unwrap();
        });
        // The line is thin, so its pixels are partly covered: bluish is enough.
        let blue = |y: usize| {
            let [r, _, b] = pixel(&saved, 150, y);
            b > 200 && r < 150
        };
        assert!((row - 2..=row + 2).any(blue), "{kind:?}");
        // Not over the whole height of the text.
        assert_eq!(pixel(&saved, 150, 792 - 716), [255, 255, 255], "{kind:?}");
    }
}

#[test]
fn a_squiggly_line_is_drawn_under_the_text() {
    let note =
        NewAnnotation::text_markup(MarkupKind::Squiggly, vec![quad(100.0, 700.0, 200.0, 720.0)]);
    let saved = edited(&one_page("", ""), |editor| {
        editor.add_annotation(0, &note).unwrap();
    });
    let mut painted = 0;
    for y in (792 - 706)..(792 - 698) {
        for x in 100..200 {
            if pixel(&saved, x, y) != [255, 255, 255] {
                painted += 1;
            }
        }
    }
    assert!(painted > 100, "{painted}");
}

#[test]
fn the_annotations_a_page_had_are_kept() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots 5 0 R >>".to_string()),
        (4, "<< /Type /Annot /Subtype /Link /Rect [10 10 50 20] /A << /S /URI /URI (https://example.com) >> >>".to_string()),
        (5, "[4 0 R]".to_string()),
    ]);
    let saved = edited(&data, |editor| {
        let note =
            NewAnnotation::text_markup(MarkupKind::Highlight, vec![quad(10.0, 100.0, 60.0, 120.0)]);
        editor.add_annotation(0, &note).unwrap();
        editor.add_annotation(0, &note).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let subtypes: Vec<String> = document
        .annotations(0)
        .into_iter()
        .map(|a| a.subtype)
        .collect();
    assert_eq!(subtypes, ["Link", "Highlight", "Highlight"]);
}

#[test]
fn errors_are_named() {
    let data = one_page("", "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let note = NewAnnotation::text_markup(MarkupKind::Highlight, vec![quad(0.0, 0.0, 10.0, 10.0)]);
    assert_eq!(
        editor.add_annotation(3, &note).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    let empty = NewAnnotation::text_markup(MarkupKind::Highlight, vec![]);
    assert_eq!(
        editor.add_annotation(0, &empty).unwrap_err().kind(),
        ErrorKind::InvalidBox
    );
    let nan = NewAnnotation::text_markup(MarkupKind::Underline, vec![[f64::NAN; 8]]);
    assert_eq!(
        editor.add_annotation(0, &nan).unwrap_err().kind(),
        ErrorKind::NonFiniteNumber
    );
}

#[test]
fn an_incremental_save_keeps_the_original_bytes() {
    let data = page_with_a_black_box();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let note = NewAnnotation::text_markup(
        MarkupKind::Highlight,
        vec![quad(100.0, 700.0, 200.0, 720.0)],
    );
    editor.add_annotation(0, &note).unwrap();
    let saved = editor.save_incremental().unwrap();
    assert!(saved.starts_with(&data));
    let mut reopened = Document::open(&saved).unwrap();
    assert_eq!(reopened.annotations(0).len(), 1);
}
