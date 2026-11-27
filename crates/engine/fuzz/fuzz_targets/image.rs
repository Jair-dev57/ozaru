#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::images::read_png;
use ozaru_engine::writer::{Canvas, Editor, EmbeddedImage};
use ozaru_engine::Document;
use std::sync::Arc;

const BLANK: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\n\
trailer<</Root 1 0 R>>\n";

// Reads the input as a PNG or JPEG file; if the engine can embed it, draws it
// on a page, saves, and the result must open again. It never panics, whatever
// the file says about its size or holds as pixels.
fuzz_target!(|data: &[u8]| {
    let _ = read_png(data);
    let Ok(image) = EmbeddedImage::parse(data) else {
        return;
    };
    let mut canvas = Canvas::new();
    if canvas.image(10.0, 10.0, 80.0, 60.0, &Arc::new(image)).is_err() {
        return;
    }
    let Ok(mut document) = Document::open(BLANK) else {
        return;
    };
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    if editor.draw(0, &canvas).is_ok() {
        let saved = editor.save().expect("a drawn document saves");
        assert!(Document::open(&saved).is_ok());
    }
});
