//! Adding form fields: what a reader sees of them, and that they work as fields.

use ozaru_engine::Document;
use ozaru_engine::base::ErrorKind;
use ozaru_engine::document::{FieldKind, FieldValue};
use ozaru_engine::render::{RenderOptions, render_page};
use ozaru_engine::text::document_text;
use ozaru_engine::writer::AnnotationFilter;
use ozaru_engine::writer::{Editor, NewButton, NewField, NewFieldKind, fill_field};

mod common;
use common::one_page;

fn with_fields(data: &[u8], add: impl FnOnce(&mut Editor<'_, '_>)) -> Vec<u8> {
    let mut document = Document::open(data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    add(&mut editor);
    editor.save().unwrap()
}

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 3] {
    let mut document = Document::open(data).unwrap();
    let page = document.page(0).unwrap().clone();
    let limits = document.limits();
    let pixmap = render_page(
        document.resolver(),
        &page,
        &RenderOptions::default(),
        limits,
    )
    .unwrap();
    let [r, g, b, a] = pixmap.pixel(x, y).unwrap();
    let over_white =
        |c: u8| ((u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a))) / 255) as u8;
    [over_white(r), over_white(g), over_white(b)]
}

// Whether some pixel of the area is dark.
fn has_ink(data: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> bool {
    ys.into_iter().any(|y| {
        xs.clone()
            .any(|x| pixel(data, x, y).iter().all(|&c| c < 110))
    })
}

fn blank() -> Vec<u8> {
    one_page("", "")
}

fn fields(data: &[u8]) -> Vec<ozaru_engine::document::FormField> {
    Document::open(data).unwrap().form_fields()
}

fn text_field(name: &str, rect: [f64; 4], value: &str) -> NewField {
    NewField::new(
        name,
        rect,
        NewFieldKind::Text {
            value: value.to_string(),
            multiline: false,
            password: false,
            max_length: None,
        },
    )
}

#[test]
fn a_text_field_is_in_the_form_with_its_value_and_look() {
    let mut field = text_field("name", [100.0, 700.0, 300.0, 720.0], "Ana Maria ñ");
    field.background = Some([255, 255, 200]);
    field.tooltip = Some("Your name".to_string());
    let saved = with_fields(&blank(), |editor| {
        editor.add_field(0, &field).unwrap();
    });
    let found = fields(&saved);
    assert_eq!(found.len(), 1);
    let f = &found[0];
    assert_eq!((f.name.as_str(), f.kind), ("name", FieldKind::Text));
    assert_eq!(f.value, Some(FieldValue::Text("Ana Maria ñ".to_string())));
    assert_eq!(f.widgets.len(), 1);
    assert_eq!(f.widgets[0].page, Some(0));
    assert_eq!(f.widgets[0].rect.left, 100.0);
    // It is drawn: its background, its border and its text.
    assert_eq!(pixel(&saved, 250, 792 - 710), [255, 255, 200]);
    assert_eq!(pixel(&saved, 100, 792 - 710), [0, 0, 0]);
    assert!(has_ink(&saved, 103..250, 792 - 718..792 - 702));
}

#[test]
fn a_new_field_can_be_filled_like_any_other() {
    let saved = with_fields(&blank(), |editor| {
        let mut field = text_field("city", [100.0, 700.0, 300.0, 720.0], "");
        field.kind = NewFieldKind::Text {
            value: String::new(),
            multiline: false,
            password: false,
            max_length: Some(6),
        };
        editor.add_field(0, &field).unwrap();
    });
    let found = fields(&saved);
    assert_eq!(found[0].max_length, Some(6));
    assert!(found[0].value.is_none() || found[0].value == Some(FieldValue::Text(String::new())));
    let mut document = Document::open(&saved).unwrap();
    let field = document.form_fields().remove(0);
    let mut editor = Editor::new(&mut document).unwrap();
    fill_field(&mut editor, &field, &FieldValue::Text("Lima".into())).unwrap();
    assert!(
        fill_field(
            &mut editor,
            &field,
            &FieldValue::Text("Too long a name".into())
        )
        .is_err()
    );
    let filled = editor.save().unwrap();
    assert_eq!(
        fields(&filled)[0].value,
        Some(FieldValue::Text("Lima".to_string()))
    );
}

#[test]
fn text_options_are_kept() {
    let saved = with_fields(&blank(), |editor| {
        let mut multi = text_field("bio", [50.0, 500.0, 250.0, 600.0], "line one\nline two");
        multi.kind = NewFieldKind::Text {
            value: "line one\nline two".into(),
            multiline: true,
            password: false,
            max_length: None,
        };
        multi.font_size = 10.0;
        multi.align = 2;
        editor.add_field(0, &multi).unwrap();
        let mut secret = text_field("pin", [50.0, 450.0, 150.0, 470.0], "1234");
        secret.kind = NewFieldKind::Text {
            value: "1234".into(),
            multiline: false,
            password: true,
            max_length: None,
        };
        editor.add_field(0, &secret).unwrap();
    });
    let found = fields(&saved);
    assert!(found[0].is_multiline() && !found[0].is_password());
    assert!(found[1].is_password());
    assert_eq!(
        (
            found[0].quadding,
            found[0]
                .appearance_string
                .as_deref()
                .map(|d| d.contains("10 Tf"))
        ),
        (2, Some(true))
    );
}

#[test]
fn check_boxes_are_on_or_off() {
    let saved = with_fields(&blank(), |editor| {
        for (name, y, checked) in [("yes", 700.0, true), ("no", 650.0, false)] {
            let field = NewField::new(
                name,
                [100.0, y, 116.0, y + 16.0],
                NewFieldKind::Checkbox { checked },
            );
            editor.add_field(0, &field).unwrap();
        }
    });
    let found = fields(&saved);
    assert_eq!(found[0].value, Some(FieldValue::Text("Yes".into())));
    assert_eq!(found[1].value, Some(FieldValue::Text("Off".into())));
    assert!(found.iter().all(|f| f.kind == FieldKind::Checkbox));
    // The tick is dark in the middle of the box that is on, and the other is empty.
    let dark = |x: usize, y: usize| pixel(&saved, x, y).iter().any(|&c| c < 200);
    assert!((104..112).any(|x| (792 - 712..792 - 704).any(|y| dark(x, y))));
    assert!(!(104..112).any(|x| (792 - 662..792 - 654).any(|y| dark(x, y))));
}

#[test]
fn radio_buttons_are_one_field_with_the_value_of_the_one_that_is_on() {
    let saved = with_fields(&blank(), |editor| {
        let group = NewField::new(
            "plan",
            [0.0; 4],
            NewFieldKind::Radio {
                buttons: vec![
                    NewButton {
                        rect: [100.0, 700.0, 116.0, 716.0],
                        export: "basic".into(),
                    },
                    NewButton {
                        rect: [140.0, 700.0, 156.0, 716.0],
                        export: "pro".into(),
                    },
                    NewButton {
                        rect: [180.0, 700.0, 196.0, 716.0],
                        export: "team".into(),
                    },
                ],
                selected: Some("pro".into()),
            },
        );
        editor.add_field(0, &group).unwrap();
    });
    let found = fields(&saved);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, FieldKind::Radio);
    assert_eq!(found[0].value, Some(FieldValue::Text("pro".into())));
    let states: Vec<_> = found[0]
        .widgets
        .iter()
        .map(|w| w.on_state.clone().unwrap())
        .collect();
    assert_eq!(states, ["basic", "pro", "team"]);
    assert!(found[0].widgets.iter().all(|w| w.page == Some(0)));
    // The dot is in the middle of the second button only.
    assert!(pixel(&saved, 148, 792 - 708).iter().all(|&c| c < 100));
    assert_eq!(pixel(&saved, 108, 792 - 708), [255, 255, 255]);
    assert_eq!(pixel(&saved, 188, 792 - 708), [255, 255, 255]);
    // Changing the value changes which is on.
    let mut document = Document::open(&saved).unwrap();
    let field = document.form_fields().remove(0);
    let mut editor = Editor::new(&mut document).unwrap();
    fill_field(&mut editor, &field, &FieldValue::Text("team".into())).unwrap();
    let again = editor.save().unwrap();
    assert!(pixel(&again, 188, 792 - 708).iter().all(|&c| c < 100));
    assert_eq!(pixel(&again, 148, 792 - 708), [255, 255, 255]);
}

#[test]
fn drop_downs_and_lists_have_their_options() {
    let options = vec![
        ("es".to_string(), "Spanish".to_string()),
        ("en".to_string(), "English".to_string()),
        ("fr".to_string(), "fr".to_string()),
    ];
    let saved = with_fields(&blank(), |editor| {
        let combo = NewField::new(
            "lang",
            [100.0, 700.0, 250.0, 720.0],
            NewFieldKind::Combo {
                options: options.clone(),
                value: Some("en".into()),
                editable: false,
            },
        );
        editor.add_field(0, &combo).unwrap();
        let list = NewField::new(
            "likes",
            [100.0, 600.0, 250.0, 680.0],
            NewFieldKind::List {
                options: options.clone(),
                selected: vec!["es".into(), "fr".into()],
                multiple: true,
            },
        );
        editor.add_field(0, &list).unwrap();
    });
    let found = fields(&saved);
    assert_eq!(found[0].kind, FieldKind::Combo);
    assert_eq!(found[0].options, options);
    assert_eq!(found[0].value, Some(FieldValue::Text("en".into())));
    assert_eq!(found[1].kind, FieldKind::List);
    assert!(found[1].is_multi_select());
    assert_eq!(
        found[1].value,
        Some(FieldValue::Choices(vec!["es".into(), "fr".into()]))
    );
    // The text of the drop-down and the options of the list are drawn.
    assert!(has_ink(&saved, 104..200, 792 - 718..792 - 702));
    assert!(has_ink(&saved, 104..200, 792 - 680..792 - 640));
}

#[test]
fn buttons_and_signature_fields() {
    let saved = with_fields(&blank(), |editor| {
        let mut button = NewField::new(
            "go",
            [100.0, 700.0, 200.0, 725.0],
            NewFieldKind::Button {
                caption: "Open site".into(),
                uri: Some("https://example.com".into()),
            },
        );
        button.background = Some([200, 220, 255]);
        editor.add_field(0, &button).unwrap();
        let sign = NewField::new(
            "signature",
            [100.0, 600.0, 300.0, 650.0],
            NewFieldKind::Signature,
        );
        editor.add_field(0, &sign).unwrap();
    });
    let found = fields(&saved);
    assert_eq!(found[0].kind, FieldKind::PushButton);
    assert_eq!(found[1].kind, FieldKind::Signature);
    assert_eq!(pixel(&saved, 105, 792 - 705), [200, 220, 255]);
    // The caption is dark over the light blue.
    assert!(has_ink(&saved, 110..190, 792 - 718..792 - 706));
}

#[test]
fn fields_are_added_to_a_form_that_exists_and_keep_its_fields() {
    let first = with_fields(&blank(), |editor| {
        editor
            .add_field(0, &text_field("one", [10.0, 10.0, 100.0, 30.0], "1"))
            .unwrap();
    });
    let second = with_fields(&first, |editor| {
        editor
            .add_field(0, &text_field("two", [10.0, 50.0, 100.0, 70.0], "2"))
            .unwrap();
    });
    let names: Vec<_> = fields(&second).into_iter().map(|f| f.name).collect();
    assert_eq!(names, ["one", "two"]);
}

#[test]
fn bad_fields_are_errors() {
    let first = with_fields(&blank(), |editor| {
        editor
            .add_field(0, &text_field("taken", [10.0, 10.0, 100.0, 30.0], ""))
            .unwrap();
    });
    let mut document = Document::open(&first).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let rect = [10.0, 100.0, 100.0, 120.0];
    let mut kind = |field: NewField| editor.add_field(0, &field).unwrap_err().kind();
    assert_eq!(kind(text_field("taken", rect, "")), ErrorKind::InvalidField);
    assert_eq!(kind(text_field("", rect, "")), ErrorKind::InvalidField);
    assert_eq!(kind(text_field("a.b", rect, "")), ErrorKind::InvalidField);
    assert_eq!(
        kind(text_field("flat", [5.0, 5.0, 5.0, 50.0], "")),
        ErrorKind::InvalidBox
    );
    assert_eq!(
        kind(text_field("nan", [f64::NAN, 5.0, 5.0, 50.0], "")),
        ErrorKind::NonFiniteNumber
    );
    assert_eq!(
        kind(NewField::new(
            "none",
            rect,
            NewFieldKind::List {
                options: vec![],
                selected: vec![],
                multiple: false
            }
        )),
        ErrorKind::InvalidField
    );
    assert_eq!(
        kind(NewField::new(
            "empty",
            rect,
            NewFieldKind::Radio {
                buttons: vec![],
                selected: None
            }
        )),
        ErrorKind::InvalidField
    );
    let twice = NewFieldKind::Radio {
        buttons: vec![
            NewButton {
                rect,
                export: "a".into(),
            },
            NewButton {
                rect,
                export: "a".into(),
            },
        ],
        selected: None,
    };
    assert_eq!(
        kind(NewField::new("dup", rect, twice)),
        ErrorKind::InvalidField
    );
    let missing = NewFieldKind::Radio {
        buttons: vec![NewButton {
            rect,
            export: "a".into(),
        }],
        selected: Some("b".into()),
    };
    assert_eq!(
        kind(NewField::new("sel", rect, missing)),
        ErrorKind::InvalidFieldValue
    );
    let combo = NewFieldKind::Combo {
        options: vec![("a".into(), "a".into())],
        value: Some("zzz".into()),
        editable: false,
    };
    assert_eq!(
        kind(NewField::new("combo", rect, combo)),
        ErrorKind::InvalidFieldValue
    );
    let past = text_field("page", rect, "");
    let mut document = Document::open(&first).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    assert_eq!(
        editor.add_field(3, &past).unwrap_err().kind(),
        ErrorKind::PageNotFound
    );
}

#[test]
fn added_fields_can_be_flattened_into_the_page() {
    let saved = with_fields(&blank(), |editor| {
        editor
            .add_field(
                0,
                &text_field("name", [100.0, 700.0, 300.0, 720.0], "Ana Maria"),
            )
            .unwrap();
        let combo = NewField::new(
            "lang",
            [100.0, 650.0, 250.0, 670.0],
            NewFieldKind::Combo {
                options: vec![("en".into(), "English".into())],
                value: Some("en".into()),
                editable: false,
            },
        );
        editor.add_field(0, &combo).unwrap();
    });
    let mut document = Document::open(&saved).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let report = editor.flatten_widgets(0, None).unwrap();
    assert_eq!((report.painted, report.fields), (2, 2));
    let flat = editor.save().unwrap();
    assert!(fields(&flat).is_empty());
    let mut document = Document::open(&flat).unwrap();
    let text = &document_text(&mut document)[0].text;
    assert!(
        text.contains("Ana Maria") && text.contains("English"),
        "{text}"
    );
    let _ = AnnotationFilter::default();
}
