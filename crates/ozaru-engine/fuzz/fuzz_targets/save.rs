#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::writer::Editor;
use ozaru_engine::{Document, OpenOptions};

// Any document the engine opens must save, with its pages turned, to a file
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
