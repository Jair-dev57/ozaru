//! Adding annotations to pages that exist, and changing or removing the ones
//! they have.

use super::Editor;
use super::draw::font_dictionary;
use crate::base::{Error, ErrorKind, Result};
use crate::document::decode_text_string;
use crate::filters::deflate_zlib;
use crate::fonts::StandardFont;
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
use crate::writer::annotation::{
    AnnotationChanges, AnnotationFilter, AnnotationShape, LinkTarget, MarkupKind, NewAnnotation,
};
use crate::writer::canvas::{
    Canvas, LineCap, LineJoin, font_resource, num, state_resource, text_width,
};
use crate::writer::forms::text_string;

fn finite(values: &[f64]) -> Result<()> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::NonFiniteNumber, 0))
    }
}

fn name(text: &str) -> Object {
    Object::Name(Name::from(text))
}

fn numbers(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|&v| Object::Real(v)).collect())
}

// A rectangle with its corners in order and `margin` points larger.
fn normalized(rect: [f64; 4], margin: f64) -> [f64; 4] {
    [
        rect[0].min(rect[2]) - margin,
        rect[1].min(rect[3]) - margin,
        rect[0].max(rect[2]) + margin,
        rect[1].max(rect[3]) + margin,
    ]
}

// The box around some points, `margin` points larger.
fn around(points: &[[f64; 2]], margin: f64) -> [f64; 4] {
    let mut area = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for point in points {
        area[0] = area[0].min(point[0]);
        area[1] = area[1].min(point[1]);
        area[2] = area[2].max(point[0]);
        area[3] = area[3].max(point[1]);
    }
    [
        area[0] - margin,
        area[1] - margin,
        area[2] + margin,
        area[3] + margin,
    ]
}

// A URI as the ASCII a PDF string holds: other bytes as `%XX`.
fn uri_bytes(uri: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for &byte in uri.as_bytes() {
        if (33..=126).contains(&byte) {
            out.push(byte);
        } else {
            out.extend_from_slice(format!("%{byte:02X}").as_bytes());
        }
    }
    out
}

// What the annotation draws and the entries that say what it is.
struct Built {
    rect: [f64; 4],
    entries: Vec<(&'static str, Object)>,
    // The form that draws it, if it needs one.
    form: Option<ObjectId>,
    // Annotations that are printed, and for sticky notes that keep their size.
    flags: i64,
}

impl Editor<'_, '_> {
    /// Adds `annotation` to page `index` and gives its object.
    ///
    /// The annotation is made with an appearance stream, so every reader draws
    /// it as it is drawn here, and the page keeps what it had. It is a
    /// `PageNotFound` error past the last page (or for a link to one), an
    /// `InvalidBox` one for a markup without quadrilaterals, a path without
    /// points or a box without area, an `UnsupportedCharacter` one for text
    /// Helvetica can't draw, and a `NonFiniteNumber` one for numbers that
    /// aren't finite.
    pub fn add_annotation(&mut self, index: usize, annotation: &NewAnnotation) -> Result<ObjectId> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        finite(&[annotation.opacity])?;
        let page_id = self.page_object(index)?;
        let color = annotation.color.map(|v| f64::from(v) / 255.0);
        let opacity = annotation.opacity.clamp(0.0, 1.0);
        let built = self.build(annotation, color, opacity)?;

        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), name("Annot"));
        dictionary.insert(Name::from("Rect"), numbers(&built.rect));
        for (key, value) in built.entries {
            dictionary.insert(Name::from(key), value);
        }
        if let Some(form) = built.form {
            let mut normal = Dictionary::new();
            normal.insert(Name::from("N"), Object::Reference(form));
            dictionary.insert(Name::from("AP"), Object::Dictionary(normal));
        }
        if opacity < 1.0 {
            dictionary.insert(Name::from("CA"), Object::Real(opacity));
        }
        dictionary.insert(Name::from("F"), Object::Integer(built.flags));
        dictionary.insert(Name::from("P"), Object::Reference(page_id));
        if let Some(text) = &annotation.contents {
            dictionary.insert(Name::from("Contents"), text_string(text));
        }
        if let Some(text) = &annotation.author {
            dictionary.insert(Name::from("T"), text_string(text));
        }
        if let Some(text) = &annotation.subject {
            dictionary.insert(Name::from("Subj"), text_string(text));
        }
        if let Some(date) = &annotation.date {
            dictionary.insert(
                Name::from("M"),
                Object::String(PdfString::new(
                    date.to_pdf_string().into_bytes(),
                    StringFormat::Literal,
                )),
            );
        }
        let id = self.add(Object::Dictionary(Dictionary::new()));
        dictionary.insert(
            Name::from("NM"),
            Object::String(PdfString::new(
                format!("ozaru-{}", id.number).into_bytes(),
                StringFormat::Literal,
            )),
        );
        self.set(id, Object::Dictionary(dictionary));

        let mut items = self.annotation_list(page_id)?;
        items.push(Object::Reference(id));
        self.set_annotation_list(page_id, items)?;
        Ok(id)
    }

    // The entries of a page's `/Annots`, which may be an array or the
    // reference to one.
    fn annotation_list(&mut self, page_id: ObjectId) -> Result<Vec<Object>> {
        let page = self.page_dictionary(page_id)?;
        Ok(match page.get("Annots") {
            Some(Object::Array(items)) => items.clone(),
            Some(Object::Reference(list)) => {
                let list = *list;
                match self.object(list)? {
                    Object::Array(items) => items,
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        })
    }

    fn set_annotation_list(&mut self, page_id: ObjectId, items: Vec<Object>) -> Result<()> {
        let mut page = self.page_dictionary(page_id)?;
        if items.is_empty() {
            page.remove("Annots");
        } else {
            page.insert(Name::from("Annots"), Object::Array(items));
        }
        self.set(page_id, Object::Dictionary(page));
        Ok(())
    }

    fn build(
        &mut self,
        annotation: &NewAnnotation,
        color: [f64; 3],
        opacity: f64,
    ) -> Result<Built> {
        let invalid = || Error::new(ErrorKind::InvalidBox, 0);
        let mut entries: Vec<(&'static str, Object)> = Vec::new();
        let stroke = annotation.color;
        match &annotation.shape {
            AnnotationShape::TextMarkup { kind, quads } => {
                if quads.is_empty() {
                    return Err(invalid());
                }
                for quad in quads {
                    finite(quad)?;
                }
                let area = bounding_box(quads, 1.5);
                entries.push(("Subtype", name(kind.subtype())));
                entries.push((
                    "QuadPoints",
                    numbers(&quads.iter().flatten().copied().collect::<Vec<_>>()),
                ));
                entries.push(("C", numbers(&color)));
                let form = self.markup_appearance(*kind, quads, color, opacity, area);
                Ok(Built {
                    rect: area,
                    entries,
                    form: Some(form),
                    flags: 4,
                })
            }
            AnnotationShape::Note { x, y, open } => {
                finite(&[*x, *y])?;
                let rect = [*x, y - 20.0, x + 20.0, *y];
                entries.push(("Subtype", name("Text")));
                entries.push(("Name", name("Note")));
                entries.push(("C", numbers(&color)));
                entries.push(("Open", Object::Boolean(*open)));
                let mut canvas = Canvas::new();
                canvas.set_fill_color(stroke);
                canvas.set_stroke_color([0, 0, 0]);
                canvas.set_line_width(0.8)?;
                canvas.rectangle(x + 1.0, y - 19.0, 18.0, 18.0)?;
                canvas.fill_and_stroke();
                for step in 0..3 {
                    let at = y - 6.0 - f64::from(step) * 4.0;
                    canvas.move_to(x + 4.0, at)?;
                    canvas.line_to(x + 16.0, at)?;
                }
                canvas.stroke();
                let form = self.canvas_form(&canvas, rect);
                // Print, don't zoom, don't rotate.
                Ok(Built {
                    rect,
                    entries,
                    form: Some(form),
                    flags: 28,
                })
            }
            AnnotationShape::Link {
                rect,
                target,
                border_width,
            } => {
                finite(rect)?;
                finite(&[*border_width])?;
                let rect = normalized(*rect, 0.0);
                if rect[0] == rect[2] || rect[1] == rect[3] {
                    return Err(invalid());
                }
                let width = border_width.max(0.0);
                entries.push(("Subtype", name("Link")));
                entries.push((
                    "Border",
                    Object::Array(vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Real(width),
                    ]),
                ));
                match target {
                    LinkTarget::Uri(uri) => {
                        let mut action = Dictionary::new();
                        action.insert(Name::from("S"), name("URI"));
                        action.insert(
                            Name::from("URI"),
                            Object::String(PdfString::new(uri_bytes(uri), StringFormat::Literal)),
                        );
                        entries.push(("A", Object::Dictionary(action)));
                    }
                    LinkTarget::Page(page) => {
                        let target = self.page_object(*page)?;
                        entries.push((
                            "Dest",
                            Object::Array(vec![
                                Object::Reference(target),
                                name("XYZ"),
                                Object::Null,
                                Object::Null,
                                Object::Null,
                            ]),
                        ));
                    }
                }
                let mut form = None;
                if width > 0.0 {
                    entries.push(("C", numbers(&color)));
                    let mut canvas = Canvas::new();
                    canvas.set_stroke_color(stroke);
                    canvas.set_line_width(width)?;
                    canvas.rectangle(
                        rect[0] + width / 2.0,
                        rect[1] + width / 2.0,
                        rect[2] - rect[0] - width,
                        rect[3] - rect[1] - width,
                    )?;
                    canvas.stroke();
                    form = Some(self.canvas_form(&canvas, rect));
                }
                Ok(Built {
                    rect,
                    entries,
                    form,
                    flags: 4,
                })
            }
            AnnotationShape::Square { rect, fill, width }
            | AnnotationShape::Circle { rect, fill, width } => {
                finite(rect)?;
                finite(&[*width])?;
                let inner = normalized(*rect, 0.0);
                if inner[0] == inner[2] || inner[1] == inner[3] {
                    return Err(invalid());
                }
                let width = width.max(0.0);
                let outer = normalized(*rect, width / 2.0);
                let circle = matches!(annotation.shape, AnnotationShape::Circle { .. });
                entries.push(("Subtype", name(if circle { "Circle" } else { "Square" })));
                entries.push(("C", numbers(&color)));
                if let Some(fill) = fill {
                    entries.push(("IC", numbers(&fill.map(|v| f64::from(v) / 255.0))));
                }
                entries.push(("BS", border_style(width)));
                let margin = width / 2.0;
                entries.push(("RD", numbers(&[margin, margin, margin, margin])));
                let mut canvas = Canvas::new();
                canvas.set_opacity(opacity, opacity)?;
                canvas.set_stroke_color(stroke);
                canvas.set_line_width(width)?;
                if let Some(fill) = fill {
                    canvas.set_fill_color(*fill);
                }
                if circle {
                    canvas.ellipse(inner[0], inner[1], inner[2], inner[3])?;
                } else {
                    canvas.rectangle(
                        inner[0],
                        inner[1],
                        inner[2] - inner[0],
                        inner[3] - inner[1],
                    )?;
                }
                match (fill.is_some(), width > 0.0) {
                    (true, true) => canvas.fill_and_stroke(),
                    (true, false) => canvas.fill(),
                    (false, true) => canvas.stroke(),
                    (false, false) => canvas.end_path(),
                }
                let form = self.canvas_form(&canvas, outer);
                Ok(Built {
                    rect: outer,
                    entries,
                    form: Some(form),
                    flags: 4,
                })
            }
            AnnotationShape::Line {
                from,
                to,
                width,
                arrow,
            } => {
                finite(from)?;
                finite(to)?;
                finite(&[*width])?;
                let width = width.max(0.0);
                let head = if *arrow { (width * 4.0).max(7.0) } else { 0.0 };
                let area = around(&[*from, *to], head.max(width) + 1.0);
                entries.push(("Subtype", name("Line")));
                entries.push(("L", numbers(&[from[0], from[1], to[0], to[1]])));
                entries.push((
                    "LE",
                    Object::Array(vec![
                        name("None"),
                        name(if *arrow { "OpenArrow" } else { "None" }),
                    ]),
                ));
                entries.push(("C", numbers(&color)));
                entries.push(("BS", border_style(width)));
                let mut canvas = Canvas::new();
                canvas.set_opacity(opacity, opacity)?;
                canvas.set_stroke_color(stroke);
                canvas.set_line_width(width)?;
                canvas.set_line_cap(LineCap::Round);
                canvas.set_line_join(LineJoin::Round);
                canvas.move_to(from[0], from[1])?;
                canvas.line_to(to[0], to[1])?;
                if *arrow {
                    // Two strokes from the tip back, 25 degrees either side.
                    let angle = (from[1] - to[1]).atan2(from[0] - to[0]);
                    for turn in [0.436_f64, -0.436] {
                        canvas.move_to(to[0], to[1])?;
                        canvas.line_to(
                            to[0] + head * (angle + turn).cos(),
                            to[1] + head * (angle + turn).sin(),
                        )?;
                    }
                }
                canvas.stroke();
                let form = self.canvas_form(&canvas, area);
                Ok(Built {
                    rect: area,
                    entries,
                    form: Some(form),
                    flags: 4,
                })
            }
            AnnotationShape::Ink { strokes, width } => {
                finite(&[*width])?;
                let width = width.max(0.0);
                let points: Vec<[f64; 2]> = strokes.iter().flatten().copied().collect();
                if points.is_empty() || strokes.iter().any(Vec::is_empty) {
                    return Err(invalid());
                }
                for point in &points {
                    finite(point)?;
                }
                let area = around(&points, width + 1.0);
                entries.push(("Subtype", name("Ink")));
                entries.push((
                    "InkList",
                    Object::Array(
                        strokes
                            .iter()
                            .map(|stroke| {
                                numbers(&stroke.iter().flatten().copied().collect::<Vec<_>>())
                            })
                            .collect(),
                    ),
                ));
                entries.push(("C", numbers(&color)));
                entries.push(("BS", border_style(width)));
                let mut canvas = Canvas::new();
                canvas.set_opacity(opacity, opacity)?;
                canvas.set_stroke_color(stroke);
                canvas.set_line_width(width)?;
                canvas.set_line_cap(LineCap::Round);
                canvas.set_line_join(LineJoin::Round);
                for line in strokes {
                    for (at, point) in line.iter().enumerate() {
                        if at == 0 {
                            canvas.move_to(point[0], point[1])?;
                            // A single point is a dot.
                            if line.len() == 1 {
                                canvas.line_to(point[0], point[1])?;
                            }
                        } else {
                            canvas.line_to(point[0], point[1])?;
                        }
                    }
                }
                canvas.stroke();
                let form = self.canvas_form(&canvas, area);
                Ok(Built {
                    rect: area,
                    entries,
                    form: Some(form),
                    flags: 4,
                })
            }
            AnnotationShape::FreeText {
                rect,
                text,
                size,
                fill,
                border_width,
            } => {
                finite(rect)?;
                finite(&[*size, *border_width])?;
                let area = normalized(*rect, 0.0);
                if area[0] == area[2] || area[1] == area[3] || *size <= 0.0 {
                    return Err(invalid());
                }
                let border = border_width.max(0.0);
                let padding = 2.0 + border;
                let lines = wrap(text, *size, area[2] - area[0] - 2.0 * padding)?;
                entries.push(("Subtype", name("FreeText")));
                let [r, g, b] = color.map(num);
                entries.push((
                    "DA",
                    Object::String(PdfString::new(
                        format!("/Helv {} Tf {r} {g} {b} rg", num(*size)).into_bytes(),
                        StringFormat::Literal,
                    )),
                ));
                entries.push(("Q", Object::Integer(0)));
                entries.push(("BS", border_style(border)));
                let mut canvas = Canvas::new();
                canvas.set_opacity(opacity, opacity)?;
                if let Some(fill) = fill {
                    canvas.set_fill_color(*fill);
                    canvas.rectangle(area[0], area[1], area[2] - area[0], area[3] - area[1])?;
                    canvas.fill();
                }
                if border > 0.0 {
                    canvas.set_stroke_color(stroke);
                    canvas.set_line_width(border)?;
                    canvas.rectangle(
                        area[0] + border / 2.0,
                        area[1] + border / 2.0,
                        area[2] - area[0] - border,
                        area[3] - area[1] - border,
                    )?;
                    canvas.stroke();
                }
                canvas.set_fill_color(stroke);
                for (at, line) in lines.iter().enumerate() {
                    // The first baseline is an ascent below the top, the rest a
                    // line and a fifth apart.
                    let y = area[3] - padding - size * 0.72 - at as f64 * size * 1.2;
                    canvas.text(area[0] + padding, y, line, StandardFont::Helvetica, *size)?;
                }
                let form = self.canvas_form(&canvas, area);
                Ok(Built {
                    rect: area,
                    entries,
                    form: Some(form),
                    flags: 4,
                })
            }
        }
    }

    // A form XObject that draws what `canvas` has, in the box `area`.
    fn canvas_form(&mut self, canvas: &Canvas, area: [f64; 4]) -> ObjectId {
        let mut resources = Dictionary::new();
        if !canvas.states().is_empty() {
            let mut states = Dictionary::new();
            for &(fill, stroke) in canvas.states() {
                let mut state = Dictionary::new();
                state.insert(Name::from("Type"), name("ExtGState"));
                state.insert(Name::from("ca"), Object::Real(fill));
                state.insert(Name::from("CA"), Object::Real(stroke));
                states.insert(
                    Name::from(state_resource(fill, stroke).as_str()),
                    Object::Dictionary(state),
                );
            }
            resources.insert(Name::from("ExtGState"), Object::Dictionary(states));
        }
        if !canvas.fonts().is_empty() {
            let mut fonts = Dictionary::new();
            for &font in canvas.fonts() {
                let object = self.add(Object::Dictionary(font_dictionary(font)));
                fonts.insert(
                    Name::from(font_resource(font).as_str()),
                    Object::Reference(object),
                );
            }
            resources.insert(Name::from("Font"), Object::Dictionary(fonts));
        }
        let mut form = Dictionary::new();
        form.insert(Name::from("Type"), name("XObject"));
        form.insert(Name::from("Subtype"), name("Form"));
        form.insert(Name::from("BBox"), numbers(&area));
        form.insert(Name::from("Resources"), Object::Dictionary(resources));
        form.insert(Name::from("Filter"), name("FlateDecode"));
        self.add_stream(form, deflate_zlib(canvas.content(), 6))
    }

    /// Removes the annotations of page `index` that `filter` reaches, and the
    /// pop-up windows that go with them, and gives how many it removed. Form
    /// fields are never removed.
    ///
    /// It is a `PageNotFound` error past the last page.
    pub fn remove_annotations(&mut self, index: usize, filter: &AnnotationFilter) -> Result<usize> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        let page_id = self.page_object(index)?;
        let items = self.annotation_list(page_id)?;
        let mut removed = Vec::new();
        let mut count = 0;
        for item in &items {
            if let Some((_, dictionary)) = self.annotation_of(item)?
                && matches(&dictionary, filter)
            {
                removed.push(item.clone());
                count += 1;
                if let Some(popup) = dictionary.get("Popup") {
                    removed.push(popup.clone());
                }
            }
        }
        if removed.is_empty() {
            return Ok(0);
        }
        let kept: Vec<Object> = items.into_iter().filter(|i| !removed.contains(i)).collect();
        self.set_annotation_list(page_id, kept)?;
        Ok(count)
    }

    /// Changes the annotations of page `index` that `filter` reaches as
    /// `changes` say, and gives how many it changed. A change of text, author
    /// or subject doesn't redraw the annotation: its appearance stays.
    ///
    /// It is a `PageNotFound` error past the last page.
    pub fn update_annotations(
        &mut self,
        index: usize,
        filter: &AnnotationFilter,
        changes: &AnnotationChanges,
    ) -> Result<usize> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        let page_id = self.page_object(index)?;
        let items = self.annotation_list(page_id)?;
        let mut count = 0;
        let mut inline = items.clone();
        let mut inline_changed = false;
        for (position, item) in items.iter().enumerate() {
            let Some((_, mut dictionary)) = self.annotation_of(item)? else {
                continue;
            };
            if !matches(&dictionary, filter) {
                continue;
            }
            for (key, change) in [
                ("Contents", &changes.contents),
                ("T", &changes.author),
                ("Subj", &changes.subject),
            ] {
                match change {
                    Some(Some(text)) => {
                        dictionary.insert(Name::from(key), text_string(text));
                    }
                    Some(None) => {
                        dictionary.remove(key);
                    }
                    None => {}
                }
            }
            if let Some(hidden) = changes.hidden {
                let flags = dictionary
                    .get("F")
                    .and_then(Object::as_integer)
                    .unwrap_or(0);
                let flags = if hidden { flags | 2 } else { flags & !2 };
                dictionary.insert(Name::from("F"), Object::Integer(flags));
            }
            match item {
                Object::Reference(id) => self.set(*id, Object::Dictionary(dictionary)),
                _ => {
                    inline[position] = Object::Dictionary(dictionary);
                    inline_changed = true;
                }
            }
            count += 1;
        }
        if inline_changed {
            self.set_annotation_list(page_id, inline)?;
        }
        Ok(count)
    }

    // The dictionary an entry of `/Annots` stands for, with its subtype, if it
    // is an annotation that isn't a form field.
    fn annotation_of(&mut self, item: &Object) -> Result<Option<(String, Dictionary)>> {
        let dictionary = match item {
            Object::Dictionary(dictionary) => dictionary.clone(),
            Object::Reference(id) => match self.object(*id)? {
                Object::Dictionary(dictionary) => dictionary,
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let subtype = dictionary
            .get("Subtype")
            .and_then(Object::as_name)
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned());
        Ok(match subtype {
            Some(subtype) if subtype != "Widget" => Some((subtype, dictionary)),
            _ => None,
        })
    }

    // The form XObject that draws a text markup.
    fn markup_appearance(
        &mut self,
        kind: MarkupKind,
        quads: &[[f64; 8]],
        color: [f64; 3],
        opacity: f64,
        area: [f64; 4],
    ) -> ObjectId {
        let [r, g, b] = color.map(num);
        let mut content = String::from("/GS0 gs\n");
        match kind {
            MarkupKind::Highlight => content.push_str(&format!("{r} {g} {b} rg\n")),
            _ => content.push_str(&format!("{r} {g} {b} RG\n1 J\n1 j\n")),
        }
        for quad in quads {
            let [tlx, tly, trx, try_, blx, bly, brx, bry] = *quad;
            match kind {
                MarkupKind::Highlight => content.push_str(&format!(
                    "{} {} m\n{} {} l\n{} {} l\n{} {} l\nh\nf\n",
                    num(tlx),
                    num(tly),
                    num(trx),
                    num(try_),
                    num(brx),
                    num(bry),
                    num(blx),
                    num(bly),
                )),
                MarkupKind::Underline => {
                    let width = line_width(quad);
                    // The line sits a little above the bottom edge.
                    let (dx, dy) = lift(quad, width);
                    content.push_str(&format!(
                        "{} w\n{} {} m\n{} {} l\nS\n",
                        num(width),
                        num(blx + dx),
                        num(bly + dy),
                        num(brx + dx),
                        num(bry + dy)
                    ));
                }
                MarkupKind::StrikeOut => {
                    let width = line_width(quad);
                    content.push_str(&format!(
                        "{} w\n{} {} m\n{} {} l\nS\n",
                        num(width),
                        num((tlx + blx) / 2.0),
                        num((tly + bly) / 2.0),
                        num((trx + brx) / 2.0),
                        num((try_ + bry) / 2.0)
                    ));
                }
                MarkupKind::Squiggly => {
                    let width = (line_width(quad) * 0.8).max(0.4);
                    content.push_str(&format!("{} w\n", num(width)));
                    content.push_str(&squiggle(quad, width));
                }
            }
        }

        let mut state = Dictionary::new();
        state.insert(Name::from("Type"), name("ExtGState"));
        state.insert(Name::from("ca"), Object::Real(opacity));
        state.insert(Name::from("CA"), Object::Real(opacity));
        if kind == MarkupKind::Highlight {
            // Multiply, so the text under the band stays dark.
            state.insert(Name::from("BM"), name("Multiply"));
        }
        let mut states = Dictionary::new();
        states.insert(Name::from("GS0"), Object::Dictionary(state));
        let mut resources = Dictionary::new();
        resources.insert(Name::from("ExtGState"), Object::Dictionary(states));

        let mut form = Dictionary::new();
        form.insert(Name::from("Type"), name("XObject"));
        form.insert(Name::from("Subtype"), name("Form"));
        form.insert(Name::from("BBox"), numbers(&area));
        form.insert(Name::from("Resources"), Object::Dictionary(resources));
        form.insert(Name::from("Filter"), name("FlateDecode"));
        self.add_stream(form, deflate_zlib(content.as_bytes(), 6))
    }
}

// How the lines of a shape are drawn: a border style with a width.
fn border_style(width: f64) -> Object {
    let mut style = Dictionary::new();
    style.insert(Name::from("W"), Object::Real(width));
    style.insert(Name::from("S"), name("S"));
    Object::Dictionary(style)
}

// Whether `dictionary` is reached by every part of `filter` that is set.
fn matches(dictionary: &Dictionary, filter: &AnnotationFilter) -> bool {
    let text = |key: &str| {
        dictionary
            .get(key)
            .and_then(Object::as_string)
            .map(|s| decode_text_string(s.as_bytes()))
    };
    if !filter.subtypes.is_empty() {
        let subtype = dictionary
            .get("Subtype")
            .and_then(Object::as_name)
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned())
            .unwrap_or_default();
        if !filter.subtypes.contains(&subtype) {
            return false;
        }
    }
    if let Some(wanted) = &filter.name
        && text("NM").as_ref() != Some(wanted)
    {
        return false;
    }
    if let Some(wanted) = &filter.author
        && text("T").as_ref() != Some(wanted)
    {
        return false;
    }
    if let Some(wanted) = &filter.contents_contain
        && !text("Contents").is_some_and(|contents| contents.contains(wanted.as_str()))
    {
        return false;
    }
    true
}

// The lines of `text` in Helvetica at `size` points that fit in `width`
// points: broken at spaces, and at the end of a word that is too long.
fn wrap(text: &str, size: f64, width: f64) -> Result<Vec<String>> {
    let fits = |line: &str| text_width(StandardFont::Helvetica, size, line).map(|w| w <= width);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if fits(&candidate)? {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word wider than the box is cut where it stops fitting.
            let mut rest = word.to_string();
            while !fits(&rest)? && rest.chars().count() > 1 {
                let mut cut = rest.chars().count() - 1;
                while cut > 1 && !fits(&rest.chars().take(cut).collect::<String>())? {
                    cut -= 1;
                }
                lines.push(rest.chars().take(cut).collect());
                rest = rest.chars().skip(cut).collect();
            }
            line = rest;
        }
        lines.push(line);
    }
    Ok(lines)
}

// The box around the quadrilaterals, `margin` points larger: left, bottom,
// right, top.
fn bounding_box(quads: &[[f64; 8]], margin: f64) -> [f64; 4] {
    let mut area = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for quad in quads {
        for point in quad.as_chunks::<2>().0 {
            area[0] = area[0].min(point[0]);
            area[1] = area[1].min(point[1]);
            area[2] = area[2].max(point[0]);
            area[3] = area[3].max(point[1]);
        }
    }
    [
        area[0] - margin,
        area[1] - margin,
        area[2] + margin,
        area[3] + margin,
    ]
}

// The height of the quadrilateral: from its left side's top to its bottom.
fn height(quad: &[f64; 8]) -> f64 {
    (quad[0] - quad[4]).hypot(quad[1] - quad[5])
}

// A line a twentieth of the text's height thick, between half a point and
// one and a half.
fn line_width(quad: &[f64; 8]) -> f64 {
    (height(quad) * 0.06).clamp(0.5, 1.5)
}

// How far above the bottom edge a line of `width` goes, as a displacement
// along the quadrilateral's own up direction.
fn lift(quad: &[f64; 8], width: f64) -> (f64, f64) {
    let h = height(quad);
    if h == 0.0 {
        return (0.0, 0.0);
    }
    let (up_x, up_y) = ((quad[0] - quad[4]) / h, (quad[1] - quad[5]) / h);
    (up_x * width, up_y * width)
}

// A wavy line along the bottom edge of the quadrilateral.
fn squiggle(quad: &[f64; 8], width: f64) -> String {
    let [_, _, _, _, blx, bly, brx, bry] = *quad;
    let (dx, dy) = (brx - blx, bry - bly);
    let length = dx.hypot(dy);
    if length < 0.1 {
        return String::new();
    }
    let (ux, uy) = (dx / length, dy / length);
    // Up is perpendicular to the edge.
    let (nx, ny) = (-uy, ux);
    let amplitude = (height(quad) * 0.08).clamp(0.8, 1.6);
    let step = amplitude * 1.6;
    let mut out = String::new();
    let mut at = 0.0;
    let mut up = false;
    let base = width;
    let mut first = true;
    while at <= length + step {
        let t = at.min(length);
        let lift = base + if up { amplitude } else { 0.0 };
        let (x, y) = (blx + ux * t + nx * lift, bly + uy * t + ny * lift);
        out.push_str(&format!(
            "{} {} {}\n",
            num(x),
            num(y),
            if first { "m" } else { "l" }
        ));
        first = false;
        up = !up;
        if at >= length {
            break;
        }
        at += step;
    }
    out.push_str("S\n");
    out
}
