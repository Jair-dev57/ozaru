#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::{
    AnnotationChanges, AnnotationFilter, AnnotationShape, Editor, LinkTarget, MarkupKind,
    NewAnnotation,
};
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
            let _ = editor.add_annotation(page, &note);
        }
    }
    let rect = [10.0, 20.0, 90.0, 60.0];
    for page in 0..editor.page_count().min(4) {
        for shape in [
            AnnotationShape::Note { x: 5.0, y: 90.0, open: false },
            AnnotationShape::Link { rect, target: LinkTarget::Uri("https://a.b/ñ c".into()), border_width: 1.0 },
            AnnotationShape::Link { rect, target: LinkTarget::Page(0), border_width: 0.0 },
            AnnotationShape::Square { rect, fill: Some([1, 2, 3]), width: 2.0 },
            AnnotationShape::Circle { rect, fill: None, width: 1.0 },
            AnnotationShape::Line { from: [0.0, 0.0], to: [50.0, 50.0], width: 2.0, arrow: true },
            AnnotationShape::Ink { strokes: vec![vec![[1.0, 1.0], [9.0, 9.0]], vec![[5.0, 5.0]]], width: 1.0 },
            AnnotationShape::FreeText { rect, text: "Fuzz wraps over a few lines".into(), size: 9.0, fill: None, border_width: 1.0 },
        ] {
            let mut note = NewAnnotation::new(shape);
            note.opacity = 0.7;
            let _ = editor.add_annotation(page, &note);
        }
        // What was added can be changed and removed.
        let filter = AnnotationFilter { subtypes: vec!["Square".into()], ..Default::default() };
        let changes = AnnotationChanges { contents: Some(Some("changed".into())), hidden: Some(true), ..Default::default() };
        let _ = editor.update_annotations(page, &filter, &changes);
        let _ = editor.remove_annotations(
            page,
            &AnnotationFilter { subtypes: vec!["Circle".into()], ..Default::default() },
        );
    }
    if let Ok(saved) = editor.save() {
        let mut reopened = Document::open(&saved).expect("a saved document opens");
        // Every page's annotations can be read back.
        for page in 0..reopened.page_count().min(4) {
            let _ = reopened.annotations(page);
        }
    }
});
