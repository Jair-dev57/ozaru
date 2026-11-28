#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::{
    AnnotationFilter, Editor, NewButton, NewField, NewFieldKind,
};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF, adds a field of every kind to its first pages, fills
// and flattens them and the annotations, and saves it; the result must be a
// PDF the engine reads again. It never panics.
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
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let rect = [10.0, 20.0, 110.0, 50.0];
    let choices = vec![("a".to_string(), "A".to_string()), ("b".to_string(), "b".to_string())];
    let kinds = [
        NewFieldKind::Text { value: "Fuzz ñ".into(), multiline: true, password: false, max_length: Some(20) },
        NewFieldKind::Checkbox { checked: true },
        NewFieldKind::Radio {
            buttons: vec![
                NewButton { rect, export: "x".into() },
                NewButton { rect: [120.0, 20.0, 160.0, 50.0], export: "y".into() },
            ],
            selected: Some("y".into()),
        },
        NewFieldKind::Combo { options: choices.clone(), value: Some("a".into()), editable: false },
        NewFieldKind::List { options: choices, selected: vec!["b".into()], multiple: true },
        NewFieldKind::Button { caption: "Go".into(), uri: Some("https://a.b".into()) },
        NewFieldKind::Signature,
    ];
    for page in 0..editor.page_count().min(3) {
        for (number, kind) in kinds.iter().enumerate() {
            let mut field = NewField::new(format!("f{page}_{number}"), rect, kind.clone());
            field.background = Some([250, 250, 200]);
            let _ = editor.add_field(page, &field);
        }
        let _ = editor.flatten_widgets(page, Some(&["f0_0".to_string()]));
        let _ = editor.flatten_annotations(page, &AnnotationFilter::default());
        if page == 1 {
            let _ = editor.flatten_widgets(page, None);
        }
    }
    if let Ok(saved) = editor.save() {
        let mut reopened = Document::open(&saved).expect("a saved document opens");
        let _ = reopened.form_fields();
        for page in 0..reopened.page_count().min(3) {
            let _ = reopened.annotations(page);
        }
    }
});
