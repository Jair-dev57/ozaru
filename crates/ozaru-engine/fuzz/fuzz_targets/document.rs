#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::{Diagnostics, Limits};
use ozaru_engine::content::Interpreter;
use ozaru_engine::images::{ImageFormat, image_file, read_image};
use ozaru_engine::text::{page_blocks, page_text};
use ozaru_engine::fonts::{read_composite_font, read_simple_font};
use ozaru_engine::syntax::Object;
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a document, locked and then with the password "user", and
// reads every page, the fonts and the content of the first pages and the
// information dictionary.
// Each is a result or an error, and every error and diagnostic is inside the
// input.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        max_pages: 10_000,
        max_content_operations: 100_000,
        ..Limits::default()
    };
    let options = OpenOptions {
        limits,
        password: None,
    };
    let mut document = match Document::open_with(data, options) {
        Ok(document) => document,
        Err(error) => {
            assert!(error.offset() <= data.len());
            return;
        }
    };
    check(&document, data.len());
    fonts(&mut document, limits);
    contents(&mut document, limits, data.len());
    if document.unlock(b"user") {
        check(&document, data.len());
        fonts(&mut document, limits);
        contents(&mut document, limits, data.len());
    }
});

// Reads the fonts of the first ten pages, and the text and width of every code
// of each simple font and of some codes of each composite font.
fn fonts(document: &mut Document<'_>, limits: Limits) {
    let mut fonts = Vec::new();
    for page in document.pages().iter().take(10) {
        if let Some(Object::Dictionary(dictionary)) = page.resources().category("Font") {
            fonts.extend(dictionary.iter().map(|(_, font)| font.clone()));
        }
    }
    let resolver = document.resolver();
    for font in fonts.iter().take(50) {
        let font = match font {
            Object::Reference(id) => match resolver.resolve(*id) {
                Ok(font) => font,
                Err(_) => continue,
            },
            _ => continue,
        };
        let Some(dictionary) = font.as_dictionary() else {
            continue;
        };
        let mut diagnostics = Diagnostics::new(10);
        if let Some(font) = read_simple_font(resolver, dictionary, limits, 0, &mut diagnostics) {
            for code in 0..=255 {
                let _ = font.text(code);
                assert!(font.width(code).is_finite());
            }
        } else if let Some(font) =
            read_composite_font(resolver, dictionary, limits, 0, &mut diagnostics)
        {
            let bytes = b"\x00\x01\x00\x41\x81\x40\xFF\xD8\x3D\xDE\x00A";
            let codes = font.codes(bytes);
            assert!(codes.len() <= bytes.len());
            assert_eq!(codes.iter().map(|code| usize::from(code.length)).sum::<usize>(), bytes.len());
            for code in codes {
                let _ = (font.text(code), font.cid(code));
                assert!(font.width(code).is_finite());
            }
        }
    }
}

fn check(document: &Document<'_>, length: usize) {
    assert!(document.page_count() <= 10_000);
    for page in document.pages() {
        let media_box = page.media_box();
        assert!(media_box.width() > 0.0 && media_box.height() > 0.0);
        let crop_box = page.crop_box();
        assert!(crop_box.left >= media_box.left && crop_box.right <= media_box.right);
        assert!(crop_box.bottom >= media_box.bottom && crop_box.top <= media_box.top);
        assert!(page.rotation() % 90 == 0 && page.rotation() < 360);
    }
    if let Some(info) = document.info() {
        let _ = (info.title(), info.creation_date(), info.trapped());
    }
    assert!(document.diagnostics().iter().all(|d| d.offset() <= length));
}

// Runs the content of the first five pages, reads their first images and
// puts their text and blocks together: every glyph is placed at a finite position, every diagnostic is
// inside the input, no line ends with whitespace, and every word is inside
// its block.
fn contents(document: &mut Document<'_>, limits: Limits, length: usize) {
    let pages: Vec<_> = document.pages().iter().take(5).cloned().collect();
    let mut interpreter = Interpreter::new(limits);
    for page in &pages {
        let content = interpreter.run_page(document.resolver(), page);
        for glyph in &content.glyphs {
            assert!(glyph.matrix.is_finite() && glyph.width.is_finite());
        }
        assert!(content.diagnostics.iter().all(|d| d.offset() <= length));
        for image in content.images.iter().take(20) {
            let Ok(data) = read_image(document.resolver(), &image.source, limits) else {
                continue;
            };
            let d = &data.description;
            assert!(d.width >= 1 && d.height >= 1);
            if let Some(file) = image_file(&data) {
                match file.format {
                    ImageFormat::Png => assert!(file.data.starts_with(b"\x89PNG")),
                    ImageFormat::Jpeg | ImageFormat::Jpeg2000 => {}
                }
            }
        }
        let text = page_text(&content, &page.crop_box());
        assert!(!text.ends_with(char::is_whitespace));
        for block in page_blocks(&content, &page.crop_box()) {
            let b = block.bbox;
            let finite = |r: &ozaru_engine::document::Rectangle| {
                [r.left, r.bottom, r.right, r.top].iter().all(|v| v.is_finite())
            };
            for word in block.words().iter().flatten() {
                assert!(!word.text.is_empty());
                let w = word.bbox;
                if finite(&b) && finite(&w) {
                    assert!(w.left >= b.left && w.right <= b.right);
                    assert!(w.bottom >= b.bottom && w.top <= b.top);
                }
            }
        }
    }
}
