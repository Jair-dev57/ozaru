#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a document, locked and then with the password "user", and
// reads every page and the information dictionary. Each is a result or an error,
// and every error and diagnostic is inside the input.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        max_pages: 10_000,
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
    if document.unlock(b"user") {
        check(&document, data.len());
    }
});

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
