#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::writer::Editor;
use ozaru_engine::{Document, OpenOptions};

// Any document the engine opens must save, with its pages turned and some
// left out, moved and repeated, and its pages added again from another copy,
// to a file
// that opens again with as many pages, that saves again to the same bytes.
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
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    let count = document.page_count();
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    if count > 0 {
        editor.rotate_page(0, 90).unwrap();
    }
    // Every other page, from the last, and the first one twice.
    let mut pages: Vec<usize> = (0..count).rev().step_by(2).collect();
    if count > 0 {
        pages.push(0);
        pages.push(0);
    }
    if editor.select_pages(&pages).is_err() {
        return;
    }
    // And the pages of a second copy of the document, in reverse order.
    let Ok(mut other) = Document::open_with(data, options) else {
        return;
    };
    let reversed: Vec<usize> = (0..other.page_count()).rev().collect();
    if editor.import_pages(&mut other, &reversed).is_err() {
        return;
    }
    let count = pages.len() + reversed.len();
    let Ok(saved) = editor.save() else {
        return;
    };
    let mut again = Document::open_with(&saved, options).expect("saved files open");
    assert_eq!(again.page_count(), count);
    let resaved = Editor::new(&mut again).unwrap().save().unwrap();
    let mut third = Document::open_with(&resaved, options).unwrap();
    assert_eq!(third.page_count(), count);
    assert_eq!(Editor::new(&mut third).unwrap().save().unwrap(), resaved);
});
