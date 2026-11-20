//! Filling the fields of a form: the value of each field, and the appearance of
//! each widget, drawn here because readers show the appearance, not the value
//! (ISO 32000-2, sections 12.7.4 and 12.5.5).

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::document::{FieldKind, FieldValue, FormField, Widget};
use crate::fonts::{BaseEncoding, StandardFont};
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidFieldValue, 0)
}

/// Sets the value of `field` and draws its widgets again.
///
/// - **Text and drop-down lists** take `Text`. A text field can't take more
///   characters than its `/MaxLen`, and a list that isn't editable only takes
///   the value of one of its options.
/// - **Lists** take `Text` or `Choices`; several only if they allow it.
/// - **Check boxes and radio buttons** take `Text` with `Off` or the name of the
///   state that turns a widget on.
/// - Push buttons and signatures hold no value that can be set.
///
/// It is an `InvalidFieldValue` error when the field can't hold the value.
pub fn fill_field(
    editor: &mut Editor<'_, '_>,
    field: &FormField,
    value: &FieldValue,
) -> Result<()> {
    match field.kind {
        FieldKind::Text | FieldKind::Combo | FieldKind::List => fill_text(editor, field, value),
        FieldKind::Checkbox | FieldKind::Radio => fill_button(editor, field, value),
        _ => Err(invalid()),
    }
}

fn text_of(value: &FieldValue) -> Result<&str> {
    match value {
        FieldValue::Text(text) => Ok(text),
        FieldValue::Choices(_) => Err(invalid()),
    }
}

// A PDF text string: PDFDocEncoding when the text is in it, UTF-16 otherwise.
fn text_string(text: &str) -> Object {
    let simple = text
        .chars()
        .all(|c| (' '..='~').contains(&c) || matches!(c, '\n' | '\r' | '\t'));
    let bytes = if simple {
        text.as_bytes().to_vec()
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        bytes
    };
    Object::String(PdfString::new(bytes, StringFormat::Literal))
}

fn dictionary_of(object: Object) -> Result<Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Ok(dictionary),
        Object::Stream(stream) => Ok(stream.dictionary().clone()),
        _ => Err(invalid()),
    }
}

fn fill_text(editor: &mut Editor<'_, '_>, field: &FormField, value: &FieldValue) -> Result<()> {
    let mut dictionary = dictionary_of(editor.object(field.id)?)?;

    let shown: Vec<String> = match (field.kind, value) {
        (FieldKind::List, FieldValue::Choices(choices)) => {
            if choices.len() > 1 && !field.is_multi_select() {
                return Err(invalid());
            }
            for choice in choices {
                if !field.options.iter().any(|(export, _)| export == choice) {
                    return Err(invalid());
                }
            }
            let objects = choices.iter().map(|c| text_string(c)).collect();
            dictionary.insert(Name::from("V"), Object::Array(objects));
            choices.clone()
        }
        _ => {
            let text = text_of(value)?;
            if field.kind == FieldKind::Text
                && field
                    .max_length
                    .is_some_and(|max| text.chars().count() > max)
            {
                return Err(invalid());
            }
            let is_option = field.options.iter().any(|(export, _)| export == text);
            if matches!(field.kind, FieldKind::List) && !is_option && !text.is_empty() {
                return Err(invalid());
            }
            if field.kind == FieldKind::Combo
                && !field.is_editable()
                && !is_option
                && !text.is_empty()
            {
                return Err(invalid());
            }
            dictionary.insert(Name::from("V"), text_string(text));
            // A choice shows the text of its option.
            let display = field
                .options
                .iter()
                .find(|(export, _)| export == text)
                .map_or(text.to_string(), |(_, shown)| shown.clone());
            vec![display]
        }
    };
    // Their selected options are what an `/I` array of indexes says.
    dictionary.remove("I");
    editor.set(field.id, Object::Dictionary(dictionary));
    for widget in &field.widgets {
        draw_text_widget(editor, field, widget, &shown, value)?;
    }
    Ok(())
}

fn fill_button(editor: &mut Editor<'_, '_>, field: &FormField, value: &FieldValue) -> Result<()> {
    let state = text_of(value)?;
    let known: Vec<&str> = field
        .widgets
        .iter()
        .filter_map(|widget| widget.on_state.as_deref())
        .collect();
    // A check box without drawings gets a `Yes` state of its own.
    if state != "Off" && !known.contains(&state) && !(known.is_empty() && state == "Yes") {
        return Err(invalid());
    }
    let mut dictionary = dictionary_of(editor.object(field.id)?)?;
    dictionary.insert(Name::from("V"), Object::Name(Name::from(state)));
    editor.set(field.id, Object::Dictionary(dictionary));
    for widget in &field.widgets {
        let mut annotation = dictionary_of(editor.object(widget.id)?)?;
        let on = widget.on_state.as_deref().unwrap_or("Yes");
        if widget.on_state.is_none() {
            let appearance = check_appearance(editor, widget)?;
            annotation.insert(Name::from("AP"), appearance);
        }
        let current = if state == on { on } else { "Off" };
        annotation.insert(Name::from("AS"), Object::Name(Name::from(current)));
        if widget.id == field.id {
            annotation.insert(Name::from("V"), Object::Name(Name::from(state)));
        }
        editor.set(widget.id, Object::Dictionary(annotation));
    }
    Ok(())
}

// The appearances of a check box that has none: empty for `Off`, with a tick
// for `Yes`.
fn check_appearance(editor: &mut Editor<'_, '_>, widget: &Widget) -> Result<Object> {
    let (width, height) = (widget.rect.width(), widget.rect.height());
    let font = {
        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), Object::Name(Name::from("Font")));
        dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("Type1")));
        dictionary.insert(
            Name::from("BaseFont"),
            Object::Name(Name::from("ZapfDingbats")),
        );
        editor.add(Object::Dictionary(dictionary))
    };
    let size = (height.min(width) * 0.8).max(1.0);
    let frame = format!(
        "0.5 w 0.25 0.25 {:.3} {:.3} re S\n",
        width - 0.5,
        height - 0.5
    );
    let tick = format!(
        "{frame}BT /ZaDb {size:.2} Tf 0 g {:.3} {:.3} Td (4) Tj ET\n",
        (width - size * 0.846) / 2.0,
        (height - size * 0.7) / 2.0
    );
    let mut states = Dictionary::new();
    for (name, content) in [("Off", frame), ("Yes", tick)] {
        let stream = form(editor, width, height, &content, Some(("ZaDb", font)));
        states.insert(Name::from(name), Object::Reference(stream));
    }
    let mut appearance = Dictionary::new();
    appearance.insert(Name::from("N"), Object::Dictionary(states));
    Ok(Object::Dictionary(appearance))
}

// A form XObject `width` by `height` with `content`, and a font resource.
fn form(
    editor: &mut Editor<'_, '_>,
    width: f64,
    height: f64,
    content: &str,
    font: Option<(&str, ObjectId)>,
) -> ObjectId {
    let mut dictionary = Dictionary::new();
    dictionary.insert(Name::from("Type"), Object::Name(Name::from("XObject")));
    dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("Form")));
    dictionary.insert(
        Name::from("BBox"),
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(width),
            Object::Real(height),
        ]),
    );
    if let Some((name, id)) = font {
        let mut fonts = Dictionary::new();
        fonts.insert(Name::from(name), Object::Reference(id));
        let mut resources = Dictionary::new();
        resources.insert(Name::from("Font"), Object::Dictionary(fonts));
        dictionary.insert(Name::from("Resources"), Object::Dictionary(resources));
    }
    editor.add_stream(dictionary, content.as_bytes().to_vec())
}

// What the default appearance says: the font's resource name, its size (0 for
// automatic) and the rest of the operators, which set the color.
struct DefaultAppearance {
    font: String,
    size: f64,
    before: Vec<String>,
    after: Vec<String>,
}

fn parse_default_appearance(text: &str) -> DefaultAppearance {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    match tokens.iter().position(|t| *t == "Tf") {
        Some(at) if at >= 2 => DefaultAppearance {
            font: tokens[at - 2].trim_start_matches('/').to_string(),
            size: tokens[at - 1].parse().unwrap_or(0.0),
            before: tokens[..at - 2].iter().map(|t| t.to_string()).collect(),
            after: tokens[at + 1..].iter().map(|t| t.to_string()).collect(),
        },
        _ => DefaultAppearance {
            font: "Helv".to_string(),
            size: 0.0,
            before: Vec::new(),
            after: vec!["0".into(), "g".into()],
        },
    }
}

// How wide the characters of a font are, and its ascent and descent.
struct Metrics {
    first: i64,
    widths: Option<Vec<f64>>,
    standard: StandardFont,
    ascent: f64,
    descent: f64,
}

impl Metrics {
    fn of(font: &Dictionary) -> Self {
        let standard = font
            .get("BaseFont")
            .and_then(Object::as_name)
            .and_then(|name| StandardFont::from_base_font(name.as_bytes()))
            .unwrap_or(StandardFont::Helvetica);
        let widths = font.get("Widths").and_then(Object::as_array).map(|widths| {
            widths
                .iter()
                .map(|w| w.as_number().unwrap_or(0.0) / 1000.0)
                .collect()
        });
        let (ascent, descent) = standard.ascender_descender();
        Self {
            first: font
                .get("FirstChar")
                .and_then(Object::as_integer)
                .unwrap_or(0),
            widths,
            standard,
            ascent: ascent / 1000.0,
            descent: descent / 1000.0,
        }
    }

    fn width(&self, code: u8) -> f64 {
        if let Some(widths) = &self.widths
            && let Some(width) = usize::try_from(i64::from(code) - self.first)
                .ok()
                .and_then(|i| widths.get(i))
        {
            return *width;
        }
        BaseEncoding::WinAnsi
            .glyph_name(code)
            .and_then(|glyph| self.standard.glyph_width(glyph.as_bytes()))
            .unwrap_or(500.0)
            / 1000.0
    }
}

// The byte of the WinAnsi encoding for a character, `?` if it has none.
fn win_ansi(c: char) -> u8 {
    match c as u32 {
        0x20..=0x7E | 0xA0..=0xFF => c as u8,
        0x20AC => 0x80,
        0x201A => 0x82,
        0x0192 => 0x83,
        0x201E => 0x84,
        0x2026 => 0x85,
        0x2020 => 0x86,
        0x2021 => 0x87,
        0x02C6 => 0x88,
        0x2030 => 0x89,
        0x0160 => 0x8A,
        0x2039 => 0x8B,
        0x0152 => 0x8C,
        0x017D => 0x8E,
        0x2018 => 0x91,
        0x2019 => 0x92,
        0x201C => 0x93,
        0x201D => 0x94,
        0x2022 => 0x95,
        0x2013 => 0x96,
        0x2014 => 0x97,
        0x02DC => 0x98,
        0x2122 => 0x99,
        0x0161 => 0x9A,
        0x203A => 0x9B,
        0x0153 => 0x9C,
        0x017E => 0x9E,
        0x0178 => 0x9F,
        _ => b'?',
    }
}

fn escaped(bytes: &[u8]) -> String {
    let mut out = String::from("(");
    for &byte in bytes {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7E => out.push(byte as char),
            other => out.push_str(&format!("\\{other:03o}")),
        }
    }
    out.push(')');
    out
}

fn color_operators(object: Option<&Object>, stroke: bool) -> Option<String> {
    let numbers: Vec<f64> = object?
        .as_array()?
        .iter()
        .filter_map(Object::as_number)
        .collect();
    let operator = match (numbers.len(), stroke) {
        (1, false) => "g",
        (1, true) => "G",
        (3, false) => "rg",
        (3, true) => "RG",
        (4, false) => "k",
        (4, true) => "K",
        _ => return None,
    };
    let numbers: Vec<String> = numbers.iter().map(|n| format!("{n:.3}")).collect();
    Some(format!("{} {operator}\n", numbers.join(" ")))
}

// Draws the widget: its background and border from `/MK`, and `lines` of text.
fn draw_text_widget(
    editor: &mut Editor<'_, '_>,
    field: &FormField,
    widget: &Widget,
    lines: &[String],
    value: &FieldValue,
) -> Result<()> {
    let mut annotation = dictionary_of(editor.object(widget.id)?)?;
    let (width, height) = (widget.rect.width(), widget.rect.height());
    let default = parse_default_appearance(field.appearance_string.as_deref().unwrap_or(""));

    // The font: the form's resource of that name, or Helvetica.
    let form_fonts = acro_form_fonts(editor)?;
    let font_ref = form_fonts
        .iter()
        .find(|(name, _)| *name == default.font)
        .map(|(_, id)| *id);
    let font_id = match font_ref {
        Some(id) => id,
        None => {
            let mut dictionary = Dictionary::new();
            dictionary.insert(Name::from("Type"), Object::Name(Name::from("Font")));
            dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("Type1")));
            dictionary.insert(
                Name::from("BaseFont"),
                Object::Name(Name::from("Helvetica")),
            );
            dictionary.insert(
                Name::from("Encoding"),
                Object::Name(Name::from("WinAnsiEncoding")),
            );
            editor.add(Object::Dictionary(dictionary))
        }
    };
    let metrics = Metrics::of(&dictionary_of(editor.object(font_id)?).unwrap_or_default());

    let mk = annotation
        .get("MK")
        .cloned()
        .and_then(|mk| dictionary_of(mk).ok())
        .unwrap_or_default();
    let border_width = annotation
        .get("BS")
        .and_then(Object::as_dictionary)
        .and_then(|bs| bs.get("W"))
        .and_then(Object::as_number)
        .unwrap_or(if mk.contains_key("BC") { 1.0 } else { 0.0 });
    let mut content = String::new();
    if let Some(background) = color_operators(mk.get("BG"), false) {
        content.push_str(&format!(
            "q {background}0 0 {width:.3} {height:.3} re f Q\n"
        ));
    }
    if border_width > 0.0
        && let Some(border) = color_operators(mk.get("BC"), true)
    {
        let half = border_width / 2.0;
        content.push_str(&format!(
            "q {border}{border_width:.3} w {half:.3} {half:.3} {:.3} {:.3} re S Q\n",
            width - border_width,
            height - border_width
        ));
    }

    let padding = border_width.max(1.0) + 1.0;
    let multiline = field.is_multiline();
    let is_list = field.kind == FieldKind::List;
    let size = if default.size > 0.0 {
        default.size
    } else if multiline || is_list {
        12.0
    } else {
        (height - 2.0 * padding).clamp(4.0, 12.0)
    };
    let inner = (width - 2.0 * padding).max(1.0);

    // The lines to draw: wrapped for a multiline field, one per option for
    // a list, and dots for a password.
    let mut shown: Vec<Vec<u8>> = Vec::new();
    let encode = |text: &str| -> Vec<u8> {
        text.chars()
            .map(|c| {
                if field.is_password() {
                    b'*'
                } else {
                    win_ansi(c)
                }
            })
            .collect()
    };
    let measure =
        |bytes: &[u8]| -> f64 { bytes.iter().map(|&b| metrics.width(b)).sum::<f64>() * size };
    let selected_lines: Vec<usize>;
    if is_list {
        shown = field.options.iter().map(|(_, text)| encode(text)).collect();
        selected_lines = match value {
            FieldValue::Choices(choices) => field
                .options
                .iter()
                .enumerate()
                .filter(|(_, (export, _))| choices.contains(export))
                .map(|(i, _)| i)
                .collect(),
            FieldValue::Text(text) => field
                .options
                .iter()
                .enumerate()
                .filter(|(_, (export, _))| export == text)
                .map(|(i, _)| i)
                .collect(),
        };
    } else {
        selected_lines = Vec::new();
        let text = lines.first().map_or("", String::as_str);
        if multiline {
            for paragraph in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
                wrap(&encode(paragraph), inner, &measure, &mut shown);
            }
        } else {
            shown.push(encode(&text.replace(['\n', '\r'], " ")));
        }
    }

    let leading = size * (metrics.ascent - metrics.descent).max(1.0) * 1.0;
    let mut text = String::new();
    text.push_str(&format!(
        "/Tx BMC\nq\n{padding:.3} {padding:.3} {inner:.3} {:.3} re W n\n",
        (height - 2.0 * padding).max(1.0)
    ));
    for line in &selected_lines {
        let top = height - padding - leading * *line as f64;
        text.push_str(&format!(
            "q 0.600 0.750 0.850 rg {padding:.3} {:.3} {inner:.3} {leading:.3} re f Q\n",
            top - leading
        ));
    }
    text.push_str("BT\n");
    let mut operators = default.before.clone();
    operators.push(format!("/{}", default.font));
    operators.push(format!("{size:.2}"));
    operators.push("Tf".to_string());
    operators.extend(default.after.iter().cloned());
    text.push_str(&operators.join(" "));
    text.push('\n');
    let mut previous = (0.0, 0.0);
    let one_line_baseline =
        (height - (metrics.ascent - metrics.descent) * size) / 2.0 - metrics.descent * size;
    for (index, line) in shown.iter().enumerate() {
        let advance = measure(line);
        let x = match field.quadding {
            1 => padding + (inner - advance) / 2.0,
            2 => padding + inner - advance,
            _ => padding,
        }
        .max(padding.min(0.0));
        let y = if multiline || is_list {
            height - padding - metrics.ascent * size - leading * index as f64
        } else {
            one_line_baseline
        };
        text.push_str(&format!(
            "{:.3} {:.3} Td\n{} Tj\n",
            x - previous.0,
            y - previous.1,
            escaped(line)
        ));
        previous = (x, y);
    }
    text.push_str("ET\nQ\nEMC\n");
    content.push_str(&text);

    let stream = form(
        editor,
        width,
        height,
        &content,
        Some((&default.font, font_id)),
    );
    let mut appearance = Dictionary::new();
    appearance.insert(Name::from("N"), Object::Reference(stream));
    annotation.insert(Name::from("AP"), Object::Dictionary(appearance));
    editor.set(widget.id, Object::Dictionary(annotation));
    Ok(())
}

// Splits `text` into lines at the spaces, each as wide as `limit` at most
// (a word wider than that is split where it overflows).
fn wrap(text: &[u8], limit: f64, measure: &dyn Fn(&[u8]) -> f64, out: &mut Vec<Vec<u8>>) {
    let mut line: Vec<u8> = Vec::new();
    for word in text.split_inclusive(|&b| b == b' ') {
        let mut candidate = line.clone();
        candidate.extend_from_slice(word);
        let trimmed: &[u8] = candidate.strip_suffix(b" ").unwrap_or(&candidate);
        if measure(trimmed) <= limit || line.is_empty() && word.len() <= 1 {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            out.push(std::mem::take(&mut line));
        }
        // A word that is longer than the line, cut where it overflows.
        let mut piece: Vec<u8> = Vec::new();
        for &byte in word {
            piece.push(byte);
            if measure(&piece) > limit && piece.len() > 1 {
                let last = piece.pop().expect("more than one byte");
                out.push(std::mem::take(&mut piece));
                piece.push(last);
            }
        }
        line = piece;
    }
    out.push(line);
}

// The fonts of the form's default resources, `/AcroForm /DR /Font`, by name.
fn acro_form_fonts(editor: &mut Editor<'_, '_>) -> Result<Vec<(String, ObjectId)>> {
    let catalog = dictionary_of(editor.object(editor.catalog())?)?;
    let form = match catalog.get("AcroForm") {
        Some(Object::Reference(id)) => dictionary_of(editor.object(*id)?).unwrap_or_default(),
        Some(Object::Dictionary(dictionary)) => dictionary.clone(),
        _ => return Ok(Vec::new()),
    };
    let resources = match form.get("DR") {
        Some(Object::Reference(id)) => dictionary_of(editor.object(*id)?).unwrap_or_default(),
        Some(Object::Dictionary(dictionary)) => dictionary.clone(),
        _ => return Ok(Vec::new()),
    };
    let fonts = match resources.get("Font") {
        Some(Object::Reference(id)) => dictionary_of(editor.object(*id)?).unwrap_or_default(),
        Some(Object::Dictionary(dictionary)) => dictionary.clone(),
        _ => return Ok(Vec::new()),
    };
    Ok(fonts
        .iter()
        .filter_map(|(name, font)| {
            Some((
                String::from_utf8_lossy(name.as_bytes()).into_owned(),
                font.as_reference()?,
            ))
        })
        .collect())
}
