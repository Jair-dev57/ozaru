#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::text::{
    TableStrategy, document_blocks_colored, document_markdown, document_tables, page_search,
    page_spans,
};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and finds the spans, a search, the tables and the
// Markdown of its first pages. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() || document.page_count() > 8 {
        return;
    }
    for page in document_blocks_colored(&mut document) {
        let _ = page_spans(&page.blocks);
        let _ = page_search(&page.blocks, "a e", false);
        let _ = page_search(&page.blocks, "ß", true);
    }
    let _ = document_tables(&mut document, TableStrategy::Both);
    let _ = document_markdown(&mut document, TableStrategy::Both);
});
