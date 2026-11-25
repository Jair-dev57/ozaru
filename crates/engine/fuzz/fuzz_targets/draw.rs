#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::fonts::StandardFont;
use ozaru_engine::writer::{Canvas, Editor};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF, draws on its first pages, and saves it; the
// result must be a PDF the engine reads again. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() || document.page_count() > 16 {
        return;
    }
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let mut canvas = Canvas::new();
    canvas.save();
    let _ = canvas.set_opacity(0.4, 0.8);
    canvas.set_fill_color([200, 30, 30]);
    let _ = canvas.rotate_around(100.0, 100.0, 30.0);
    let _ = canvas.text(10.0, 10.0, "Fuzz ñ €", StandardFont::TimesBold, 14.0);
    let _ = canvas.ellipse(0.0, 0.0, 80.0, 40.0);
    canvas.fill();
    canvas.restore();
    for page in 0..editor.page_count().min(4) {
        let _ = editor.draw(page, &canvas);
    }
    if let Ok(saved) = editor.save() {
        assert!(Document::open(&saved).is_ok());
    }
});
