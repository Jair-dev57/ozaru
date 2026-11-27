//! Redacting pages that exist: what is under the areas is taken out, and a
//! box is drawn over them.

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::document::Rectangle;
use crate::filters::deflate_zlib;
use crate::syntax::{Dictionary, Name, Object, ObjectId};
use crate::writer::canvas::Canvas;
use crate::writer::redact::{RedactArea, RedactOptions, RedactReport, Rewriter, Rewritten};

impl Editor<'_, '_> {
    /// Takes out of page `index` what is under `areas`, and draws the box of
    /// each area over it.
    ///
    /// Text is taken out of the strings that show it (the rest of the line
    /// keeps its place), lines, shapes and images are removed or, for images,
    /// have their pixels under the area blanked, as `options` say, forms that
    /// hold any of that are copied and rewritten, and the annotations under
    /// an area are removed. Nothing of what goes stays in the saved file: the
    /// old content is dropped when it is written, so don't save the result
    /// incrementally.
    ///
    /// What it doesn't reach: the text of other places of the document (the
    /// structure tree, bookmarks, form fields, the metadata), glyphs of Type 3
    /// fonts, and what patterns paint.
    ///
    /// It is a `PageNotFound` error past the last page, and a
    /// `RedactionIncomplete` one if the page is so complex that its content
    /// couldn't be checked to the end. Redact before drawing or editing the
    /// content of the same page.
    pub fn redact(
        &mut self,
        index: usize,
        areas: &[RedactArea],
        options: &RedactOptions,
    ) -> Result<RedactReport> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        let mut report = RedactReport::default();
        if areas.is_empty() {
            return Ok(report);
        }
        for area in areas {
            let r = area.rect;
            if ![r.left, r.bottom, r.right, r.top]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err(Error::new(ErrorKind::NonFiniteNumber, 0));
            }
        }
        let page_id = self.page_object(index)?;

        // The content, rewritten.
        let source = self.pages[index].source;
        if let Some(page) = source
            .and_then(|source| self.document.page(source))
            .cloned()
        {
            let limits = self.document.limits();
            let first_name = self.next_number as usize;
            let mut rewriter = Rewriter::new(
                self.document.resolver(),
                areas,
                *options,
                limits,
                first_name,
            );
            let (content, trace, data) = rewriter
                .interpreter
                .trace_page(&mut *rewriter.resolver, &page);
            let rewritten = rewriter.rewrite(&data, page.resources(), &content, &trace)?;
            report.add(&rewriter.report);
            if rewritten.changed {
                let mut dictionary = self.page_dictionary(page_id)?;
                let mut resources = self.page_resources(index, &dictionary)?;
                let entries = self.add_rewritten(&rewritten, &resources)?;
                if !entries.is_empty() {
                    let mut objects = self.sub_dictionary(&resources, "XObject")?;
                    for (name, id) in entries {
                        objects.insert(name, Object::Reference(id));
                    }
                    resources.insert(Name::from("XObject"), Object::Dictionary(objects));
                }
                let contents = self.add_content(&rewritten.data);
                dictionary.insert(Name::from("Resources"), Object::Dictionary(resources));
                dictionary.insert(Name::from("Contents"), Object::Reference(contents));
                self.set(page_id, Object::Dictionary(dictionary));
            }
        }

        // The annotations under the areas.
        self.redact_annotations(page_id, areas, options, &mut report)?;

        // The boxes.
        let mut canvas = Canvas::new();
        for area in areas {
            let Some(fill) = area.fill else {
                continue;
            };
            let r = area.rect;
            canvas.set_fill_color(fill);
            canvas.rectangle(r.left, r.bottom, r.right - r.left, r.top - r.bottom)?;
            canvas.fill();
        }
        self.draw(index, &canvas)?;
        Ok(report)
    }

    // The objects of a rewritten stream: its images and forms. They are given
    // by the names the stream uses, for the resources that hold them.
    // `resources` are those of the stream, which a form without resources of
    // its own shares.
    fn add_rewritten(
        &mut self,
        rewritten: &Rewritten,
        resources: &Dictionary,
    ) -> Result<Vec<(Name, ObjectId)>> {
        let mut entries = Vec::new();
        for (name, image) in &rewritten.images {
            let mut dictionary = image.dictionary.clone();
            if let Some((mask_dictionary, mask_data)) = &image.mask {
                let mask = self.add_stream(mask_dictionary.clone(), mask_data.clone());
                dictionary.insert(Name::from("SMask"), Object::Reference(mask));
            }
            let id = self.add_stream(dictionary, image.data.clone());
            entries.push((Name::from(name.as_str()), id));
        }
        for (name, original, inner) in &rewritten.forms {
            let Object::Stream(stream) = self.object(*original)? else {
                continue;
            };
            let mut dictionary = stream.dictionary().clone();
            let own = match dictionary.get("Resources").cloned() {
                Some(entry) => Some(self.dictionary_of(Some(entry))?),
                None => None,
            };
            let mut form_resources = own.clone().unwrap_or_else(|| resources.clone());
            let children = self.add_rewritten(inner, &form_resources)?;
            if !children.is_empty() {
                let mut objects = self.sub_dictionary(&form_resources, "XObject")?;
                for (child, id) in children {
                    objects.insert(child, Object::Reference(id));
                }
                form_resources.insert(Name::from("XObject"), Object::Dictionary(objects));
                dictionary.insert(Name::from("Resources"), Object::Dictionary(form_resources));
            }
            for key in [
                "Length",
                "DecodeParms",
                "DL",
                "F",
                "FFilter",
                "FDecodeParms",
            ] {
                dictionary.remove(key);
            }
            dictionary.insert(
                Name::from("Filter"),
                Object::Name(Name::from("FlateDecode")),
            );
            let id = self.add_stream(dictionary, deflate_zlib(&inner.data, 6));
            entries.push((Name::from(name.as_str()), id));
        }
        Ok(entries)
    }

    // Takes out the annotations of the page whose rectangle touches an area
    // (and counts the form fields that do, which stay).
    fn redact_annotations(
        &mut self,
        page_id: ObjectId,
        areas: &[RedactArea],
        options: &RedactOptions,
        report: &mut RedactReport,
    ) -> Result<()> {
        let items = self.annotation_list(page_id)?;
        let mut kept = Vec::with_capacity(items.len());
        let mut popups: Vec<Object> = Vec::new();
        let mut changed = false;
        for item in items {
            let dictionary = match &item {
                Object::Dictionary(dictionary) => Some(dictionary.clone()),
                Object::Reference(id) => match self.object(*id)? {
                    Object::Dictionary(dictionary) => Some(dictionary),
                    _ => None,
                },
                _ => None,
            };
            let Some(dictionary) = dictionary else {
                kept.push(item);
                continue;
            };
            let rect: Vec<f64> = match dictionary.get("Rect") {
                Some(Object::Array(numbers)) => {
                    numbers.iter().filter_map(Object::as_number).collect()
                }
                _ => Vec::new(),
            };
            let touched = rect.len() == 4 && {
                let r = Rectangle::new(rect[0], rect[1], rect[2], rect[3]);
                areas.iter().any(|a| {
                    r.left < a.rect.right
                        && a.rect.left < r.right
                        && r.bottom < a.rect.top
                        && a.rect.bottom < r.top
                })
            };
            let widget = dictionary
                .get("Subtype")
                .and_then(Object::as_name)
                .is_some_and(|name| name.as_bytes() == b"Widget");
            if touched && widget {
                report.widgets_left += 1;
            }
            if touched && !widget && options.annotations {
                report.annotations += 1;
                changed = true;
                // Its pop-up goes with it.
                if let Some(Object::Reference(popup)) = dictionary.get("Popup") {
                    popups.push(Object::Reference(*popup));
                }
                continue;
            }
            kept.push(item);
        }
        if changed {
            kept.retain(|item| !popups.contains(item));
            self.set_annotation_list(page_id, kept)?;
        }
        Ok(())
    }
}
