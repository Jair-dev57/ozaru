//! Form fields to add to a page (ISO 32000-2, section 12.7), ready for
//! [`Editor::add_field`](super::Editor::add_field).

/// One button of a group of radio buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct NewButton {
    /// `[left, bottom, right, top]` in default user space.
    pub rect: [f64; 4],
    /// The value the field has when this button is on.
    pub export: String,
}

/// What a field is.
#[derive(Debug, Clone, PartialEq)]
pub enum NewFieldKind {
    /// A box to type text in.
    Text {
        value: String,
        multiline: bool,
        password: bool,
        max_length: Option<usize>,
    },
    /// A box that is on or off.
    Checkbox { checked: bool },
    /// Buttons of which one is on: they are one field, with the value of the
    /// one that is on. The field's own `rect` is ignored.
    Radio {
        buttons: Vec<NewButton>,
        selected: Option<String>,
    },
    /// A drop-down list. The options are each the value they give and the
    /// text they show.
    Combo {
        options: Vec<(String, String)>,
        value: Option<String>,
        editable: bool,
    },
    /// A list of options to choose from.
    List {
        options: Vec<(String, String)>,
        selected: Vec<String>,
        multiple: bool,
    },
    /// A button with a caption, that opens `uri` if it has one.
    Button {
        caption: String,
        uri: Option<String>,
    },
    /// A place to sign, left empty.
    Signature,
}

/// A form field to add to a page.
#[derive(Debug, Clone, PartialEq)]
pub struct NewField {
    /// The field's name, which must be new in the form and have no dots.
    pub name: String,
    /// `[left, bottom, right, top]` in default user space.
    pub rect: [f64; 4],
    pub kind: NewFieldKind,
    /// The size of the text in points; 0 fits it to the box.
    pub font_size: f64,
    /// Red, green and blue from 0 to 255.
    pub text_color: [u8; 3],
    pub border_color: Option<[u8; 3]>,
    pub background: Option<[u8; 3]>,
    pub border_width: f64,
    /// 0 left, 1 centered, 2 right.
    pub align: u8,
    pub read_only: bool,
    pub required: bool,
    /// The text shown when the pointer rests on the field.
    pub tooltip: Option<String>,
}

impl NewField {
    /// A field with the usual look: black text, a thin black border, no
    /// background.
    pub fn new(name: impl Into<String>, rect: [f64; 4], kind: NewFieldKind) -> Self {
        Self {
            name: name.into(),
            rect,
            kind,
            font_size: 0.0,
            text_color: [0, 0, 0],
            border_color: Some([0, 0, 0]),
            background: None,
            border_width: 1.0,
            align: 0,
            read_only: false,
            required: false,
            tooltip: None,
        }
    }
}
