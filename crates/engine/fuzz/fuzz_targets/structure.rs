#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::document::{Bookmark, Date, Rectangle};
use ozaru_engine::writer::{Editor, NewAttachment, PageLabel};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF, writes metadata, bookmarks, page labels and an
// attachment into it, crops, adds and deletes pages, and saves it, with the
// result checked to be a PDF the engine reads again. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() || document.page_count() > 64 {
        return;
    }
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let _ = editor.set_info_text("Title", Some("título 日本"));
    let date = Date {
        year: 2026,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        utc_offset: Some(-60),
    };
    let _ = editor.set_info_date("CreationDate", Some(&date));
    let _ = editor.set_xmp(Some("<x/>"));
    let bookmark = |title: &str, level, page| Bookmark {
        title: title.to_string(),
        level,
        page,
        uri: None,
        open: level == 1,
    };
    let _ = editor.set_outlines(&[
        bookmark("a", 1, Some(0)),
        bookmark("b", 2, Some(1)),
        bookmark("c", 1, None),
    ]);
    let _ = editor.set_page_labels(&[PageLabel {
        page: 0,
        style: Some('r'),
        prefix: "p".to_string(),
        start: 1,
    }]);
    let attachment = NewAttachment {
        name: "f".to_string(),
        filename: "f.txt".to_string(),
        data: data.iter().copied().take(256).collect(),
        description: None,
        mime_type: Some("text/plain".to_string()),
    };
    let _ = editor.add_attachment(&attachment);
    let _ = editor.remove_attachment("g");
    let _ = editor.crop_page(0, Rectangle::new(0.0, 0.0, 50.0, 50.0));
    let _ = editor.insert_blank_page(0, 100.0, 100.0);
    let _ = editor.delete_pages(&[1]);
    if let Ok(saved) = editor.save() {
        assert!(Document::open(&saved).is_ok());
    }
});
