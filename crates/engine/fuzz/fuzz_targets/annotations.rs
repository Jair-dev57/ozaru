#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and reads the annotations of its first pages,
// links and named destinations included. It never panics.
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
    for index in 0..document.page_count().min(8) {
        let _ = document.annotations(index);
    }
});
