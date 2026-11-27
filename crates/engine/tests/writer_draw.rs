//! Drawing on pages: what the canvas writes, and what a page looks like after.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::fonts::StandardFont;
use ozaru_engine::images::{PngColor, document_images, write_png};
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::syntax::Object;
use ozaru_engine::text::{characters, document_text};
use ozaru_engine::writer::{
    Canvas, Editor, EmbeddedFont, EmbeddedImage, LineCap, LineJoin, text_width,
};
use std::sync::Arc;

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

// A font of the user: Liberation Sans, which the engine carries.
const SANS: &[u8] = include_bytes!("../src/fonts/data/standard/OzaruStdSans-Regular.ttf");

fn sans() -> Arc<EmbeddedFont> {
    Arc::new(EmbeddedFont::parse(SANS).unwrap())
}

fn dark_pixels(data: &[u8]) -> usize {
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
    let mut count = 0;
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            let [r, g, b, a] = pixmap.pixel(x, y).unwrap();
            if a > 128 && r < 100 && g < 100 && b < 100 {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn text_in_a_font_of_the_user_is_embedded_and_extracted() {
    let font = sans();
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text_embedded(72.0, 700.0, "Año Ñandú – €5", &font, 24.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document_text(&mut document)[0].text, "Año Ñandú – €5");
    // It looks like text, and is much smaller than the whole font.
    assert!(dark_pixels(&saved) > 300);
    assert!(saved.len() < SANS.len() / 2, "{} bytes", saved.len());
}

#[test]
fn the_subset_draws_the_same_as_the_whole_font() {
    let font = sans();
    // The same text in the font as the engine's own draws it for the standard
    // font Helvetica, whose substitute is this font.
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text_embedded(50.0, 400.0, "Hamburgefonstiv", &font, 40.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let standard = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text(
                50.0,
                400.0,
                "Hamburgefonstiv",
                StandardFont::Helvetica,
                40.0,
            )
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let (a, b) = (dark_pixels(&saved), dark_pixels(&standard));
    assert!(a.abs_diff(b) * 20 < a.max(b), "{a} against {b}");
}

#[test]
fn widths_come_from_the_font() {
    let font = sans();
    let helvetica = text_width(StandardFont::Helvetica, 10.0, "Hello").unwrap();
    let own = font.text_width(10.0, "Hello").unwrap();
    // Liberation Sans is metric compatible with Helvetica.
    assert!((helvetica - own).abs() < 0.05, "{helvetica} {own}");
}

#[test]
fn a_character_a_user_font_lacks_is_an_error() {
    let font = sans();
    let mut canvas = Canvas::new();
    let error = canvas
        .text_embedded(0.0, 0.0, "漢", &font, 12.0)
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnsupportedCharacter);
    assert!(font.text_width(12.0, "漢").is_err());
}

#[test]
fn two_drawings_with_different_glyphs_keep_both_subsets() {
    let font = sans();
    let saved = edited(&blank(), |editor| {
        for (y, text) in [(700.0, "first"), (650.0, "second")] {
            let mut canvas = Canvas::new();
            canvas.text_embedded(72.0, y, text, &font, 20.0).unwrap();
            editor.draw(0, &canvas).unwrap();
        }
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document_text(&mut document)[0].text, "first\nsecond");
}

#[test]
fn the_same_font_is_shared_by_the_texts_of_one_canvas() {
    let font = sans();
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text_embedded(72.0, 700.0, "abc", &font, 20.0)
            .unwrap();
        canvas
            .text_embedded(72.0, 650.0, "cde", &font, 20.0)
            .unwrap();
        assert_eq!(canvas.embedded_fonts().len(), 1);
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    assert_eq!(document_text(&mut document)[0].text, "abc\ncde");
}

#[test]
fn what_is_not_a_truetype_font_is_refused() {
    for data in [&b""[..], b"not a font", &SANS[..200]] {
        let error = EmbeddedFont::parse(data).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnsupportedFont);
    }
}

#[test]
fn a_subset_reads_as_a_font_with_the_same_glyphs() {
    use ozaru_engine::fonts::outline::TrueType;
    let font = sans();
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas
            .text_embedded(72.0, 700.0, "éA", &font, 20.0)
            .unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    // The embedded file is the only stream that begins as a font does.
    let mut document = Document::open(&saved).unwrap();
    let mut found = None;
    for number in 1..40 {
        let id = ozaru_engine::syntax::ObjectId::new(number, 0);
        if let Ok(object) = document.resolver().resolve(id)
            && let Some(dictionary) = object.as_dictionary()
            && let Some(file) = dictionary.get("FontFile2")
        {
            let file = file.clone();
            if let Object::Reference(file) = file {
                found = document.resolver().stream_data(file).ok();
            }
        }
    }
    let file = found.expect("an embedded font file").data;
    let subset = TrueType::parse(&file).unwrap();
    let whole = TrueType::parse(SANS).unwrap();
    for ch in ['A', 'é'] {
        let gid = whole.map(3, 1).unwrap().glyph(ch as u32).unwrap();
        let (a, b) = (subset.outline(gid).unwrap(), whole.outline(gid).unwrap());
        assert_eq!(a, b, "{ch}");
        assert!(!a.is_empty());
    }
    // A glyph nobody drew has no outline left.
    let other = whole.map(3, 1).unwrap().glyph('Z' as u32).unwrap();
    if usize::from(other) < subset.glyph_count() {
        assert!(subset.outline(other).is_none_or(|path| path.is_empty()));
    }
    assert!(file.len() < SANS.len() / 2);
}

// The font with the embedding bits of its license (`fsType` of the `OS/2` table) set.
fn with_license(bits: u16) -> Vec<u8> {
    let mut data = SANS.to_vec();
    let count = usize::from(u16::from_be_bytes([data[4], data[5]]));
    for i in 0..count {
        let at = 12 + 16 * i;
        if &data[at..at + 4] == b"OS/2" {
            let offset = u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap()) as usize;
            data[offset + 8..offset + 10].copy_from_slice(&bits.to_be_bytes());
            return data;
        }
    }
    panic!("the font has no OS/2 table");
}

#[test]
fn licenses_that_forbid_embedding_or_subsetting_are_respected() {
    // Installable (0), print and preview (4) and editable (8) fonts are fine.
    for bits in [0x0000, 0x0004, 0x0008] {
        assert!(
            EmbeddedFont::parse(&with_license(bits)).is_ok(),
            "{bits:#x}"
        );
    }
    // Restricted (2), and those that may not be subset (0x100).
    for bits in [0x0002, 0x0100, 0x0104] {
        let error = EmbeddedFont::parse(&with_license(bits)).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::FontNotEmbeddable, "{bits:#x}");
    }
}

// A 2 by 2 picture: red, green / blue, white.
fn quarters() -> Arc<EmbeddedImage> {
    let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let png = write_png(2, 2, PngColor::Rgb, &pixels);
    Arc::new(EmbeddedImage::parse(&png).unwrap())
}

#[test]
fn a_png_is_drawn_stretched_to_the_rectangle() {
    let image = quarters();
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        // The rectangle from (100, 500) to (300, 700): pixel y = 792 - point y.
        canvas.image(100.0, 500.0, 200.0, 200.0, &image).unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    assert_eq!(pixel(&saved, 150, 142), [255, 0, 0]);
    assert_eq!(pixel(&saved, 250, 142), [0, 255, 0]);
    assert_eq!(pixel(&saved, 150, 242), [0, 0, 255]);
    assert_eq!(pixel(&saved, 250, 242), [255, 255, 255]);
    assert_eq!(pixel(&saved, 50, 142), [255, 255, 255]);
}

#[test]
fn the_transparency_of_a_png_shows_the_page() {
    // Red, half see-through on the left; solid blue on the right.
    let pixels = [255, 0, 0, 0, 0, 0, 255, 255];
    let png = write_png(2, 1, PngColor::Rgba, &pixels);
    let image = Arc::new(EmbeddedImage::parse(&png).unwrap());
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.image(100.0, 500.0, 200.0, 100.0, &image).unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    // Fully see-through (alpha 0) shows the white page.
    assert_eq!(pixel(&saved, 150, 242), [255, 255, 255]);
    assert_eq!(pixel(&saved, 250, 242), [0, 0, 255]);
}

#[test]
fn an_image_is_embedded_once_however_many_times_it_is_drawn() {
    let image = quarters();
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.image(10.0, 10.0, 50.0, 50.0, &image).unwrap();
        canvas.image(100.0, 10.0, 50.0, 50.0, &image).unwrap();
        assert_eq!(canvas.images().len(), 1);
        editor.draw(0, &canvas).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let images = document_images(&mut document);
    assert_eq!(images[0].images.len(), 2);
    assert_eq!(images[0].images[0].id, images[0].images[1].id);
    let description = images[0].images[0].description.as_ref().unwrap();
    assert_eq!((description.width, description.height), (2, 2));
}

#[test]
fn a_jpeg_goes_in_as_it_is() {
    let jpeg = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/files/jpeg/rgb-420.jpg"
    ))
    .unwrap();
    let image = Arc::new(EmbeddedImage::parse(&jpeg).unwrap());
    assert!(image.width() > 0 && image.height() > 0);
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.image(100.0, 400.0, 200.0, 200.0, &image).unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    // The file's bytes are in the PDF, not a recompression.
    assert!(saved.windows(jpeg.len()).any(|window| window == &jpeg[..]));
    let mut document = Document::open(&saved).unwrap();
    let images = document_images(&mut document);
    let description = images[0].images[0].description.as_ref().unwrap();
    assert_eq!(
        (description.width, description.height),
        (image.width(), image.height())
    );
    assert_ne!(pixel(&saved, 200, 292), [255, 255, 255]);
}

#[test]
fn jpeg_files_of_every_kind_are_read() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/jpeg");
    let mut count = 0;
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "jpg") {
            let image = EmbeddedImage::parse(&std::fs::read(&path).unwrap());
            assert!(image.is_ok(), "{}", path.display());
            count += 1;
        }
    }
    assert!(count >= 15);
}

#[test]
fn pictures_that_are_not_jpeg_or_png_are_refused() {
    for data in [&b""[..], b"GIF89a....", b"hello", &[0xFF, 0xD8, 0xFF]] {
        assert!(EmbeddedImage::parse(data).is_err());
    }
    let error = EmbeddedImage::parse(b"GIF89a").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnsupportedImage);
}

#[test]
fn an_image_keeps_the_opacity_set_before_it() {
    let image = quarters();
    let saved = edited(&blank(), |editor| {
        let mut canvas = Canvas::new();
        canvas.set_opacity(0.5, 0.5).unwrap();
        canvas.image(100.0, 500.0, 200.0, 200.0, &image).unwrap();
        editor.draw(0, &canvas).unwrap();
    });
    let [r, g, b] = pixel(&saved, 150, 142);
    assert_eq!(r, 255);
    assert!(
        (120..135).contains(&g) && (120..135).contains(&b),
        "{g} {b}"
    );
}
