//! Adding annotations to pages that exist.

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::filters::deflate_zlib;
use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
use crate::writer::annotation::{AnnotationShape, MarkupKind, NewAnnotation};
use crate::writer::canvas::num;
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

impl Editor<'_, '_> {
    /// Adds `annotation` to page `index` and gives its object.
    ///
    /// The annotation is made with an appearance stream, so every reader draws
    /// it as it is drawn here, and the page keeps what it had. It is a
    /// `PageNotFound` error past the last page, an `InvalidBox` one for a
    /// markup without quadrilaterals, and a `NonFiniteNumber` one for numbers
    /// that aren't finite.
    pub fn add_annotation(&mut self, index: usize, annotation: &NewAnnotation) -> Result<ObjectId> {
        if index >= self.pages.len() {
            return Err(Error::new(ErrorKind::PageNotFound, 0));
        }
        finite(&[annotation.opacity])?;
        let page_id = self.page_object(index)?;
        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), name("Annot"));
        let color = annotation.color.map(|v| f64::from(v) / 255.0);
        let opacity = annotation.opacity.clamp(0.0, 1.0);
        match &annotation.shape {
            AnnotationShape::TextMarkup { kind, quads } => {
                if quads.is_empty() {
                    return Err(Error::new(ErrorKind::InvalidBox, 0));
                }
                for quad in quads {
                    finite(quad)?;
                }
                let area = bounding_box(quads, 1.5);
                dictionary.insert(Name::from("Subtype"), name(kind.subtype()));
                dictionary.insert(Name::from("Rect"), numbers(&area));
                dictionary.insert(
                    Name::from("QuadPoints"),
                    numbers(&quads.iter().flatten().copied().collect::<Vec<_>>()),
                );
                let form = self.markup_appearance(*kind, quads, color, opacity, area);
                let mut normal = Dictionary::new();
                normal.insert(Name::from("N"), Object::Reference(form));
                dictionary.insert(Name::from("AP"), Object::Dictionary(normal));
            }
        }
        dictionary.insert(Name::from("C"), numbers(&color));
        if opacity < 1.0 {
            dictionary.insert(Name::from("CA"), Object::Real(opacity));
        }
        // Print it.
        dictionary.insert(Name::from("F"), Object::Integer(4));
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

        // The page's list of annotations, which may be an array or the
        // reference to one.
        let mut page = self.page_dictionary(page_id)?;
        let existing = match page.get("Annots") {
            Some(Object::Array(items)) => items.clone(),
            Some(Object::Reference(list)) => {
                let list = *list;
                match self.object(list)? {
                    Object::Array(items) => items,
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        let mut items = existing;
        items.push(Object::Reference(id));
        page.insert(Name::from("Annots"), Object::Array(items));
        self.set(page_id, Object::Dictionary(page));
        Ok(id)
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
