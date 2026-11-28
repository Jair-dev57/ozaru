//! Flattening: the drawings of annotations and form fields become part of the
//! page, and the annotations and fields are gone (ISO 32000-2, section 12.5.5).

use std::collections::HashSet;

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::document::{FieldKind, FieldValue, decode_text_string};
use crate::syntax::{Dictionary, Name, Object, ObjectId};
use crate::writer::annotation::AnnotationFilter;
use crate::writer::canvas::num;
use crate::writer::forms::fill_field;

/// What flattening a page did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlattenReport {
    /// Annotations or widgets whose drawing was put on the page.
    pub painted: usize,
    /// Annotations or widgets taken off the page, including those that had
    /// nothing to draw or were hidden.
    pub removed: usize,
    /// Form fields taken out of the form.
    pub fields: usize,
}

impl FlattenReport {
    pub fn add(&mut self, other: &Self) {
        self.painted += other.painted;
        self.removed += other.removed;
        self.fields += other.fields;
    }
}

// What flattening reaches.
enum Selector<'a> {
    // Widgets, of the fields named, or of all.
    Widgets(Option<&'a [String]>),
    // Annotations that aren't widgets, links only if asked for.
    Annotations(&'a AnnotationFilter),
}

fn numbers_of(object: Option<&Object>) -> Vec<f64> {
    match object {
        Some(Object::Array(items)) => items.iter().filter_map(Object::as_number).collect(),
        _ => Vec::new(),
    }
}

impl Editor<'_, '_> {
    /// Puts the drawing of every widget of page `index` on the page, and
    /// removes the widgets and their fields from the form: the form can't be
    /// filled any more, and what it shows is fixed. `fields` are the full
    /// names of the fields to flatten; all if `None`.
    ///
    /// A widget without a drawing gets one from its value first. Hidden
    /// widgets are removed without being drawn. When no field is left, the
    /// form itself is removed.
    ///
    /// It is a `PageNotFound` error past the last page.
    pub fn flatten_widgets(
        &mut self,
        index: usize,
        fields: Option<&[String]>,
    ) -> Result<FlattenReport> {
        self.flatten(index, &Selector::Widgets(fields))
    }

    /// Puts the drawing of the annotations of page `index` that `filter`
    /// reaches on the page, and removes them. Form fields are left to
    /// [`flatten_widgets`](Self::flatten_widgets), and links stay unless
    /// `filter` names them: they have nothing to draw and still work.
    ///
    /// It is a `PageNotFound` error past the last page.
    pub fn flatten_annotations(
        &mut self,
        index: usize,
        filter: &AnnotationFilter,
    ) -> Result<FlattenReport> {
        self.flatten(index, &Selector::Annotations(filter))
    }

    fn flatten(&mut self, index: usize, selector: &Selector<'_>) -> Result<FlattenReport> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        let page_id = self.page_object(index)?;
        let items = self.annotation_list(page_id)?;
        let mut report = FlattenReport::default();
        let mut kept = Vec::with_capacity(items.len());
        let mut removed_ids: HashSet<ObjectId> = HashSet::new();
        let mut popups: Vec<Object> = Vec::new();
        let mut content = String::new();
        let mut xobjects: Vec<(Name, ObjectId)> = Vec::new();
        // The fields as the document has them, for drawings that are missing.
        let mut known: Option<Vec<crate::document::FormField>> = None;

        for item in items {
            let Some(mut dictionary) = self.annotation_dictionary(&item)? else {
                kept.push(item);
                continue;
            };
            if !self.selects(&dictionary, selector)? {
                kept.push(item);
                continue;
            }
            // A widget with nothing to show gets its drawing from its value.
            let widget = is_subtype(&dictionary, "Widget");
            if widget
                && !has_appearance(&dictionary)
                && let Some(id) = item.as_reference()
            {
                let fields = known.get_or_insert_with(|| self.document.form_fields());
                if let Some(field) = fields
                    .iter()
                    .find(|field| field.widgets.iter().any(|w| w.id == id))
                    .cloned()
                {
                    let value = field.value.clone().unwrap_or(match field.kind {
                        FieldKind::Checkbox | FieldKind::Radio => FieldValue::Text("Off".into()),
                        FieldKind::List => FieldValue::Choices(Vec::new()),
                        _ => FieldValue::Text(String::new()),
                    });
                    let _ = fill_field(self, &field, &value);
                    if let Some(updated) = self.annotation_dictionary(&item)? {
                        dictionary = updated;
                    }
                }
            }
            let flags = dictionary
                .get("F")
                .and_then(Object::as_integer)
                .unwrap_or(0);
            // Hidden, and not for viewing: not drawn.
            let visible = flags & (2 | 32) == 0;
            if visible && let Some((stream, matrix)) = self.painted_form(&dictionary)? {
                let name = Name::from(format!("OzFl{}", xobjects.len() + 1).as_str());
                let [a, b, c, d, e, f] = matrix.map(num);
                content.push_str(&format!(
                    "q {a} {b} {c} {d} {e} {f} cm /{} Do Q\n",
                    String::from_utf8_lossy(name.as_bytes())
                ));
                xobjects.push((name, stream));
                report.painted += 1;
            }
            report.removed += 1;
            if let Some(id) = item.as_reference() {
                removed_ids.insert(id);
            }
            if let Some(Object::Reference(popup)) = dictionary.get("Popup") {
                popups.push(Object::Reference(*popup));
            }
        }
        if report.removed == 0 {
            return Ok(report);
        }
        kept.retain(|item| !popups.contains(item));
        self.set_annotation_list(page_id, kept)?;
        if !content.is_empty() {
            self.append_content(index, content.as_bytes(), &xobjects)?;
        }
        if matches!(selector, Selector::Widgets(_)) {
            report.fields = self.prune_fields(&removed_ids)?;
        }
        Ok(report)
    }

    // The dictionary of an entry of `/Annots`, if it is one.
    pub(super) fn annotation_dictionary(&mut self, item: &Object) -> Result<Option<Dictionary>> {
        Ok(match item {
            Object::Dictionary(dictionary) => Some(dictionary.clone()),
            Object::Reference(id) => match self.object(*id)? {
                Object::Dictionary(dictionary) => Some(dictionary),
                _ => None,
            },
            _ => None,
        })
    }

    // Whether the annotation is one `selector` reaches.
    fn selects(&mut self, dictionary: &Dictionary, selector: &Selector<'_>) -> Result<bool> {
        let widget = is_subtype(dictionary, "Widget");
        match selector {
            Selector::Widgets(names) => {
                if !widget {
                    return Ok(false);
                }
                match names {
                    None => Ok(true),
                    Some(names) => {
                        let name = self.field_name(dictionary)?;
                        Ok(names.contains(&name))
                    }
                }
            }
            Selector::Annotations(filter) => {
                if widget {
                    return Ok(false);
                }
                let subtype = dictionary
                    .get("Subtype")
                    .and_then(Object::as_name)
                    .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned())
                    .unwrap_or_default();
                // Links, pop-ups and the like have no drawing: only if named.
                if matches!(subtype.as_str(), "Link" | "Popup")
                    && !filter.subtypes.contains(&subtype)
                {
                    return Ok(false);
                }
                Ok(crate::writer::editor::annotate::matches(dictionary, filter))
            }
        }
    }

    // The full name of the field a widget belongs to: the partial names of it
    // and its ancestors, joined by dots.
    fn field_name(&mut self, dictionary: &Dictionary) -> Result<String> {
        let mut names = Vec::new();
        let mut current = dictionary.clone();
        for _ in 0..32 {
            if let Some(Object::String(name)) = current.get("T") {
                names.push(decode_text_string(name.as_bytes()));
            }
            let Some(Object::Reference(parent)) = current.get("Parent").cloned() else {
                break;
            };
            match self.object(parent)? {
                Object::Dictionary(parent) => current = parent,
                _ => break,
            }
        }
        names.reverse();
        Ok(names.join("."))
    }

    // The form XObject that draws the annotation now, and the matrix that
    // places it in the annotation's rectangle (Algorithm 8.1 of the standard).
    fn painted_form(&mut self, dictionary: &Dictionary) -> Result<Option<(ObjectId, [f64; 6])>> {
        let Some(ap) = dictionary.get("AP").cloned() else {
            return Ok(None);
        };
        let ap = match ap {
            Object::Reference(id) => self.object(id)?,
            other => other,
        };
        let Some(normal) = ap.as_dictionary().and_then(|d| d.get("N")).cloned() else {
            return Ok(None);
        };
        // A stream, or the states of a button, of which `/AS` chooses one.
        let mut stream = match normal {
            Object::Reference(id) => match self.object(id)? {
                Object::Stream(_) => Some(id),
                Object::Dictionary(states) => state_stream(dictionary, &states),
                _ => None,
            },
            Object::Dictionary(states) => state_stream(dictionary, &states),
            _ => None,
        };
        let Some(id) = stream.take() else {
            return Ok(None);
        };
        let Object::Stream(form) = self.object(id)? else {
            return Ok(None);
        };
        let form = form.dictionary().clone();
        let bbox = numbers_of(form.get("BBox"));
        let rect = numbers_of(dictionary.get("Rect"));
        if bbox.len() != 4 || rect.len() != 4 {
            return Ok(None);
        }
        let own = numbers_of(form.get("Matrix"));
        let matrix = if own.len() == 6 {
            [own[0], own[1], own[2], own[3], own[4], own[5]]
        } else {
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        };
        // The box of the form once its own matrix is applied.
        let corners = [
            (bbox[0], bbox[1]),
            (bbox[2], bbox[1]),
            (bbox[2], bbox[3]),
            (bbox[0], bbox[3]),
        ]
        .map(|(x, y)| {
            (
                matrix[0] * x + matrix[2] * y + matrix[4],
                matrix[1] * x + matrix[3] * y + matrix[5],
            )
        });
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in corners {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let (left, bottom) = (rect[0].min(rect[2]), rect[1].min(rect[3]));
        let (width, height) = ((rect[0] - rect[2]).abs(), (rect[1] - rect[3]).abs());
        let (box_width, box_height) = (x1 - x0, y1 - y0);
        let scale_x = if box_width > 0.0 {
            width / box_width
        } else {
            1.0
        };
        let scale_y = if box_height > 0.0 {
            height / box_height
        } else {
            1.0
        };
        if ![scale_x, scale_y, x0, y0].iter().all(|v| v.is_finite()) {
            return Ok(None);
        }
        Ok(Some((
            id,
            [
                scale_x,
                0.0,
                0.0,
                scale_y,
                left - x0 * scale_x,
                bottom - y0 * scale_y,
            ],
        )))
    }

    // Takes the widgets in `removed` out of the form, and the fields left
    // without any; without fields, the form itself goes. Gives how many fields
    // were taken out.
    fn prune_fields(&mut self, removed: &HashSet<ObjectId>) -> Result<usize> {
        let mut catalog = self.catalog_dictionary()?;
        let (form_id, mut form) = match catalog.get("AcroForm").cloned() {
            Some(Object::Reference(id)) => match self.object(id)? {
                Object::Dictionary(form) => (Some(id), form),
                _ => return Ok(0),
            },
            Some(Object::Dictionary(form)) => (None, form),
            _ => return Ok(0),
        };
        let roots = match form.get("Fields").cloned() {
            Some(Object::Array(items)) => items,
            Some(Object::Reference(id)) => match self.object(id)? {
                Object::Array(items) => items,
                _ => return Ok(0),
            },
            _ => return Ok(0),
        };
        let mut dropped = 0;
        let mut kept = Vec::new();
        for root in roots {
            if self.prune_node(&root, removed, 0, &mut dropped)? {
                kept.push(root);
            }
        }
        if kept.is_empty() {
            catalog.remove("AcroForm");
            self.set(self.catalog, Object::Dictionary(catalog));
            return Ok(dropped);
        }
        form.insert(Name::from("Fields"), Object::Array(kept));
        match form_id {
            Some(id) => self.set(id, Object::Dictionary(form)),
            None => {
                catalog.insert(Name::from("AcroForm"), Object::Dictionary(form));
                self.set(self.catalog, Object::Dictionary(catalog));
            }
        }
        Ok(dropped)
    }

    // Whether a node of the field tree stays: not if it is a removed widget, or
    // its kids are all gone.
    fn prune_node(
        &mut self,
        node: &Object,
        removed: &HashSet<ObjectId>,
        depth: usize,
        dropped: &mut usize,
    ) -> Result<bool> {
        if depth > 32 {
            return Ok(true);
        }
        let id = node.as_reference();
        if let Some(id) = id
            && removed.contains(&id)
        {
            *dropped += 1;
            return Ok(false);
        }
        let dictionary = match node {
            Object::Dictionary(dictionary) => dictionary.clone(),
            Object::Reference(id) => match self.object(*id)? {
                Object::Dictionary(dictionary) => dictionary,
                _ => return Ok(true),
            },
            _ => return Ok(true),
        };
        let Some(kids) = (match dictionary.get("Kids").cloned() {
            Some(Object::Array(items)) => Some(items),
            Some(Object::Reference(list)) => match self.object(list)? {
                Object::Array(items) => Some(items),
                _ => None,
            },
            _ => None,
        }) else {
            return Ok(true);
        };
        let before = kids.len();
        let mut remaining = Vec::new();
        for kid in kids {
            // A kid that is a widget of this field and goes isn't a field.
            let is_widget = kid.as_reference().is_some_and(|k| removed.contains(&k));
            let before_dropped = *dropped;
            if self.prune_node(&kid, removed, depth + 1, dropped)? {
                remaining.push(kid);
            } else if is_widget {
                *dropped = before_dropped;
            }
        }
        if remaining.len() == before {
            return Ok(true);
        }
        if remaining.is_empty() {
            if dictionary.contains_key("T") {
                *dropped += 1;
            }
            return Ok(false);
        }
        let mut updated = dictionary;
        updated.insert(Name::from("Kids"), Object::Array(remaining));
        if let Some(id) = id {
            self.set(id, Object::Dictionary(updated));
        }
        Ok(true)
    }
}

fn is_subtype(dictionary: &Dictionary, wanted: &str) -> bool {
    dictionary
        .get("Subtype")
        .and_then(Object::as_name)
        .is_some_and(|name| name.as_bytes() == wanted.as_bytes())
}

fn has_appearance(dictionary: &Dictionary) -> bool {
    match dictionary.get("AP") {
        Some(Object::Dictionary(ap)) => ap.contains_key("N"),
        Some(Object::Reference(_)) => true,
        _ => false,
    }
}

// The drawing of the state `/AS` names, among those of a button.
fn state_stream(annotation: &Dictionary, states: &Dictionary) -> Option<ObjectId> {
    let state = annotation.get("AS").and_then(Object::as_name)?;
    states.get(state.as_bytes())?.as_reference()
}
