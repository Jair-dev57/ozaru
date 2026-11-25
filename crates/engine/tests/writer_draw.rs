//! Drawing on pages: what the canvas writes, and what a page looks like after.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::fonts::StandardFont;
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::text::{characters, document_text};
use ozaru_engine::writer::{Canvas, Editor, LineCap, LineJoin, text_width};

mod common;
use common::{file_with, first_page, one_page, stream_object};

fn edited(data: &[u8], change: impl FnOnce(&mut Editor<'_, '_>)) -> Vec<u8> {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    change(&mut editor);
    editor.save().unwrap()
}

// The color of the pixel at (x, y) of the first page drawn at scale 1, over white.
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
    let over_white = |c: u8| (u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a))) / 255;
    [
        over_white(r) as u8,
        over_white(g) as u8,
        over_white(b) as u8,
    ]
}

fn blank() -> Vec<u8> {
    one_page("", "")
}

// A page 612 by 792 points: pixel (x, y) is the point (x, 792 - y).
fn red_square() -> Canvas {
    let mut canvas = Canvas::new();
    canvas.set_fill_color([255, 0, 0]);
    canvas.rectangle(100.0, 600.0, 100.0, 100.0).unwrap();
    canvas.fill();
    canvas
}

#[test]
fn a_filled_rectangle_is_drawn_where_it_was_asked() {
    let saved = edited(&blank(), |editor| editor.draw(0, &red_square()).unwrap());
    assert_eq!(pixel(&saved, 150, 142), [255, 0, 0]);
    assert_eq!(pixel(&saved, 50, 142), [255, 255, 255]);
    assert_eq!(pixel(&saved, 150, 300), [255, 255, 255]);
}

#[test]
fn what_the_page_had_is_kept() {
    let data = one_page("BT /F1 20 Tf 72 700 Td (Original) Tj ET", "");
    let saved = edited(&data, |editor| {
        let mut canvas = Canvas::new();
        canvas.set_fill_color([0, 0, 255]);
        canvas
            .text(72.0, 500.0, "Added", StandardFont::HelveticaBold, 24.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let text = &document_text(&mut document)[0].text;
    assert_eq!(text, "Original\nAdded");
    // The original font is still there next to the new one.
    let content = first_page(&saved);
    let fonts: Vec<_> = content
        .glyphs
        .iter()
        .filter_map(|g| g.font.base_font().map(|n| n.as_bytes().to_vec()))
        .collect();
    assert!(fonts.contains(&b"Helvetica".to_vec()));
    assert!(fonts.contains(&b"Helvetica-Bold".to_vec()));
}

#[test]
fn text_has_its_size_color_and_place() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.set_fill_color([0, 128, 0]);
        canvas
            .text(100.0, 400.0, "Hola", StandardFont::TimesRoman, 18.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let content = first_page(&saved);
    let chars = characters(&content);
    assert_eq!(
        chars.iter().map(|c| c.text.as_str()).collect::<String>(),
        "Hola"
    );
    assert_eq!(chars[0].start, (100.0, 400.0));
    assert_eq!(chars[0].size, 18.0);
    assert_eq!(chars[0].font_name().as_deref(), Some("Times-Roman"));
}

#[test]
fn latin_text_is_encoded_in_winansi() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text(
                72.0,
                700.0,
                "Año Ñandú – €5 «ü»",
                StandardFont::Helvetica,
                12.0,
            )
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document_text(&mut document)[0].text, "Año Ñandú – €5 «ü»");
}

#[test]
fn symbol_and_dingbats_have_their_own_characters() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text(72.0, 700.0, "αβγ", StandardFont::Symbol, 12.0)
            .unwrap();
        canvas
            .text(72.0, 650.0, "✓", StandardFont::ZapfDingbats, 12.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let text = &document_text(&mut document)[0].text;
    assert!(text.contains("αβγ") && text.contains('✓'), "{text:?}");
}

#[test]
fn a_character_the_font_lacks_is_an_error() {
    let mut canvas = Canvas::new();
    let error = canvas
        .text(0.0, 0.0, "日本", StandardFont::Helvetica, 12.0)
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnsupportedCharacter);
    assert!(canvas.is_empty());
    assert_eq!(
        text_width(StandardFont::Courier, 10.0, "😀")
            .unwrap_err()
            .kind(),
        ErrorKind::UnsupportedCharacter
    );
}

#[test]
fn widths_are_those_of_the_fonts_metrics() {
    // H 722, e 556, l 222, l 222, o 556 = 2278 thousandths.
    let width = text_width(StandardFont::Helvetica, 12.0, "Hello").unwrap();
    assert!((width - 27.336).abs() < 1e-9, "{width}");
    // Courier is 600 for every character.
    let width = text_width(StandardFont::Courier, 10.0, "abc").unwrap();
    assert!((width - 18.0).abs() < 1e-9);
    assert_eq!(text_width(StandardFont::Helvetica, 10.0, "").unwrap(), 0.0);
}

#[test]
fn opacity_mixes_with_the_page() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.set_opacity(0.5, 1.0).unwrap();
        canvas.set_fill_color([0, 0, 0]);
        canvas.rectangle(100.0, 600.0, 100.0, 100.0).unwrap();
        canvas.fill();
        editor.draw(0, &canvas).unwrap();
    });
    let [r, g, b] = pixel(&saved, 150, 142);
    for channel in [r, g, b] {
        assert!((120..=135).contains(&channel), "{channel}");
    }
}

#[test]
fn ellipses_curves_and_lines_are_drawn() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.set_fill_color([0, 0, 255]);
        canvas.ellipse(100.0, 500.0, 300.0, 600.0).unwrap();
        canvas.fill();
        canvas.set_stroke_color([0, 160, 0]);
        canvas.set_line_width(8.0).unwrap();
        canvas.set_line_cap(LineCap::Round);
        canvas.set_line_join(LineJoin::Round);
        canvas.set_dash(&[], 0.0).unwrap();
        canvas.move_to(100.0, 300.0).unwrap();
        canvas.line_to(300.0, 300.0).unwrap();
        canvas.stroke();
        editor.draw(0, &canvas).unwrap();
    });
    // The middle of the ellipse is blue; the corner of its box isn't.
    assert_eq!(pixel(&saved, 200, 792 - 550), [0, 0, 255]);
    assert_eq!(pixel(&saved, 105, 792 - 505), [255, 255, 255]);
    // The line is green where it passes.
    assert_eq!(pixel(&saved, 200, 792 - 300), [0, 160, 0]);
}

#[test]
fn a_dashed_line_has_gaps() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.set_stroke_color([0, 0, 0]);
        canvas.set_line_width(6.0).unwrap();
        canvas.set_dash(&[20.0, 20.0], 0.0).unwrap();
        canvas.move_to(100.0, 400.0).unwrap();
        canvas.line_to(300.0, 400.0).unwrap();
        canvas.stroke();
        editor.draw(0, &canvas).unwrap();
    });
    assert_eq!(pixel(&saved, 110, 792 - 400), [0, 0, 0]);
    assert_eq!(pixel(&saved, 130, 792 - 400), [255, 255, 255]);
}

#[test]
fn rotated_text_runs_the_other_way() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.save();
        canvas.rotate_around(300.0, 400.0, 90.0).unwrap();
        canvas
            .text(0.0, 0.0, "Up", StandardFont::Helvetica, 20.0)
            .unwrap();
        canvas.restore();
        editor.draw(0, &canvas).unwrap();
    });
    let chars = characters(&first_page(&saved));
    assert!((chars[0].direction.0).abs() < 1e-9 && (chars[0].direction.1 - 1.0).abs() < 1e-9);
    assert!((chars[0].start.0 - 300.0).abs() < 1e-6 && (chars[0].start.1 - 400.0).abs() < 1e-6);
}

#[test]
fn saved_states_come_back() {
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.save();
        canvas.set_fill_color([255, 0, 0]);
        canvas.restore();
        // Black again, the default.
        canvas.rectangle(100.0, 600.0, 100.0, 100.0).unwrap();
        canvas.fill();
        editor.draw(0, &canvas).unwrap();
    });
    assert_eq!(pixel(&saved, 150, 142), [0, 0, 0]);
}

#[test]
fn the_drawing_is_not_moved_by_what_the_page_leaves_changed() {
    // The page's content ends with a matrix and a color changed, unbalanced.
    let data = one_page("2 0 0 2 100 100 cm 1 0 0 rg", "");
    let saved = edited(&data, |editor| {
        editor.draw(0, &red_square_at_origin()).unwrap()
    });
    assert_eq!(pixel(&saved, 25, 792 - 25), [255, 0, 0]);
    assert_eq!(pixel(&saved, 150, 792 - 150), [255, 255, 255]);
}

fn red_square_at_origin() -> Canvas {
    let mut canvas = Canvas::new();
    canvas.set_fill_color([255, 0, 0]);
    canvas.rectangle(0.0, 0.0, 50.0, 50.0).unwrap();
    canvas.fill();
    canvas
}

#[test]
fn drawing_twice_on_a_page_keeps_both() {
    let saved = edited(&blank(), |editor| {
        let mut first = Canvas::new();
        first.set_opacity(0.5, 0.5).unwrap();
        first
            .text(72.0, 700.0, "one", StandardFont::Helvetica, 12.0)
            .unwrap();
        editor.draw(0, &first).unwrap();
        let mut second = Canvas::new();
        second.set_opacity(0.25, 0.25).unwrap();
        second
            .text(72.0, 650.0, "two", StandardFont::Helvetica, 12.0)
            .unwrap();
        second
            .text(72.0, 600.0, "three", StandardFont::Courier, 12.0)
            .unwrap();
        editor.draw(0, &second).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document_text(&mut document)[0].text, "one\ntwo\nthree");
}

#[test]
fn pages_that_inherit_their_resources_keep_them() {
    // The fonts are in the page tree, not in the page.
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 4 0 R >> >> >>"
                .to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>".to_string(),
        ),
        (
            4,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_string(),
        ),
        (
            5,
            stream_object("", "BT /F1 12 Tf 72 700 Td (Inherited) Tj ET"),
        ),
    ]);
    let saved = edited(&data, |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text(72.0, 600.0, "New", StandardFont::Helvetica, 12.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document_text(&mut document)[0].text, "Inherited\nNew");
}

#[test]
fn pages_with_a_list_of_streams_or_none_are_drawn_on() {
    let data = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Contents [5 0 R 6 0 R] /Resources << /Font << /F1 7 0 R >> >> >>"
                .to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        ),
        (5, stream_object("", "BT /F1 12 Tf 72 700 Td (Part ")),
        (6, stream_object("", "one) Tj ET")),
        (
            7,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
    ]);
    let saved = edited(&data, |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text(72.0, 600.0, "More", StandardFont::Helvetica, 12.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
        editor.draw(1, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let pages = document_text(&mut document);
    assert_eq!(pages[0].text, "Part one\nMore");
    assert_eq!(pages[1].text, "More");
}

#[test]
fn drawing_on_a_page_that_is_not_there_or_nothing_is_fine() {
    let data = blank();
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    assert_eq!(
        editor.draw(3, &red_square()).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
    // Nothing to draw changes nothing.
    editor.draw(0, &Canvas::new()).unwrap();
    let saved = editor.save().unwrap();
    assert_eq!(pixel(&saved, 150, 142), [255, 255, 255]);
}

#[test]
fn numbers_that_are_not_finite_are_refused() {
    let mut canvas = Canvas::new();
    for result in [
        canvas.move_to(f64::NAN, 0.0),
        canvas.line_to(0.0, f64::INFINITY),
        canvas.rectangle(0.0, 0.0, f64::NAN, 1.0),
        canvas.ellipse(0.0, 0.0, 1.0, f64::NEG_INFINITY),
        canvas.set_line_width(f64::NAN),
        canvas.set_dash(&[f64::NAN], 0.0),
        canvas.set_opacity(f64::NAN, 1.0),
        canvas.rotate_around(0.0, 0.0, f64::NAN),
        canvas.transform([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]),
        canvas.curve_to(0.0, 0.0, 0.0, 0.0, f64::NAN, 0.0),
        canvas.text(f64::NAN, 0.0, "x", StandardFont::Helvetica, 12.0),
    ] {
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NonFiniteNumber);
    }
    assert!(canvas.is_empty());
}

#[test]
fn numbers_are_written_short() {
    let mut canvas = Canvas::new();
    canvas.move_to(1.0, 2.5).unwrap();
    canvas.line_to(0.123_456, -0.000_01).unwrap();
    canvas.set_line_width(-3.0).unwrap();
    canvas.set_dash(&[3.0, 1.5], 0.25).unwrap();
    let text = String::from_utf8(canvas.content().to_vec()).unwrap();
    assert_eq!(text, "1 2.5 m\n0.1235 0 l\n0 w\n[3 1.5] 0.25 d\n");
}

#[test]
fn opacity_is_limited_to_the_range() {
    let mut canvas = Canvas::new();
    canvas.set_opacity(2.0, -1.0).unwrap();
    assert_eq!(canvas.states(), [(1.0, 0.0)]);
}

#[test]
fn incremental_saves_keep_the_original_bytes() {
    let data = one_page("BT /F1 12 Tf 72 700 Td (Signed) Tj ET", "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    editor.draw(0, &red_square()).unwrap();
    let saved = editor.save_incremental().unwrap();
    assert!(saved.starts_with(&data));
    assert_eq!(pixel(&saved, 150, 142), [255, 0, 0]);
}
