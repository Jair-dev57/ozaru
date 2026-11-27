#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::document::Rectangle;
use ozaru_engine::writer::{Editor, GraphicsMode, ImageMode, RedactArea, RedactOptions};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF, redacts areas of its first pages with each way of
// treating images and shapes, and saves it; the result must be a PDF the engine
// reads again. It never panics, and a page too complex to check is an error,
// never a partly redacted page.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() || document.page_count() > 8 {
        return;
    }
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let areas = [
        RedactArea::black(Rectangle::new(0.0, 0.0, 300.0, 400.0)),
        RedactArea {
            rect: Rectangle::new(100.0, 300.0, 500.0, 700.0),
            fill: None,
        },
    ];
    for (page, (images, graphics)) in (0..editor.page_count().min(3)).zip([
        (ImageMode::Pixels, GraphicsMode::Covered),
        (ImageMode::Remove, GraphicsMode::Touched),
        (ImageMode::Keep, GraphicsMode::Keep),
    ]) {
        let options = RedactOptions {
            images,
            graphics,
            annotations: true,
        };
        let _ = editor.redact(page, &areas, &options);
    }
    if let Ok(saved) = editor.save() {
        let mut reopened = Document::open(&saved).expect("a saved document opens");
        for page in 0..reopened.page_count().min(3) {
            let _ = reopened.annotations(page);
        }
    }
});
