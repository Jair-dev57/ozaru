#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::{Editor, MarkupKind, NewAnnotation};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF, adds text markups to its first pages, and saves
// it; the result must be a PDF the engine reads again, with the annotations
// it was given. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() || document.page_count() > 16 {
        return;
    }
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let quad = [10.0, 40.0, 90.0, 40.0, 10.0, 20.0, 90.0, 20.0];
    let mut added = 0;
    for page in 0..editor.page_count().min(4) {
        for kind in [
            MarkupKind::Highlight,
            MarkupKind::Underline,
            MarkupKind::StrikeOut,
            MarkupKind::Squiggly,
        ] {
            let mut note = NewAnnotation::text_markup(kind, vec![quad, quad]);
            note.contents = Some("Fuzz ñ".to_string());
            note.opacity = 0.5;
            if editor.add_annotation(page, &note).is_ok() {
                added += 1;
            }
        }
    }
    if let Ok(saved) = editor.save() {
        let mut reopened = Document::open(&saved).expect("a saved document opens");
        let found: usize = (0..reopened.page_count().min(4))
            .map(|page| reopened.annotations(page).len())
            .sum();
        assert!(found >= added);
    }
});
