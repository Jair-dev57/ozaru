//! Adding annotations: what is written, what readers make of it, and how it is drawn.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::document::{Date, Link};
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::writer::{
    AnnotationChanges, AnnotationFilter, AnnotationShape, Editor, LinkTarget, MarkupKind,
    NewAnnotation,
};

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

fn two_pages() -> Vec<u8> {
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        ),
    ])
}

fn add(data: &[u8], page: usize, note: NewAnnotation) -> Vec<u8> {
    edited(data, |editor| {
        editor.add_annotation(page, &note).unwrap();
    })
}

#[test]
fn a_note_is_a_text_annotation_with_an_icon() {
    let mut note = NewAnnotation::new(AnnotationShape::Note {
        x: 100.0,
        y: 700.0,
        open: true,
    });
    note.color = [255, 220, 0];
    note.contents = Some("Remember this".to_string());
    let saved = add(&two_pages(), 0, note);
    let mut document = Document::open(&saved).unwrap();
    let found = document.annotations(0);
    assert_eq!(found[0].subtype, "Text");
    assert_eq!(found[0].contents.as_deref(), Some("Remember this"));
    assert_eq!((found[0].rect.left, found[0].rect.top), (100.0, 700.0));
    // The icon is drawn: yellow inside, at pixel (110, 792 - 690).
    assert_eq!(pixel(&saved, 105, 792 - 697), [255, 220, 0]);
}

#[test]
fn links_go_to_a_uri_or_a_page() {
    let data = two_pages();
    let saved = edited(&data, |editor| {
        let uri = NewAnnotation::new(AnnotationShape::Link {
            rect: [10.0, 10.0, 110.0, 30.0],
            target: LinkTarget::Uri("https://example.com/a b?x=ñ".to_string()),
            border_width: 0.0,
        });
        editor.add_annotation(0, &uri).unwrap();
        let mut page = NewAnnotation::new(AnnotationShape::Link {
            rect: [10.0, 50.0, 110.0, 70.0],
            target: LinkTarget::Page(1),
            border_width: 2.0,
        });
        page.color = [0, 0, 255];
        editor.add_annotation(0, &page).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let found = document.annotations(0);
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0].link,
        Some(Link::Uri("https://example.com/a%20b?x=%C3%B1".to_string()))
    );
    assert_eq!(found[1].link, Some(Link::Page(1)));
    // Only the one with a border draws anything.
    assert!(found[0].appearance.is_none() && found[1].appearance.is_some());
}

#[test]
fn a_link_to_a_page_that_isnt_there_is_an_error() {
    let data = two_pages();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let link = NewAnnotation::new(AnnotationShape::Link {
        rect: [0.0, 0.0, 10.0, 10.0],
        target: LinkTarget::Page(9),
        border_width: 0.0,
    });
    assert_eq!(
        editor.add_annotation(0, &link).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
}

#[test]
fn squares_circles_lines_and_ink_are_drawn_in_their_color() {
    // (shape, a pixel that must have the color, a pixel that must stay white)
    let cases: Vec<(AnnotationShape, (usize, usize), (usize, usize))> = vec![
        (
            AnnotationShape::Square {
                rect: [100.0, 600.0, 200.0, 700.0],
                fill: Some([0, 200, 0]),
                width: 3.0,
            },
            (150, 792 - 650),
            (50, 792 - 650),
        ),
        (
            AnnotationShape::Circle {
                rect: [100.0, 600.0, 200.0, 700.0],
                fill: Some([0, 200, 0]),
                width: 3.0,
            },
            (150, 792 - 650),
            (102, 792 - 602),
        ),
        (
            AnnotationShape::Line {
                from: [100.0, 650.0],
                to: [300.0, 650.0],
                width: 6.0,
                arrow: true,
            },
            (200, 792 - 650),
            (200, 792 - 600),
        ),
        (
            AnnotationShape::Ink {
                strokes: vec![vec![[100.0, 600.0], [150.0, 700.0], [200.0, 600.0]]],
                width: 6.0,
            },
            (125, 792 - 650),
            (150, 792 - 600),
        ),
    ];
    for (shape, on, off) in cases {
        let kind = format!("{shape:?}");
        let mut note = NewAnnotation::new(shape);
        note.color = [0, 200, 0];
        let saved = add(&one_page("", ""), 0, note);
        let [r, g, b] = pixel(&saved, on.0, on.1);
        assert!(g > 150 && r < 100 && b < 100, "{kind}: {r} {g} {b}");
        assert_eq!(pixel(&saved, off.0, off.1), [255, 255, 255], "{kind}");
    }
}

#[test]
fn a_line_with_an_arrow_has_the_arrow_and_its_end_style() {
    let note = NewAnnotation::new(AnnotationShape::Line {
        from: [100.0, 650.0],
        to: [300.0, 650.0],
        width: 2.0,
        arrow: true,
    });
    let saved = add(&one_page("", ""), 0, note);
    // The head makes the tip wider than the shaft: a point above the line near the end is painted.
    let red_above = (792 - 655..792 - 652).any(|y| pixel(&saved, 292, y)[1] < 100);
    assert!(red_above);
    assert!(!(792 - 660..792 - 656).any(|y| pixel(&saved, 200, y)[1] < 100));
}

#[test]
fn free_text_wraps_to_its_box() {
    let mut note = NewAnnotation::new(AnnotationShape::FreeText {
        rect: [100.0, 500.0, 160.0, 700.0],
        text: "Alpha beta gamma delta epsilon".to_string(),
        size: 12.0,
        fill: Some([255, 255, 200]),
        border_width: 1.0,
    });
    note.color = [0, 0, 0];
    let saved = add(&one_page("", ""), 0, note);
    assert_eq!(pixel(&saved, 150, 792 - 690), [255, 255, 200]);
    // Text on the first line, and more of it a line and a fifth (14.4 points) lower.
    let dark_in = |rows: std::ops::Range<usize>| {
        rows.into_iter()
            .any(|y| (102..158).any(|x| pixel(&saved, x, y)[0] < 100))
    };
    assert!(dark_in(95..105));
    assert!(dark_in(110..119));
    assert!(dark_in(124..134));
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document.annotations(0)[0].subtype, "FreeText");
}

#[test]
fn text_helvetica_cant_draw_is_an_error() {
    let data = one_page("", "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let note = NewAnnotation::new(AnnotationShape::FreeText {
        rect: [0.0, 0.0, 100.0, 100.0],
        text: "日本".to_string(),
        size: 12.0,
        fill: None,
        border_width: 0.0,
    });
    assert_eq!(
        editor.add_annotation(0, &note).unwrap_err().kind(),
        ErrorKind::UnsupportedCharacter
    );
}

#[test]
fn shapes_without_area_or_points_are_errors() {
    let data = one_page("", "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    for shape in [
        AnnotationShape::Square {
            rect: [5.0, 5.0, 5.0, 50.0],
            fill: None,
            width: 1.0,
        },
        AnnotationShape::Ink {
            strokes: vec![],
            width: 1.0,
        },
        AnnotationShape::Ink {
            strokes: vec![vec![]],
            width: 1.0,
        },
    ] {
        let error = editor
            .add_annotation(0, &NewAnnotation::new(shape))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidBox);
    }
}

fn three_marks() -> Vec<u8> {
    let mut data = two_pages();
    for (author, kind, text) in [
        ("Ana", MarkupKind::Highlight, "one"),
        ("Ana", MarkupKind::Underline, "two"),
        ("Luis", MarkupKind::Highlight, "three"),
    ] {
        let mut note = NewAnnotation::text_markup(kind, vec![quad(10.0, 10.0, 60.0, 30.0)]);
        note.author = Some(author.to_string());
        note.contents = Some(text.to_string());
        data = add(&data, 0, note);
    }
    data
}

#[test]
fn annotations_are_removed_by_subtype_author_or_text() {
    let data = three_marks();
    let removed = |filter: AnnotationFilter| {
        let mut count = 0;
        let saved = edited(&data, |editor| {
            count = editor.remove_annotations(0, &filter).unwrap();
        });
        let mut document = Document::open(&saved).unwrap();
        let left: Vec<String> = document
            .annotations(0)
            .into_iter()
            .filter_map(|a| a.contents)
            .collect();
        (count, left)
    };
    assert_eq!(
        removed(AnnotationFilter {
            subtypes: vec!["Highlight".into()],
            ..Default::default()
        }),
        (2, vec!["two".to_string()])
    );
    assert_eq!(
        removed(AnnotationFilter {
            author: Some("Ana".into()),
            ..Default::default()
        }),
        (2, vec!["three".to_string()])
    );
    assert_eq!(
        removed(AnnotationFilter {
            contents_contain: Some("hre".into()),
            ..Default::default()
        }),
        (1, vec!["one".to_string(), "two".to_string()])
    );
    assert_eq!(removed(AnnotationFilter::default()).0, 3);
    assert_eq!(
        removed(AnnotationFilter {
            author: Some("Nobody".into()),
            ..Default::default()
        })
        .0,
        0
    );
}

#[test]
fn removing_every_annotation_leaves_the_page_without_the_entry() {
    let data = three_marks();
    let saved = edited(&data, |editor| {
        editor
            .remove_annotations(0, &AnnotationFilter::default())
            .unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    assert!(document.annotations(0).is_empty());
    assert!(
        document
            .page(0)
            .unwrap()
            .dictionary()
            .get("Annots")
            .is_none()
    );
}

#[test]
fn form_fields_are_never_removed() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [4 0 R] >>".to_string(),
        ),
        (
            4,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [10 10 100 30] >>"
                .to_string(),
        ),
    ]);
    let saved = edited(&data, |editor| {
        assert_eq!(
            editor
                .remove_annotations(0, &AnnotationFilter::default())
                .unwrap(),
            0
        );
        assert_eq!(
            editor
                .remove_annotations(
                    0,
                    &AnnotationFilter {
                        subtypes: vec!["Widget".into()],
                        ..Default::default()
                    }
                )
                .unwrap(),
            0
        );
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document.annotations(0).len(), 1);
}

#[test]
fn annotations_are_updated_in_place() {
    let data = three_marks();
    let saved = edited(&data, |editor| {
        let filter = AnnotationFilter {
            author: Some("Ana".into()),
            ..Default::default()
        };
        let changes = AnnotationChanges {
            contents: Some(Some("revised ñ".to_string())),
            author: Some(Some("Bea".to_string())),
            subject: Some(Some("topic".to_string())),
            hidden: Some(true),
        };
        assert_eq!(editor.update_annotations(0, &filter, &changes).unwrap(), 2);
        // Another change removes a text and shows them again.
        let filter = AnnotationFilter {
            author: Some("Bea".into()),
            subtypes: vec!["Underline".into()],
            ..Default::default()
        };
        let changes = AnnotationChanges {
            subject: Some(None),
            hidden: Some(false),
            ..Default::default()
        };
        assert_eq!(editor.update_annotations(0, &filter, &changes).unwrap(), 1);
    });
    let mut document = Document::open(&saved).unwrap();
    let found = document.annotations(0);
    assert_eq!(found[0].contents.as_deref(), Some("revised ñ"));
    assert_eq!(found[0].author.as_deref(), Some("Bea"));
    assert!(found[0].is_hidden());
    assert_eq!(found[1].subject, None);
    assert!(!found[1].is_hidden());
    assert_eq!(found[2].author.as_deref(), Some("Luis"));
    assert_eq!(found[2].contents.as_deref(), Some("three"));
}

#[test]
fn removing_and_updating_on_pages_that_are_not_there_is_an_error() {
    let data = two_pages();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let filter = AnnotationFilter::default();
    assert_eq!(
        editor.remove_annotations(5, &filter).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    assert_eq!(
        editor
            .update_annotations(5, &filter, &AnnotationChanges::default())
            .unwrap_err()
            .kind(),
        ErrorKind::PageNotFound
    );
}
