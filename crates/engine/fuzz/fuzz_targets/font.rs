#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::{Canvas, EmbeddedFont, Editor};
use ozaru_engine::Document;
use std::sync::Arc;

const BLANK: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\n\
trailer<</Root 1 0 R>>\n";

// Reads the input as a font file; if it is one the engine can embed, draws
// some of its characters with it, saves, and the result must open again. It
// never panics, whatever the tables hold.
fuzz_target!(|data: &[u8]| {
    let Ok(font) = EmbeddedFont::parse(data) else {
        return;
    };
    let font = Arc::new(font);
    let text: String = (0x20_u32..0x2200)
        .filter_map(char::from_u32)
        .filter(|&c| font.glyph(c).is_some())
        .step_by(7)
        .take(40)
        .collect();
    let mut canvas = Canvas::new();
    if canvas.text_embedded(10.0, 10.0, &text, &font, 12.0).is_err() {
        return;
    }
    let _ = font.text_width(12.0, &text);
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
