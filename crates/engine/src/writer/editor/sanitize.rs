//! Sanitizing a document: see [`Editor::sanitize`].

use std::collections::HashSet;

use super::Editor;
use crate::base::Result;
use crate::document::{FieldKind, FieldValue};
use crate::syntax::{Dictionary, Name, Object, ObjectId};
use crate::writer::forms::fill_field;
use crate::writer::sanitize::{SanitizeOptions, SanitizeReport};

// The kinds of action that run something, or reach for something outside the
// document; links to addresses and pages aren't among them.
const ACTIVE_ACTIONS: [&[u8]; 10] = [
    b"JavaScript",
    b"Launch",
    b"SubmitForm",
    b"ImportData",
    b"Rendition",
    b"Movie",
    b"Sound",
    b"GoToE",
    b"GoTo3DView",
    b"RichMediaExecute",
];

// Annotations that play media or run content.
const ACTIVE_ANNOTATIONS: [&[u8]; 5] = [b"Movie", b"Sound", b"Screen", b"RichMedia", b"3D"];

// Annotations that are comments.
const COMMENTS: [&[u8]; 16] = [
    b"Text",
    b"FreeText",
    b"Line",
    b"Square",
    b"Circle",
    b"Polygon",
    b"PolyLine",
    b"Highlight",
    b"Underline",
    b"Squiggly",
    b"StrikeOut",
    b"Stamp",
    b"Caret",
    b"Ink",
    b"Popup",
    b"Redact",
];

// Entries of a page or drawing that are metadata or private data.
const METADATA_KEYS: [&str; 2] = ["Metadata", "PieceInfo"];

// The most levels of forms inside forms looked into.
const MAX_FORM_DEPTH: usize = 8;

fn kind_of(dictionary: &Dictionary) -> Vec<u8> {
    dictionary
        .get("Subtype")
        .and_then(Object::as_name)
        .map(|n| n.as_bytes().to_vec())
        .unwrap_or_default()
}

impl Editor<'_, '_> {
    /// Takes out of the document what `options` say, and gives what it took.
    ///
    /// The pages look the same. Nothing that is taken out stays in the saved
    /// file (objects nothing points at aren't written), so don't save the
    /// result incrementally.
    ///
    /// What it can't reach: text inside the content of the pages (that is
    /// [`redact`](Self::redact)), the names of fonts, the file's own
    /// identifier, and anything in a place it doesn't know.
    pub fn sanitize(&mut self, options: &SanitizeOptions) -> Result<SanitizeReport> {
        let mut report = SanitizeReport::default();
        self.sanitize_catalog(options, &mut report)?;
        if options.metadata {
            self.sanitize_info(options, &mut report)?;
        }
        for index in 0..self.pages.len() {
            self.sanitize_page(index, options, &mut report)?;
        }
        if options.form_values {
            self.clear_form_values(&mut report)?;
        }
        Ok(report)
    }

    // Whether an action, or one that follows it, runs something.
    fn is_active(&mut self, action: &Object, depth: usize) -> Result<bool> {
        if depth > 8 {
            return Ok(true);
        }
        let dictionary = match action {
            Object::Dictionary(dictionary) => dictionary.clone(),
            Object::Reference(id) => match self.object(*id)? {
                Object::Dictionary(dictionary) => dictionary,
                _ => return Ok(false),
            },
            Object::Array(items) => {
                for item in items {
                    if self.is_active(item, depth + 1)? {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            _ => return Ok(false),
        };
        let name = dictionary
            .get("S")
            .and_then(Object::as_name)
            .map(|n| n.as_bytes().to_vec())
            .unwrap_or_default();
        if ACTIVE_ACTIONS.contains(&name.as_slice()) {
            return Ok(true);
        }
        match dictionary.get("Next").cloned() {
            Some(next) => self.is_active(&next, depth + 1),
            None => Ok(false),
        }
    }

    // Removes the action at `key` of `dictionary` if it runs something, and the
    // additional actions at `AA` altogether.
    fn clean_actions(
        &mut self,
        dictionary: &mut Dictionary,
        report: &mut SanitizeReport,
    ) -> Result<bool> {
        let mut changed = false;
        if let Some(action) = dictionary.get("A").cloned()
            && self.is_active(&action, 0)?
        {
            dictionary.remove("A");
            report.scripts += 1;
            changed = true;
        }
        if dictionary.remove("AA").is_some() {
            report.scripts += 1;
            changed = true;
        }
        Ok(changed)
    }

    fn sanitize_catalog(
        &mut self,
        options: &SanitizeOptions,
        report: &mut SanitizeReport,
    ) -> Result<()> {
        let mut catalog = self.catalog_dictionary()?;
        let before = catalog.clone();
        if options.metadata {
            if catalog.remove("Metadata").is_some() {
                report.metadata_streams += 1;
            }
            if catalog.remove("PieceInfo").is_some() {
                report.private_data += 1;
            }
        }
        if options.scripts {
            if let Some(open) = catalog.get("OpenAction").cloned()
                && !matches!(open, Object::Array(_))
                && self.is_active(&open, 0)?
            {
                catalog.remove("OpenAction");
                report.scripts += 1;
            }
            if catalog.remove("AA").is_some() {
                report.scripts += 1;
            }
        }
        if options.attachments && catalog.remove("AF").is_some() {
            report.attachments += 1;
        }
        if options.bookmarks && catalog.contains_key("Outlines") {
            report.bookmarks = self.document.outlines().len().max(1);
            catalog.remove("Outlines");
        }
        if options.structure {
            let removed = catalog.remove("StructTreeRoot").is_some();
            let marked = catalog.remove("MarkInfo").is_some();
            report.structure += usize::from(removed || marked);
        }
        // The names of the document: scripts and attached files.
        if (options.scripts || options.attachments)
            && let Some(names) = catalog.get("Names").cloned()
        {
            let mut dictionary = self.dictionary_of(Some(names))?;
            if options.scripts && dictionary.remove("JavaScript").is_some() {
                report.scripts += 1;
            }
            if options.attachments && dictionary.remove("EmbeddedFiles").is_some() {
                report.attachments += 1;
            }
            if dictionary.is_empty() {
                catalog.remove("Names");
            } else {
                catalog.insert(Name::from("Names"), Object::Dictionary(dictionary));
            }
        }
        // The interactive form: XFA data and the calculation order.
        if options.scripts
            && let Some(form) = catalog.get("AcroForm").cloned()
        {
            let form_id = form.as_reference();
            let mut dictionary = self.dictionary_of(Some(form))?;
            let mut changed = false;
            if dictionary.remove("XFA").is_some() {
                report.scripts += 1;
                changed = true;
            }
            changed |= dictionary.remove("CO").is_some();
            if changed {
                match form_id {
                    Some(id) => self.set(id, Object::Dictionary(dictionary)),
                    None => {
                        catalog.insert(Name::from("AcroForm"), Object::Dictionary(dictionary));
                    }
                }
            }
            self.sanitize_fields(options, report)?;
        }
        if catalog != before {
            self.set(self.catalog, Object::Dictionary(catalog));
        }
        Ok(())
    }

    // The actions of the fields of the form, which hold the scripts that
    // format, validate and calculate.
    fn sanitize_fields(
        &mut self,
        _options: &SanitizeOptions,
        report: &mut SanitizeReport,
    ) -> Result<()> {
        let mut stack: Vec<Object> = Vec::new();
        let (_, form) = self.acro_form()?;
        stack.extend(self.form_fields_list(&form)?);
        let mut seen: HashSet<ObjectId> = HashSet::new();
        while let Some(node) = stack.pop() {
            let id = node.as_reference();
            if let Some(id) = id
                && !seen.insert(id)
            {
                continue;
            }
            let Some(mut dictionary) = (match &node {
                Object::Dictionary(dictionary) => Some(dictionary.clone()),
                Object::Reference(id) => match self.object(*id)? {
                    Object::Dictionary(dictionary) => Some(dictionary),
                    _ => None,
                },
                _ => None,
            }) else {
                continue;
            };
            if let Some(Object::Array(kids)) = dictionary.get("Kids") {
                stack.extend(kids.iter().cloned());
            }
            if self.clean_actions(&mut dictionary, report)?
                && let Some(id) = id
            {
                self.set(id, Object::Dictionary(dictionary));
            }
            if seen.len() > 100_000 {
                break;
            }
        }
        Ok(())
    }

    fn sanitize_info(
        &mut self,
        options: &SanitizeOptions,
        report: &mut SanitizeReport,
    ) -> Result<()> {
        let Some(id) = self.info else {
            return Ok(());
        };
        let mut info = match self.object(id)? {
            Object::Dictionary(info) => info,
            _ => return Ok(()),
        };
        let keys: Vec<Name> = info.iter().map(|(key, _)| key.clone()).collect();
        for key in keys {
            let name = String::from_utf8_lossy(key.as_bytes()).into_owned();
            if options.keep_info.contains(&name) {
                continue;
            }
            info.remove(key.as_bytes());
            report.info_entries += 1;
        }
        if info.is_empty() {
            self.info = None;
        } else {
            self.set(id, Object::Dictionary(info));
        }
        Ok(())
    }

    fn sanitize_page(
        &mut self,
        index: usize,
        options: &SanitizeOptions,
        report: &mut SanitizeReport,
    ) -> Result<()> {
        let page_id = self.page_object(index)?;
        let mut page = self.page_dictionary(page_id)?;
        let before = page.clone();
        if options.metadata {
            for key in METADATA_KEYS {
                if page.remove(key).is_some() {
                    if key == "Metadata" {
                        report.metadata_streams += 1;
                    } else {
                        report.private_data += 1;
                    }
                }
            }
            if page.remove("Thumb").is_some() {
                report.private_data += 1;
            }
            page.remove("LastModified");
        }
        if options.scripts && page.remove("AA").is_some() {
            report.scripts += 1;
        }
        if options.attachments && page.remove("AF").is_some() {
            report.attachments += 1;
        }
        if options.structure && page.remove("StructParents").is_some() {
            report.structure += 1;
        }
        if page != before {
            self.set(page_id, Object::Dictionary(page.clone()));
        }

        self.sanitize_annotations(page_id, options, report)?;
        if options.metadata || options.attachments {
            let resources = self.page_resources(index, &page)?;
            let mut seen = HashSet::new();
            self.sanitize_drawings(&resources, options, report, &mut seen, 0)?;
        }
        Ok(())
    }

    fn sanitize_annotations(
        &mut self,
        page_id: ObjectId,
        options: &SanitizeOptions,
        report: &mut SanitizeReport,
    ) -> Result<()> {
        let items = self.annotation_list(page_id)?;
        if items.is_empty() {
            return Ok(());
        }
        let mut kept = Vec::with_capacity(items.len());
        let mut dropped: Vec<Object> = Vec::new();
        let mut list_changed = false;
        for item in items {
            let Some(mut dictionary) = self.annotation_dictionary(&item)? else {
                kept.push(item);
                continue;
            };
            let kind = kind_of(&dictionary);
            let remove = (options.scripts && ACTIVE_ANNOTATIONS.contains(&kind.as_slice()))
                || (options.attachments && kind == b"FileAttachment")
                || (options.comments && COMMENTS.contains(&kind.as_slice()));
            if remove {
                if options.comments && COMMENTS.contains(&kind.as_slice()) {
                    report.comments += 1;
                } else if kind == b"FileAttachment" {
                    report.attachments += 1;
                } else {
                    report.scripts += 1;
                }
                if let Some(popup @ Object::Reference(_)) = dictionary.get("Popup") {
                    dropped.push(popup.clone());
                }
                list_changed = true;
                continue;
            }
            let mut changed = false;
            if options.scripts {
                changed |= self.clean_actions(&mut dictionary, report)?;
            }
            if options.metadata {
                for key in METADATA_KEYS {
                    changed |= dictionary.remove(key).is_some();
                }
            }
            if options.attachments && dictionary.remove("AF").is_some() {
                report.attachments += 1;
                changed = true;
            }
            if changed {
                match &item {
                    Object::Reference(id) => self.set(*id, Object::Dictionary(dictionary)),
                    _ => {
                        // A dictionary the page holds directly.
                        kept.push(Object::Dictionary(dictionary));
                        list_changed = true;
                        continue;
                    }
                }
            }
            kept.push(item);
        }
        if list_changed {
            kept.retain(|item| !dropped.contains(item));
            self.set_annotation_list(page_id, kept)?;
        }
        Ok(())
    }

    // Takes the metadata and attached files out of the images and drawings the
    // `resources` hold, and of those they hold.
    fn sanitize_drawings(
        &mut self,
        resources: &Dictionary,
        options: &SanitizeOptions,
        report: &mut SanitizeReport,
        seen: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_FORM_DEPTH {
            return Ok(());
        }
        let objects = self.sub_dictionary(resources, "XObject")?;
        let ids: Vec<ObjectId> = objects
            .iter()
            .filter_map(|(_, entry)| entry.as_reference())
            .collect();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let Object::Stream(stream) = self.object(id)? else {
                continue;
            };
            let mut dictionary = stream.dictionary().clone();
            let mut changed = false;
            if options.metadata {
                for key in METADATA_KEYS {
                    if dictionary.remove(key).is_some() {
                        if key == "Metadata" {
                            report.metadata_streams += 1;
                        } else {
                            report.private_data += 1;
                        }
                        changed = true;
                    }
                }
            }
            if options.attachments && dictionary.remove("AF").is_some() {
                report.attachments += 1;
                changed = true;
            }
            if changed {
                let data = self.stream_data(id)?;
                self.set_stream(id, dictionary.clone(), data);
            }
            // Forms hold drawings too.
            if let Some(inner) = dictionary.get("Resources").cloned() {
                let inner = self.dictionary_of(Some(inner))?;
                self.sanitize_drawings(&inner, options, report, seen, depth + 1)?;
            }
        }
        Ok(())
    }

    // Empties the value of every field that holds one, and draws it empty.
    fn clear_form_values(&mut self, report: &mut SanitizeReport) -> Result<()> {
        for field in self.document.form_fields() {
            let value = match field.kind {
                FieldKind::Text | FieldKind::Combo => FieldValue::Text(String::new()),
                FieldKind::Checkbox | FieldKind::Radio => FieldValue::Text("Off".to_string()),
                FieldKind::List => FieldValue::Choices(Vec::new()),
                _ => continue,
            };
            if field.value.is_none() && field.default.is_none() {
                continue;
            }
            if fill_field(self, &field, &value).is_ok() {
                report.form_values += 1;
            }
            if let Object::Dictionary(mut dictionary) = self.object(field.id)?
                && dictionary.remove("DV").is_some()
            {
                self.set(field.id, Object::Dictionary(dictionary));
            }
        }
        Ok(())
    }
}
