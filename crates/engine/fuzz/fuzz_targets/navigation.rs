#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and reads its bookmarks, page labels, named
// destinations and attached files, whose data is decoded too. It never panics.
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
    let _ = document.outlines();
    let _ = document.page_labels();
    let _ = document.named_destinations();
    for attachment in document.attachments().into_iter().take(8) {
        if let Some(stream) = attachment.stream {
            let _ = document.resolver().stream_data(stream);
        }
    }
});
