#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::compare::{CompareOptions, compare_documents};
use ozaru_engine::document::Rectangle;
use ozaru_engine::writer::{Editor, RedactArea, RedactOptions};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and compares it with itself, which must find nothing
// different, and with a copy that has a page moved and an area redacted, with
// the pages drawn too when there are few. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let (Ok(mut a), Ok(mut same)) = (
        Document::open_with(data, options),
        Document::open_with(data, options),
    ) else {
        return;
    };
    if a.needs_password() || a.page_count() == 0 || a.page_count() > 6 {
        return;
    }
    let visual = CompareOptions {
        visual: a.page_count() <= 2,
        scale: 0.25,
        ..CompareOptions::default()
    };
    let result = compare_documents(&mut a, &mut same, &visual);
    assert!(result.is_identical(), "a document differs from itself");

    // An edited copy.
    let Ok(mut editor) = Editor::new(&mut a) else {
        return;
    };
    let _ = editor.redact(
        0,
        &[RedactArea::black(Rectangle::new(0.0, 0.0, 300.0, 400.0))],
        &RedactOptions::default(),
    );
    let last = editor.page_count() - 1;
    let _ = editor.move_pages(&[last], 0);
    let Ok(saved) = editor.save() else {
        return;
    };
    let (Ok(mut original), Ok(mut edited)) = (
        Document::open_with(data, options),
        Document::open_with(&saved, options),
    ) else {
        return;
    };
    let _ = compare_documents(&mut original, &mut edited, &visual);
    let _ = compare_documents(&mut edited, &mut original, &CompareOptions::default());
});
