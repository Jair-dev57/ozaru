//! Adding form fields to pages that exist.

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::document::{FieldKind, FieldValue, FormField, Rectangle, Widget, decode_text_string};
use crate::fonts::StandardFont;
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
use crate::writer::canvas::{Canvas, text_width};
use crate::writer::forms::{fill_field, text_string};
use crate::writer::newfield::{NewField, NewFieldKind};

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidField, 0)
}

fn name(text: &str) -> Object {
    Object::Name(Name::from(text))
}

fn color(rgb: [u8; 3]) -> Object {
    Object::Array(
        rgb.iter()
            .map(|&v| Object::Real(f64::from(v) / 255.0))
            .collect(),
    )
}

// A rectangle with its corners in order.
fn normalized(rect: [f64; 4]) -> [f64; 4] {
    [
        rect[0].min(rect[2]),
        rect[1].min(rect[3]),
        rect[0].max(rect[2]),
        rect[1].max(rect[3]),
    ]
}

fn check_rect(rect: [f64; 4]) -> Result<[f64; 4]> {
    if !rect.iter().all(|v| v.is_finite()) {
        return Err(Error::new(ErrorKind::NonFiniteNumber, 0));
    }
    let rect = normalized(rect);
    if rect[0] == rect[2] || rect[1] == rect[3] {
        return Err(Error::new(ErrorKind::InvalidBox, 0));
    }
    Ok(rect)
}

fn literal(text: &str) -> Object {
    Object::String(PdfString::new(
        text.as_bytes().to_vec(),
        StringFormat::Literal,
    ))
}

// The `/Opt` array: the text of each option, or a pair when the value it
// gives is another.
fn options_array(options: &[(String, String)]) -> Object {
    Object::Array(
        options
            .iter()
            .map(|(export, shown)| {
                if export == shown {
                    text_string(export)
                } else {
                    Object::Array(vec![text_string(export), text_string(shown)])
                }
            })
            .collect(),
    )
}

impl Editor<'_, '_> {
    /// Adds `field` to page `index` and gives its object (for radio buttons,
    /// that of the group). The field is added to the form, which is made if
    /// the document has none, and its widgets get appearances, so every
    /// reader shows them; fields can be filled with
    /// [`fill_field`](crate::writer::fill_field) or by the reader's user.
    ///
    /// It is a `PageNotFound` error past the last page, an `InvalidField` one
    /// for a name that is empty, has dots or is in the form already, and for
    /// a list or group without options or buttons, an `InvalidBox` one for a
    /// box without area, and an `InvalidFieldValue` one for a value the field
    /// can't hold. Text the standard font can't draw is an
    /// `UnsupportedCharacter` error.
    pub fn add_field(&mut self, index: usize, field: &NewField) -> Result<ObjectId> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        if field.name.is_empty() || field.name.contains('.') {
            return Err(invalid());
        }
        if !field.font_size.is_finite() || !field.border_width.is_finite() {
            return Err(Error::new(ErrorKind::NonFiniteNumber, 0));
        }
        let page_id = self.page_object(index)?;
        self.ensure_form()?;
        if self.form_has_field(&field.name)? {
            return Err(invalid());
        }
        let da = {
            let [r, g, b] = field.text_color.map(|v| f64::from(v) / 255.0);
            format!(
                "/Helv {} Tf {r:.3} {g:.3} {b:.3} rg",
                crate::writer::canvas::num(field.font_size.max(0.0))
            )
        };

        let id = match &field.kind {
            NewFieldKind::Radio { buttons, selected } => {
                self.add_radio(page_id, field, buttons, selected.as_deref(), &da)?
            }
            kind => {
                let rect = check_rect(field.rect)?;
                let mut dictionary = self.widget_entries(field, rect, page_id, &da);
                dictionary.insert(Name::from("T"), text_string(&field.name));
                let mut flags: i64 = i64::from(field.read_only) | i64::from(field.required) << 1;
                let mut options: Vec<(String, String)> = Vec::new();
                let mut max_length = None;
                let (field_kind, value) = match kind {
                    NewFieldKind::Text {
                        value,
                        multiline,
                        password,
                        max_length: max,
                    } => {
                        dictionary.insert(Name::from("FT"), name("Tx"));
                        flags |= i64::from(*multiline) << 12 | i64::from(*password) << 13;
                        if let Some(max) = max {
                            dictionary.insert(Name::from("MaxLen"), Object::Integer(*max as i64));
                            max_length = Some(*max);
                        }
                        (FieldKind::Text, FieldValue::Text(value.clone()))
                    }
                    NewFieldKind::Checkbox { checked } => {
                        dictionary.insert(Name::from("FT"), name("Btn"));
                        let state = if *checked { "Yes" } else { "Off" };
                        (FieldKind::Checkbox, FieldValue::Text(state.to_string()))
                    }
                    NewFieldKind::Combo {
                        options: given,
                        value,
                        editable,
                    } => {
                        dictionary.insert(Name::from("FT"), name("Ch"));
                        flags |= 1 << 17 | i64::from(*editable) << 18;
                        dictionary.insert(Name::from("Opt"), options_array(given));
                        options = given.clone();
                        (
                            FieldKind::Combo,
                            FieldValue::Text(value.clone().unwrap_or_default()),
                        )
                    }
                    NewFieldKind::List {
                        options: given,
                        selected,
                        multiple,
                    } => {
                        if given.is_empty() {
                            return Err(invalid());
                        }
                        dictionary.insert(Name::from("FT"), name("Ch"));
                        flags |= i64::from(*multiple) << 21;
                        dictionary.insert(Name::from("Opt"), options_array(given));
                        options = given.clone();
                        (FieldKind::List, FieldValue::Choices(selected.clone()))
                    }
                    NewFieldKind::Button { caption, uri } => {
                        dictionary.insert(Name::from("FT"), name("Btn"));
                        flags |= 1 << 16;
                        let mut mk = match dictionary.get("MK") {
                            Some(Object::Dictionary(mk)) => mk.clone(),
                            _ => Dictionary::new(),
                        };
                        mk.insert(Name::from("CA"), text_string(caption));
                        dictionary.insert(Name::from("MK"), Object::Dictionary(mk));
                        if let Some(uri) = uri {
                            let mut action = Dictionary::new();
                            action.insert(Name::from("S"), name("URI"));
                            action.insert(Name::from("URI"), literal(uri));
                            dictionary.insert(Name::from("A"), Object::Dictionary(action));
                        }
                        let form = self.button_appearance(field, caption, rect)?;
                        let mut ap = Dictionary::new();
                        ap.insert(Name::from("N"), Object::Reference(form));
                        dictionary.insert(Name::from("AP"), Object::Dictionary(ap));
                        (FieldKind::PushButton, FieldValue::Text(String::new()))
                    }
                    NewFieldKind::Signature => {
                        dictionary.insert(Name::from("FT"), name("Sig"));
                        (FieldKind::Signature, FieldValue::Text(String::new()))
                    }
                    NewFieldKind::Radio { .. } => unreachable!("radio buttons are added above"),
                };
                if flags != 0 {
                    dictionary.insert(Name::from("Ff"), Object::Integer(flags));
                }
                match &value {
                    FieldValue::Text(text)
                        if !text.is_empty() && field_kind != FieldKind::Checkbox =>
                    {
                        dictionary.insert(Name::from("V"), text_string(text));
                    }
                    FieldValue::Choices(choices) if !choices.is_empty() => {
                        dictionary.insert(
                            Name::from("V"),
                            Object::Array(choices.iter().map(|c| text_string(c)).collect()),
                        );
                    }
                    _ => {}
                }
                let id = self.add(Object::Dictionary(dictionary));
                let widget = Widget {
                    id,
                    rect: Rectangle::new(rect[0], rect[1], rect[2], rect[3]),
                    page: Some(index),
                    on_state: None,
                };
                self.register_field(page_id, id, id)?;
                // Fields that show a value get their appearance from it.
                if !matches!(field_kind, FieldKind::PushButton | FieldKind::Signature) {
                    let described = FormField {
                        id,
                        name: field.name.clone(),
                        kind: field_kind,
                        value: Some(value.clone()),
                        default: None,
                        flags: flags as u32,
                        options,
                        max_length,
                        appearance_string: Some(da.clone()),
                        quadding: field.align.min(2),
                        widgets: vec![widget.clone()],
                    };
                    fill_field(self, &described, &value)?;
                }
                id
            }
        };
        Ok(id)
    }

    // The entries a widget has whatever the field: its box, look and text.
    fn widget_entries(
        &mut self,
        field: &NewField,
        rect: [f64; 4],
        page_id: ObjectId,
        da: &str,
    ) -> Dictionary {
        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), name("Annot"));
        dictionary.insert(Name::from("Subtype"), name("Widget"));
        dictionary.insert(
            Name::from("Rect"),
            Object::Array(rect.iter().map(|&v| Object::Real(v)).collect()),
        );
        dictionary.insert(Name::from("F"), Object::Integer(4));
        dictionary.insert(Name::from("P"), Object::Reference(page_id));
        dictionary.insert(Name::from("DA"), literal(da));
        if field.align > 0 {
            dictionary.insert(
                Name::from("Q"),
                Object::Integer(i64::from(field.align.min(2))),
            );
        }
        let mut mk = Dictionary::new();
        if let Some(border) = field.border_color {
            mk.insert(Name::from("BC"), color(border));
        }
        if let Some(background) = field.background {
            mk.insert(Name::from("BG"), color(background));
        }
        if !mk.is_empty() {
            dictionary.insert(Name::from("MK"), Object::Dictionary(mk));
        }
        let mut border = Dictionary::new();
        border.insert(Name::from("W"), Object::Real(field.border_width.max(0.0)));
        border.insert(Name::from("S"), name("S"));
        dictionary.insert(Name::from("BS"), Object::Dictionary(border));
        if let Some(text) = &field.tooltip {
            dictionary.insert(Name::from("TU"), text_string(text));
        }
        dictionary
    }

    // The form of the document, made with its default appearance and fonts
    // if it has none, and with the fonts the new fields use.
    fn ensure_form(&mut self) -> Result<()> {
        let (id, mut form) = self.acro_form()?;
        if !form.contains_key("Fields") {
            form.insert(Name::from("Fields"), Object::Array(Vec::new()));
        }
        if !form.contains_key("DA") {
            form.insert(Name::from("DA"), literal("/Helv 0 Tf 0 g"));
        }
        let mut resources = self.dictionary_of(form.get("DR").cloned())?;
        let mut fonts = self.dictionary_of(resources.get("Font").cloned())?;
        let mut changed = false;
        for (key, base) in [("Helv", "Helvetica"), ("ZaDb", "ZapfDingbats")] {
            if !fonts.contains_key(key) {
                let mut font = Dictionary::new();
                font.insert(Name::from("Type"), name("Font"));
                font.insert(Name::from("Subtype"), name("Type1"));
                font.insert(Name::from("BaseFont"), name(base));
                if base == "Helvetica" {
                    font.insert(Name::from("Encoding"), name("WinAnsiEncoding"));
                }
                let font = self.add(Object::Dictionary(font));
                fonts.insert(Name::from(key), Object::Reference(font));
                changed = true;
            }
        }
        if changed || !form.contains_key("DR") {
            resources.insert(Name::from("Font"), Object::Dictionary(fonts));
            form.insert(Name::from("DR"), Object::Dictionary(resources));
        }
        self.save_form(id, form)
    }

    // The `/AcroForm` of the catalog and where it is: its object, or `None` if
    // the catalog holds it.
    fn acro_form(&mut self) -> Result<(Option<ObjectId>, Dictionary)> {
        let catalog = self.catalog_dictionary()?;
        Ok(match catalog.get("AcroForm").cloned() {
            Some(Object::Reference(id)) => match self.object(id)? {
                Object::Dictionary(form) => (Some(id), form),
                _ => (None, Dictionary::new()),
            },
            Some(Object::Dictionary(form)) => (None, form),
            _ => (None, Dictionary::new()),
        })
    }

    fn save_form(&mut self, id: Option<ObjectId>, form: Dictionary) -> Result<()> {
        match id {
            Some(id) => self.set(id, Object::Dictionary(form)),
            None => {
                let mut catalog = self.catalog_dictionary()?;
                catalog.insert(Name::from("AcroForm"), Object::Dictionary(form));
                self.set(self.catalog, Object::Dictionary(catalog));
            }
        }
        Ok(())
    }

    // The entries of the form's `/Fields`.
    fn form_fields_list(&mut self, form: &Dictionary) -> Result<Vec<Object>> {
        Ok(match form.get("Fields").cloned() {
            Some(Object::Array(items)) => items,
            Some(Object::Reference(list)) => match self.object(list)? {
                Object::Array(items) => items,
                _ => Vec::new(),
            },
            _ => Vec::new(),
        })
    }

    // Whether a field of the top of the form has this partial name.
    fn form_has_field(&mut self, wanted: &str) -> Result<bool> {
        let (_, form) = self.acro_form()?;
        for root in self.form_fields_list(&form)? {
            let dictionary = match root {
                Object::Dictionary(dictionary) => dictionary,
                Object::Reference(id) => match self.object(id)? {
                    Object::Dictionary(dictionary) => dictionary,
                    _ => continue,
                },
                _ => continue,
            };
            if let Some(Object::String(partial)) = dictionary.get("T")
                && decode_text_string(partial.as_bytes()) == wanted
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // Puts a field in the form, and its widget on the page.
    fn register_field(
        &mut self,
        page_id: ObjectId,
        field: ObjectId,
        widget: ObjectId,
    ) -> Result<()> {
        let (form_id, mut form) = self.acro_form()?;
        let mut fields = self.form_fields_list(&form)?;
        fields.push(Object::Reference(field));
        match form.get("Fields").cloned() {
            Some(Object::Reference(list)) => self.set(list, Object::Array(fields)),
            _ => {
                form.insert(Name::from("Fields"), Object::Array(fields));
                self.save_form(form_id, form)?;
            }
        }
        let mut annotations = self.annotation_list(page_id)?;
        annotations.push(Object::Reference(widget));
        self.set_annotation_list(page_id, annotations)
    }

    // A group of radio buttons: the field and its kids.
    fn add_radio(
        &mut self,
        page_id: ObjectId,
        field: &NewField,
        buttons: &[crate::writer::newfield::NewButton],
        selected: Option<&str>,
        da: &str,
    ) -> Result<ObjectId> {
        if buttons.is_empty() {
            return Err(invalid());
        }
        let mut seen: Vec<&str> = Vec::new();
        for button in buttons {
            if button.export.is_empty() || seen.contains(&button.export.as_str()) {
                return Err(invalid());
            }
            seen.push(&button.export);
            check_rect(button.rect)?;
        }
        if let Some(selected) = selected
            && !seen.contains(&selected)
        {
            return Err(Error::new(ErrorKind::InvalidFieldValue, 0));
        }
        let mut group = Dictionary::new();
        group.insert(Name::from("T"), text_string(&field.name));
        group.insert(Name::from("FT"), name("Btn"));
        // Radio buttons of which one must stay on.
        let flags = i64::from(field.read_only) | i64::from(field.required) << 1 | 1 << 15 | 1 << 14;
        group.insert(Name::from("Ff"), Object::Integer(flags));
        group.insert(Name::from("V"), name(selected.unwrap_or("Off")));
        let group_id = self.add(Object::Dictionary(group.clone()));

        let mut kids = Vec::new();
        let mut annotations = self.annotation_list(page_id)?;
        for button in buttons {
            let rect = check_rect(button.rect)?;
            let mut dictionary = self.widget_entries(field, rect, page_id, da);
            dictionary.insert(Name::from("Parent"), Object::Reference(group_id));
            let on = selected == Some(button.export.as_str());
            dictionary.insert(
                Name::from("AS"),
                name(if on { &button.export } else { "Off" }),
            );
            let (on_form, off_form) = self.radio_appearance(field, rect);
            let mut states = Dictionary::new();
            states.insert(
                Name::from(button.export.as_str()),
                Object::Reference(on_form),
            );
            states.insert(Name::from("Off"), Object::Reference(off_form));
            let mut ap = Dictionary::new();
            ap.insert(Name::from("N"), Object::Dictionary(states));
            dictionary.insert(Name::from("AP"), Object::Dictionary(ap));
            let id = self.add(Object::Dictionary(dictionary));
            kids.push(Object::Reference(id));
            annotations.push(Object::Reference(id));
        }
        group.insert(Name::from("Kids"), Object::Array(kids));
        self.set(group_id, Object::Dictionary(group));
        self.set_annotation_list(page_id, annotations)?;
        // The group is in the form; its kids are on the page.
        let (form_id, mut form) = self.acro_form()?;
        let mut fields = self.form_fields_list(&form)?;
        fields.push(Object::Reference(group_id));
        match form.get("Fields").cloned() {
            Some(Object::Reference(list)) => self.set(list, Object::Array(fields)),
            _ => {
                form.insert(Name::from("Fields"), Object::Array(fields));
                self.save_form(form_id, form)?;
            }
        }
        Ok(group_id)
    }

    // The drawings of a round button: on, with a dot, and off.
    fn radio_appearance(&mut self, field: &NewField, rect: [f64; 4]) -> (ObjectId, ObjectId) {
        let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
        let border = field.border_width.max(0.0);
        let make = |editor: &mut Self, on: bool| {
            let mut canvas = Canvas::new();
            if let Some(background) = field.background {
                canvas.set_fill_color(background);
                let _ = canvas.ellipse(0.0, 0.0, width, height);
                canvas.fill();
            }
            if let (Some(stroke), true) = (field.border_color, border > 0.0) {
                canvas.set_stroke_color(stroke);
                let _ = canvas.set_line_width(border);
                let half = border / 2.0;
                let _ = canvas.ellipse(half, half, width - half, height - half);
                canvas.stroke();
            }
            if on {
                canvas.set_fill_color(field.text_color);
                let (cx, cy) = (width / 2.0, height / 2.0);
                let radius = width.min(height) * 0.25;
                let _ = canvas.ellipse(cx - radius, cy - radius, cx + radius, cy + radius);
                canvas.fill();
            }
            editor.canvas_form(&canvas, [0.0, 0.0, width, height])
        };
        (make(self, true), make(self, false))
    }

    // The drawing of a push button: its background, border and caption.
    fn button_appearance(
        &mut self,
        field: &NewField,
        caption: &str,
        rect: [f64; 4],
    ) -> Result<ObjectId> {
        let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
        let border = field.border_width.max(0.0);
        let mut canvas = Canvas::new();
        canvas.set_fill_color(field.background.unwrap_or([210, 210, 210]));
        canvas.rectangle(0.0, 0.0, width, height)?;
        canvas.fill();
        if let (Some(stroke), true) = (field.border_color, border > 0.0) {
            canvas.set_stroke_color(stroke);
            canvas.set_line_width(border)?;
            canvas.rectangle(border / 2.0, border / 2.0, width - border, height - border)?;
            canvas.stroke();
        }
        let size = if field.font_size > 0.0 {
            field.font_size
        } else {
            (height * 0.5).clamp(6.0, 14.0)
        };
        let across = text_width(StandardFont::Helvetica, size, caption)?;
        canvas.set_fill_color(field.text_color);
        canvas.text(
            ((width - across) / 2.0).max(border),
            (height - size * 0.72) / 2.0,
            caption,
            StandardFont::Helvetica,
            size,
        )?;
        Ok(self.canvas_form(&canvas, [0.0, 0.0, width, height]))
    }
}
