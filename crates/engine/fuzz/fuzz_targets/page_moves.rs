#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::Editor;
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and inserts pages of another copy of it, replaces
// some, and moves others; the result must be a PDF the engine reads again with
// the pages that were asked for. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    let Ok(mut other) = Document::open_with(data, options) else {
        return;
    };
    let count = document.page_count();
    if document.needs_password() || count == 0 || count > 12 {
        return;
    }
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let mut expected = count;
    if editor.insert_pages(count / 2, &mut other, &[0, count - 1]).is_ok() {
        expected += 2;
    }
    if editor.replace_pages(0, 1, &mut other, &[count - 1]).is_ok() {
        // One page for one: the same number.
    }
    let _ = editor.move_pages(&[expected - 1], 0);
    if let Ok(saved) = editor.save() {
        let reopened = Document::open(&saved).expect("a saved document opens");
        assert_eq!(reopened.page_count(), expected);
    }
});
