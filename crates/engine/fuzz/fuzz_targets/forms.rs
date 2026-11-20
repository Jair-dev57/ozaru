#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::document::FieldValue;
use ozaru_engine::writer::{Editor, fill_field};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF, reads the fields of its form and fills the first
// ones with text, a state and a choice, then saves. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() {
        return;
    }
    let fields = document.form_fields();
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    for field in fields.iter().take(16) {
        for value in [
            FieldValue::Text("Text with (parentheses) and ünïcode € ☺\nsecond line".into()),
            FieldValue::Text("Yes".into()),
            FieldValue::Text("Off".into()),
            FieldValue::Choices(vec!["a".into()]),
        ] {
            let _ = fill_field(&mut editor, field, &value);
        }
    }
    let _ = editor.save();
});
