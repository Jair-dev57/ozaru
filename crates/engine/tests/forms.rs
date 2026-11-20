//! Form fields: reading the tree with what fields inherit, and filling them.

use ozaru_engine::Document;
use ozaru_engine::document::{FieldKind, FieldValue};
use ozaru_engine::writer::{Editor, fill_field};

mod common;
use common::{file_with, stream_object};

// Objects: 4 `person` (parent, no widget) with kids 5 `name` (text, centered,
// inherits /FT /Tx and /DA from it), 6 `pass` (password, max length 8);
// 7 `agree` (check box without drawings); 8 `plan` (radio group) with
// widgets 9 and 10; 11 `lang` (multi-select list); 12 `country` (combo);
// 13 `bio` (multiline); 14 `go` (push button); 15 `sign` (signature);
// 16 `kind` (check box with drawings); 17 `note`, value as a stream.
fn pdf() -> Vec<u8> {
    let widget = |rect: &str| format!("/Subtype /Widget /Rect [{rect}] /P 3 0 R");
    file_with(&[
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 7 0 R 8 0 R 11 0 R 12 0 R \
             13 0 R 14 0 R 15 0 R 16 0 R 17 0 R 18 0 R 4 0 R] /DA (/Helv 0 Tf 0 g) \
             /DR << /Font << /Helv 19 0 R >> >> >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Annots \
             [5 0 R 6 0 R 7 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R 15 0 R 16 0 R 17 0 R] >>"
                .to_string(),
        ),
        (
            4,
            "<< /T (person) /FT /Tx /Ff 0 /DA (/Helv 12 Tf 0 0 1 rg) /Kids [5 0 R 6 0 R] >>"
                .to_string(),
        ),
        (
            5,
            format!(
                "<< /T (name) /Parent 4 0 R /Q 1 /V (Ana) {} /MK << /BG [0.9 0.9 1] /BC [0] >> >>",
                widget("10 350 150 370")
            ),
        ),
        (
            6,
            format!(
                "<< /T (pass) /Parent 4 0 R /Ff 8192 /MaxLen 8 {} >>",
                widget("10 320 150 340")
            ),
        ),
        (
            7,
            format!("<< /T (agree) /FT /Btn {} >>", widget("10 290 26 306")),
        ),
        (
            8,
            "<< /T (plan) /FT /Btn /Ff 32768 /V /b /Kids [9 0 R 10 0 R] >>".to_string(),
        ),
        (
            9,
            format!(
                "<< /Parent 8 0 R /AS /Off /AP << /N << /a 20 0 R /Off 20 0 R >> >> {} >>",
                widget("10 260 26 276")
            ),
        ),
        (
            10,
            format!(
                "<< /Parent 8 0 R /AS /b /AP << /N << /b 20 0 R /Off 20 0 R >> >> {} >>",
                widget("40 260 56 276")
            ),
        ),
        (
            11,
            format!(
                "<< /T (lang) /FT /Ch /Ff 2097152 /Opt [(es) [(en) (English)] (fr)] /V [(es) (fr)] {} >>",
                widget("10 180 150 240")
            ),
        ),
        (
            12,
            format!(
                "<< /T (country) /FT /Ch /Ff 131072 /Opt [(co) (cl)] /DA (/Helv 11 Tf 0 g) {} >>",
                widget("10 150 150 170")
            ),
        ),
        (
            13,
            format!(
                "<< /T (bio) /FT /Tx /Ff 4096 /DA (/Helv 10 Tf 0 g) {} >>",
                widget("10 80 150 140")
            ),
        ),
        (
            14,
            format!(
                "<< /T (go) /FT /Btn /Ff 65536 {} >>",
                widget("200 80 250 100")
            ),
        ),
        (
            15,
            format!("<< /T (sign) /FT /Sig {} >>", widget("200 120 250 140")),
        ),
        (
            16,
            format!(
                "<< /T (kind) /FT /Btn /AS /Off /AP << /N << /On 20 0 R /Off 20 0 R >> >> {} >>",
                widget("200 290 216 306")
            ),
        ),
        (
            17,
            format!(
                "<< /T (note) /FT /Tx /V 21 0 R /DA (/Courier 9 Tf 1 0 0 rg) {} >>",
                widget("200 200 290 220")
            ),
        ),
        (18, "<< /T (plain) /FT /Tx /V (no widgets) >>".to_string()),
        (
            19,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_string(),
        ),
        (
            20,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 16 16]",
                "0 g 0 0 16 16 re f",
            ),
        ),
        (21, stream_object("", "From a stream")),
    ])
}

fn field<'a>(
    fields: &'a [ozaru_engine::document::FormField],
    name: &str,
) -> &'a ozaru_engine::document::FormField {
    fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn the_tree_is_read_with_what_fields_inherit() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let fields = document.form_fields();
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    // `person` isn't a field itself, and the cycle (4 listed twice) is skipped.
    assert_eq!(
        names,
        [
            "person.name",
            "person.pass",
            "agree",
            "plan",
            "lang",
            "country",
            "bio",
            "go",
            "sign",
            "kind",
            "note",
            "plain"
        ]
    );
    let name = field(&fields, "person.name");
    assert_eq!(name.kind, FieldKind::Text);
    assert_eq!(name.value, Some(FieldValue::Text("Ana".into())));
    assert_eq!(name.quadding, 1);
    assert_eq!(
        name.appearance_string.as_deref(),
        Some("/Helv 12 Tf 0 0 1 rg")
    );
    assert_eq!(name.widgets.len(), 1);
    assert_eq!(name.widgets[0].page, Some(0));
    let pass = field(&fields, "person.pass");
    assert!(pass.is_password() && pass.max_length == Some(8) && !pass.is_read_only());
    assert_eq!(field(&fields, "agree").kind, FieldKind::Checkbox);
    let plan = field(&fields, "plan");
    assert_eq!(plan.kind, FieldKind::Radio);
    assert_eq!(plan.value, Some(FieldValue::Text("b".into())));
    assert_eq!(plan.widgets.len(), 2);
    assert_eq!(plan.widgets[1].on_state.as_deref(), Some("b"));
    let lang = field(&fields, "lang");
    assert!(lang.is_multi_select());
    assert_eq!(
        lang.options,
        [
            ("es".into(), "es".into()),
            ("en".into(), "English".into()),
            ("fr".into(), "fr".into())
        ]
    );
    assert_eq!(
        lang.value,
        Some(FieldValue::Choices(vec!["es".into(), "fr".into()]))
    );
    assert_eq!(field(&fields, "country").kind, FieldKind::Combo);
    assert!(field(&fields, "bio").is_multiline());
    assert_eq!(field(&fields, "go").kind, FieldKind::PushButton);
    assert_eq!(field(&fields, "sign").kind, FieldKind::Signature);
    assert_eq!(
        field(&fields, "note").value,
        Some(FieldValue::Text("From a stream".into()))
    );
    assert!(field(&fields, "plain").widgets.is_empty());
}

// Fills `values`, saves, and reads the fields of the result.
fn filled(values: &[(&str, FieldValue)]) -> (Vec<u8>, Vec<ozaru_engine::document::FormField>) {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let fields = document.form_fields();
    let mut editor = Editor::new(&mut document).unwrap();
    for (name, value) in values {
        fill_field(&mut editor, field(&fields, name), value).unwrap();
    }
    let saved = editor.save().unwrap();
    let mut again = Document::open(&saved).unwrap();
    let fields = again.form_fields();
    (saved, fields)
}

fn text(s: &str) -> FieldValue {
    FieldValue::Text(s.to_string())
}

#[test]
fn values_are_set_and_widgets_drawn() {
    let (saved, fields) = filled(&[
        ("person.name", text("José")),
        ("person.pass", text("secret")),
        ("agree", text("Yes")),
        ("plan", text("a")),
        ("lang", FieldValue::Choices(vec!["en".into()])),
        ("country", text("cl")),
        (
            "bio",
            text("A long paragraph of words that needs several lines to fit in the box\nnext"),
        ),
        ("note", text("€ and ‘quotes’ ☺")),
    ]);
    assert_eq!(field(&fields, "person.name").value, Some(text("José")));
    assert_eq!(field(&fields, "agree").value, Some(text("Yes")));
    assert_eq!(field(&fields, "plan").value, Some(text("a")));
    assert_eq!(
        field(&fields, "lang").value,
        Some(FieldValue::Choices(vec!["en".into()]))
    );
    assert_eq!(field(&fields, "country").value, Some(text("cl")));
    // The check box without drawings got some, and shows the state.
    let mut document = Document::open(&saved).unwrap();
    let annotations = document.annotations(0);
    let agree = annotations
        .iter()
        .find(|a| a.id == Some(field(&fields, "agree").widgets[0].id))
        .unwrap();
    assert!(agree.appearance.is_some());
    // The radio group: widget `a` is on and `b` is off.
    let plan = field(&fields, "plan");
    let mut state = |index: usize| {
        let id = plan.widgets[index].id;
        let object = document.resolver().resolve(id).unwrap();
        let dictionary = object.as_dictionary().unwrap().clone();
        String::from_utf8_lossy(dictionary.get("AS").unwrap().as_name().unwrap().as_bytes())
            .into_owned()
    };
    assert_eq!((state(0), state(1)), ("a".to_string(), "Off".to_string()));
    // Every widget draws, and the page renders with them.
    let limits = document.limits();
    let page = document.page(0).unwrap().clone();
    let image = ozaru_engine::render::render_page(
        document.resolver(),
        &page,
        &ozaru_engine::render::RenderOptions::default(),
        limits,
    )
    .unwrap();
    assert!(image.data().chunks(4).any(|p| p[0] < 200));
}

#[test]
fn what_a_field_cant_hold_is_refused() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let fields = document.form_fields();
    let mut editor = Editor::new(&mut document).unwrap();
    let mut refuse = |name: &str, value: FieldValue| {
        assert!(
            fill_field(&mut editor, field(&fields, name), &value).is_err(),
            "{name} took {value:?}"
        );
    };
    refuse("person.pass", text("more than eight"));
    refuse("agree", text("Maybe"));
    refuse("agree", FieldValue::Choices(vec![]));
    refuse("country", text("mx"));
    refuse("lang", FieldValue::Choices(vec!["xx".into()]));
    refuse("go", text("x"));
    refuse("sign", text("x"));
    refuse("person.name", FieldValue::Choices(vec!["a".into()]));
}

#[test]
fn a_list_that_doesnt_allow_several_takes_one() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let mut fields = document.form_fields();
    let mut editor = Editor::new(&mut document).unwrap();
    // The same field without the multi-select flag.
    let list = fields.iter_mut().find(|f| f.name == "lang").unwrap();
    list.flags = 0;
    assert!(
        fill_field(
            &mut editor,
            list,
            &FieldValue::Choices(vec!["es".into(), "en".into()])
        )
        .is_err()
    );
    assert!(fill_field(&mut editor, list, &FieldValue::Choices(vec!["es".into()])).is_ok());
}

#[test]
fn an_editable_combo_takes_other_values_and_incremental_updates_keep_the_file() {
    let data = pdf();
    let mut document = Document::open(&data).unwrap();
    let mut fields = document.form_fields();
    let mut editor = Editor::new(&mut document).unwrap();
    let combo = fields.iter_mut().find(|f| f.name == "country").unwrap();
    combo.flags |= 1 << 18;
    fill_field(&mut editor, combo, &text("anything")).unwrap();
    let saved = editor.save_incremental().unwrap();
    assert!(saved.starts_with(&data));
    let mut again = Document::open(&saved).unwrap();
    assert_eq!(
        field(&again.form_fields(), "country").value,
        Some(text("anything"))
    );
}

// The fields of the corpus, as pypdf reads them (`forms.tsv`).
#[test]
fn fields_of_the_corpus_are_read_as_pypdf_reads_them() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let table = std::fs::read_to_string(root.join("crates/engine/tests/corpus/forms.tsv")).unwrap();
    let rows: Vec<Vec<&str>> = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .skip(1)
        .map(|line| line.split('\t').collect())
        .collect();
    let mut names: Vec<&str> = rows.iter().map(|row| row[0]).collect();
    names.dedup();
    assert!(names.len() >= 3);
    for name in names {
        let data = std::fs::read(root.join(name)).unwrap();
        let mut document = Document::open(&data).unwrap();
        let fields = document.form_fields();
        let expected: Vec<&Vec<&str>> = rows.iter().filter(|row| row[0] == name).collect();
        assert_eq!(fields.len(), expected.len(), "{name}");
        for (field, row) in fields.iter().zip(expected) {
            assert_eq!(field.name, row[1], "{name}");
            let kind = match row[2] {
                "/Tx" => FieldKind::Text,
                "/Ch" if field.flags & (1 << 17) != 0 => FieldKind::Combo,
                "/Ch" => FieldKind::List,
                "/Btn" if field.flags & (1 << 15) != 0 => FieldKind::Radio,
                "/Btn" if field.flags & (1 << 16) != 0 => FieldKind::PushButton,
                "/Btn" => FieldKind::Checkbox,
                other => panic!("{other}"),
            };
            assert_eq!(field.kind, kind, "{name} {}", field.name);
            assert_eq!(field.flags.to_string(), row[3], "{name} {}", field.name);
            let value = match &field.value {
                Some(FieldValue::Text(text))
                    if matches!(kind, FieldKind::Checkbox | FieldKind::Radio) =>
                {
                    format!("/{text}")
                }
                Some(FieldValue::Text(text)) => text.replace('\n', " "),
                Some(FieldValue::Choices(choices)) => choices.join("|"),
                None => String::new(),
            };
            assert_eq!(
                value,
                row.get(4).copied().unwrap_or(""),
                "{name} {}",
                field.name
            );
        }
    }
}
