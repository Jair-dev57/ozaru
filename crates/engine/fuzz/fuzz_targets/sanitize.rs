#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::{Editor, SanitizeOptions};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and sanitizes it, first with the defaults and then
// with everything; the result must be a PDF the engine reads again with as many
// pages. It never panics.
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
    let count = document.page_count();
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let _ = editor.sanitize(&SanitizeOptions::default());
    let _ = editor.sanitize(&SanitizeOptions::everything());
    if let Ok(saved) = editor.save() {
        let mut reopened = Document::open(&saved).expect("a saved document opens");
        assert_eq!(reopened.page_count(), count);
        let _ = reopened.form_fields();
    }
});
