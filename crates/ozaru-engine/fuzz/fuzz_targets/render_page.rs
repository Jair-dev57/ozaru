#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and draws its first page small. It never panics,
// and takes no more than the limits allow.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1 << 22,
        max_content_operations: 20_000,
        ..Limits::default()
    };
    let options = OpenOptions {
        limits,
        password: None,
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() {
        return;
    }
    let Some(page) = document.page(0).cloned() else {
        return;
    };
    let options = RenderOptions {
        scale: 0.25,
        ..RenderOptions::default()
    };
    let _ = render_page(document.resolver(), &page, &options, limits);
});
