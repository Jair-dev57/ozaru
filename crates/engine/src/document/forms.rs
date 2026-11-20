use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{Rectangle, decode_text_string};
use crate::syntax::{Dictionary, Object, ObjectId};
use crate::xref::Resolver;

/// What a field of an interactive form holds (ISO 32000-2, section 12.7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// A line or box of text.
    Text,
    /// A box that is on or off.
    Checkbox,
    /// One button of a group of which one is on.
    Radio,
    /// A button that does something, and holds no value.
    PushButton,
    /// A drop-down list, which may allow typing.
    Combo,
    /// A list of options to choose from.
    List,
    /// A digital signature.
    Signature,
    /// A field whose type is missing or unknown.
    Unknown,
}

/// The value of a field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// The text of a text field or a choice; the state of a check box or radio
    /// button, which is `Off` or the name of its on state.
    Text(String),
    /// The options chosen in a list that allows several.
    Choices(Vec<String>),
}

/// A field of the form of a document: a terminal field of the tree that the
/// `/AcroForm` entry of the catalog leads to, with what it inherits.
#[derive(Debug, Clone)]
pub struct FormField {
    /// The field's object.
    pub id: ObjectId,
    /// The names of its ancestors and its own, joined by dots.
    pub name: String,
    pub kind: FieldKind,
    /// `/V`, or `None` if it has none.
    pub value: Option<FieldValue>,
    /// `/DV`, the value it goes back to when reset.
    pub default: Option<FieldValue>,
    /// `/Ff`, the flags of the field.
    pub flags: u32,
    /// The options of a choice, each as the value it gives and the text it
    /// shows, which are the same unless the PDF says otherwise.
    pub options: Vec<(String, String)>,
    /// `/MaxLen`, the most characters of a text field.
    pub max_length: Option<usize>,
    /// `/DA`, the default appearance: the operators that set font and color.
    pub appearance_string: Option<String>,
    /// `/Q`: 0 left, 1 centered, 2 right.
    pub quadding: u8,
    /// The widgets that show the field: annotations of its pages.
    pub widgets: Vec<Widget>,
}

/// One place where a field is shown.
#[derive(Debug, Clone)]
pub struct Widget {
    pub id: ObjectId,
    pub rect: Rectangle,
    /// The page it is on, from 0, if a page lists it.
    pub page: Option<usize>,
    /// The state that turns it on, for check boxes and radio buttons.
    pub on_state: Option<String>,
}

impl FormField {
    pub fn is_read_only(&self) -> bool {
        self.flags & 1 != 0
    }

    pub fn is_required(&self) -> bool {
        self.flags & 2 != 0
    }

    /// Whether a text field has several lines.
    pub fn is_multiline(&self) -> bool {
        self.kind == FieldKind::Text && self.flags & (1 << 12) != 0
    }

    /// Whether a text field shows its text as dots, as for passwords.
    pub fn is_password(&self) -> bool {
        self.kind == FieldKind::Text && self.flags & (1 << 13) != 0
    }

    /// Whether a list allows choosing several options.
    pub fn is_multi_select(&self) -> bool {
        self.kind == FieldKind::List && self.flags & (1 << 21) != 0
    }

    /// Whether a drop-down list allows typing other values.
    pub fn is_editable(&self) -> bool {
        self.kind == FieldKind::Combo && self.flags & (1 << 18) != 0
    }
}

// The most fields and the deepest tree read.
const MAX_FIELDS: usize = 100_000;
const MAX_DEPTH: usize = 64;

// What a field passes to its descendants (section 12.7.4.1).
#[derive(Clone, Default)]
struct Inherited {
    name: String,
    kind: Option<Vec<u8>>,
    flags: Option<u32>,
    value: Option<Object>,
    default: Option<Object>,
    options: Option<Object>,
    max_length: Option<i64>,
    appearance: Option<Object>,
    quadding: Option<i64>,
}

fn resolved(resolver: &mut Resolver<'_>, object: &Object) -> Arc<Object> {
    match object {
        Object::Reference(id) => resolver
            .resolve(*id)
            .unwrap_or_else(|_| Arc::new(Object::Null)),
        other => Arc::new(other.clone()),
    }
}

/// The fields of the form that `/AcroForm` of `catalog` lists, with their
/// widgets, in the order of the tree. `pages` holds the annotations of each
/// page, to say which page a widget is on.
///
/// A file without a form has no fields. Cycles in the tree and fields that
/// aren't dictionaries are skipped.
pub fn read_form_fields(
    resolver: &mut Resolver<'_>,
    catalog: &Dictionary,
    page_annotations: &[Vec<ObjectId>],
) -> Vec<FormField> {
    let Some(form) = catalog.get("AcroForm") else {
        return Vec::new();
    };
    let form = resolved(resolver, form);
    let Some(form) = form.as_dictionary() else {
        return Vec::new();
    };
    let Some(roots) = form.get("Fields") else {
        return Vec::new();
    };
    let roots = resolved(resolver, roots);
    let Some(roots) = roots.as_array() else {
        return Vec::new();
    };
    // The page of each widget, from the lists of annotations.
    let mut pages = HashMap::new();
    for (index, list) in page_annotations.iter().enumerate() {
        for id in list {
            pages.entry(*id).or_insert(index);
        }
    }
    // The form's own default appearance is what fields without one use.
    let start = Inherited {
        appearance: form.get("DA").cloned(),
        quadding: form
            .get("Q")
            .and_then(|q| resolved(resolver, q).as_integer()),
        ..Inherited::default()
    };
    let mut reader = Reader {
        fields: Vec::new(),
        visited: HashSet::new(),
        pages,
    };
    for root in roots {
        reader.visit(resolver, root, &start, 0);
    }
    reader.fields
}

struct Reader {
    fields: Vec<FormField>,
    visited: HashSet<ObjectId>,
    pages: HashMap<ObjectId, usize>,
}

impl Reader {
    fn visit(
        &mut self,
        resolver: &mut Resolver<'_>,
        node: &Object,
        parent: &Inherited,
        depth: usize,
    ) {
        if depth > MAX_DEPTH || self.fields.len() >= MAX_FIELDS {
            return;
        }
        let id = node.as_reference();
        if let Some(id) = id
            && !self.visited.insert(id)
        {
            return;
        }
        let object = resolved(resolver, node);
        let Some(dictionary) = object.as_dictionary() else {
            return;
        };
        let inherited = self.inherit(resolver, dictionary, parent);

        // Children with a partial name are fields; the others are widgets.
        let mut widgets = Vec::new();
        let mut children = Vec::new();
        match dictionary.get("Kids") {
            Some(kids) => {
                let kids = resolved(resolver, kids);
                for kid in kids.as_array().unwrap_or(&[]).iter().take(MAX_FIELDS) {
                    let kid_object = resolved(resolver, kid);
                    match kid_object.as_dictionary() {
                        Some(d) if d.contains_key("T") => children.push(kid.clone()),
                        Some(_) => {
                            if let Some(widget) = kid.as_reference() {
                                widgets.push(widget);
                            }
                        }
                        None => {}
                    }
                }
            }
            None => {
                if let Some(id) = id {
                    widgets.push(id);
                }
            }
        }
        for child in &children {
            self.visit(resolver, child, &inherited, depth + 1);
        }
        // A node with children that are fields is not a field itself, unless
        // widgets belong to it too.
        let Some(id) = id else { return };
        if !children.is_empty() && widgets.is_empty() {
            return;
        }
        let field = self.field(resolver, id, &inherited, &widgets);
        self.fields.push(field);
    }

    // What `dictionary` sets over what its ancestors passed down.
    fn inherit(
        &self,
        resolver: &mut Resolver<'_>,
        dictionary: &Dictionary,
        parent: &Inherited,
    ) -> Inherited {
        let mut inherited = parent.clone();
        if let Some(partial) = dictionary.get("T") {
            let partial = resolved(resolver, partial);
            if let Some(text) = partial.as_string() {
                let text = decode_text_string(text.as_bytes());
                if !inherited.name.is_empty() {
                    inherited.name.push('.');
                }
                inherited.name.push_str(&text);
            }
        }
        if let Some(kind) = dictionary.get("FT")
            && let Some(name) = resolved(resolver, kind).as_name()
        {
            inherited.kind = Some(name.as_bytes().to_vec());
        }
        if let Some(flags) = dictionary.get("Ff")
            && let Some(flags) = resolved(resolver, flags).as_integer()
        {
            inherited.flags = Some(flags as u32);
        }
        if let Some(value) = dictionary.get("V") {
            inherited.value = Some(value.clone());
        }
        if let Some(value) = dictionary.get("DV") {
            inherited.default = Some(value.clone());
        }
        if let Some(options) = dictionary.get("Opt") {
            inherited.options = Some(options.clone());
        }
        if let Some(length) = dictionary.get("MaxLen") {
            inherited.max_length = resolved(resolver, length).as_integer();
        }
        if let Some(appearance) = dictionary.get("DA") {
            inherited.appearance = Some(appearance.clone());
        }
        if let Some(quadding) = dictionary.get("Q") {
            inherited.quadding = resolved(resolver, quadding).as_integer();
        }
        inherited
    }

    fn field(
        &self,
        resolver: &mut Resolver<'_>,
        id: ObjectId,
        inherited: &Inherited,
        widgets: &[ObjectId],
    ) -> FormField {
        let flags = inherited.flags.unwrap_or(0);
        let kind = match inherited.kind.as_deref() {
            Some(b"Tx") => FieldKind::Text,
            Some(b"Btn") if flags & (1 << 16) != 0 => FieldKind::PushButton,
            Some(b"Btn") if flags & (1 << 15) != 0 => FieldKind::Radio,
            Some(b"Btn") => FieldKind::Checkbox,
            Some(b"Ch") if flags & (1 << 17) != 0 => FieldKind::Combo,
            Some(b"Ch") => FieldKind::List,
            Some(b"Sig") => FieldKind::Signature,
            _ => FieldKind::Unknown,
        };
        let value = inherited
            .value
            .as_ref()
            .and_then(|value| field_value(resolver, value));
        let default = inherited
            .default
            .as_ref()
            .and_then(|value| field_value(resolver, value));
        let options = inherited
            .options
            .as_ref()
            .map(|options| read_options(resolver, options))
            .unwrap_or_default();
        let appearance_string = inherited.appearance.as_ref().and_then(|da| {
            let da = resolved(resolver, da);
            Some(String::from_utf8_lossy(da.as_string()?.as_bytes()).into_owned())
        });
        let widgets = widgets
            .iter()
            .filter_map(|widget| self.widget(resolver, *widget))
            .collect();
        FormField {
            id,
            name: inherited.name.clone(),
            kind,
            value,
            default,
            flags,
            options,
            max_length: inherited
                .max_length
                .and_then(|length| usize::try_from(length).ok())
                .filter(|length| *length > 0),
            appearance_string,
            quadding: inherited.quadding.map_or(0, |q| q.clamp(0, 2) as u8),
            widgets,
        }
    }

    fn widget(&self, resolver: &mut Resolver<'_>, id: ObjectId) -> Option<Widget> {
        let object = resolver.resolve(id).ok()?;
        let dictionary = object.as_dictionary()?;
        let rect = resolved(resolver, dictionary.get("Rect")?);
        let numbers: Vec<f64> = rect
            .as_array()?
            .iter()
            .map_while(|n| resolved(resolver, n).as_number())
            .collect();
        let &[x1, y1, x2, y2] = numbers.as_slice() else {
            return None;
        };
        // The on state is the name in `/AP /N` that isn't `Off`.
        let mut on_state = None;
        if let Some(appearance) = dictionary.get("AP") {
            let appearance = resolved(resolver, appearance);
            if let Some(normal) = appearance.as_dictionary().and_then(|ap| ap.get("N")) {
                let normal = resolved(resolver, normal);
                if let Some(states) = normal.as_dictionary() {
                    on_state = states
                        .iter()
                        .map(|(name, _)| String::from_utf8_lossy(name.as_bytes()).into_owned())
                        .find(|name| name != "Off");
                }
            }
        }
        Some(Widget {
            id,
            rect: Rectangle::new(x1, y1, x2, y2),
            page: self.pages.get(&id).copied(),
            on_state,
        })
    }
}

fn field_value(resolver: &mut Resolver<'_>, value: &Object) -> Option<FieldValue> {
    let value_reference = value.as_reference();
    let value = resolved(resolver, value);
    match &*value {
        Object::String(string) => Some(FieldValue::Text(decode_text_string(string.as_bytes()))),
        Object::Name(name) => Some(FieldValue::Text(
            String::from_utf8_lossy(name.as_bytes()).into_owned(),
        )),
        Object::Array(items) => {
            let choices: Vec<String> = items
                .iter()
                .filter_map(|item| match &*resolved(resolver, item) {
                    Object::String(string) => Some(decode_text_string(string.as_bytes())),
                    _ => None,
                })
                .collect();
            Some(FieldValue::Choices(choices))
        }
        // The text of a field may be a stream.
        Object::Stream(_) => {
            let id = value_reference?;
            let data = resolver.stream_data(id).ok()?;
            Some(FieldValue::Text(decode_text_string(&data.data)))
        }
        _ => None,
    }
}

fn read_options(resolver: &mut Resolver<'_>, options: &Object) -> Vec<(String, String)> {
    let options = resolved(resolver, options);
    let Some(items) = options.as_array() else {
        return Vec::new();
    };
    let text = |resolver: &mut Resolver<'_>, item: &Object| -> Option<String> {
        let item = resolved(resolver, item);
        Some(decode_text_string(item.as_string()?.as_bytes()))
    };
    let mut result = Vec::new();
    for item in items.iter().take(10_000) {
        let item = resolved(resolver, item);
        match &*item {
            Object::Array(pair) if pair.len() >= 2 => {
                if let (Some(export), Some(shown)) =
                    (text(resolver, &pair[0]), text(resolver, &pair[1]))
                {
                    result.push((export, shown));
                }
            }
            other => {
                if let Some(text) = other.as_string().map(|s| decode_text_string(s.as_bytes())) {
                    result.push((text.clone(), text));
                }
            }
        }
    }
    result
}
