//! Annotations to add to pages (ISO 32000-2, section 12.5): what they say and
//! how they look, ready for [`Editor::add_annotation`](super::Editor::add_annotation).

use crate::document::Date;

/// How text is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    /// A translucent band over the text, as a highlighter makes.
    Highlight,
    /// A line under the text.
    Underline,
    /// A line through the text.
    StrikeOut,
    /// A wavy line under the text.
    Squiggly,
}

impl MarkupKind {
    /// The `/Subtype` of the annotation.
    pub fn subtype(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::StrikeOut => "StrikeOut",
            Self::Squiggly => "Squiggly",
        }
    }
}

/// Where a link goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A web address or other URI.
    Uri(String),
    /// A page of this document, counted from 0.
    Page(usize),
}

/// What an annotation is. Rectangles are `[left, bottom, right, top]` and
/// points `[x, y]`, in default user space.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationShape {
    /// Marks text: one quadrilateral for each run of text, with its corners
    /// as the standard orders them in practice: top left, top right, bottom
    /// left, bottom right, each as `x, y`.
    TextMarkup {
        kind: MarkupKind,
        quads: Vec<[f64; 8]>,
    },
    /// A sticky note: a small icon with its top left corner at `(x, y)`
    /// whose text (the `contents`) opens when it is clicked.
    Note { x: f64, y: f64, open: bool },
    /// An area that goes somewhere when clicked. The `color` and a
    /// `border_width` above 0 draw a frame around it.
    Link {
        rect: [f64; 4],
        target: LinkTarget,
        border_width: f64,
    },
    /// A rectangle, outlined in the `color`, filled with `fill` if given.
    Square {
        rect: [f64; 4],
        fill: Option<[u8; 3]>,
        width: f64,
    },
    /// An ellipse inside a rectangle.
    Circle {
        rect: [f64; 4],
        fill: Option<[u8; 3]>,
        width: f64,
    },
    /// A line, with an arrowhead at its end if asked.
    Line {
        from: [f64; 2],
        to: [f64; 2],
        width: f64,
        arrow: bool,
    },
    /// Freehand strokes, each a list of points joined by lines.
    Ink {
        strokes: Vec<Vec<[f64; 2]>>,
        width: f64,
    },
    /// Text in a box, in Helvetica, wrapped to the box; the `color` is the
    /// color of the text. It is an `UnsupportedCharacter` error if the
    /// text has a character Helvetica doesn't.
    FreeText {
        rect: [f64; 4],
        text: String,
        size: f64,
        fill: Option<[u8; 3]>,
        border_width: f64,
    },
}

/// An annotation to add to a page.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAnnotation {
    pub shape: AnnotationShape,
    /// Red, green and blue from 0 to 255.
    pub color: [u8; 3],
    /// From 0 (invisible) to 1.
    pub opacity: f64,
    /// The text of the note that goes with it, shown when it is opened.
    pub contents: Option<String>,
    /// Who made it.
    pub author: Option<String>,
    pub subject: Option<String>,
    /// When it was made.
    pub date: Option<Date>,
}

impl NewAnnotation {
    /// An annotation of `shape` with the defaults: red, solid, no text.
    pub fn new(shape: AnnotationShape) -> Self {
        Self {
            shape,
            color: [255, 0, 0],
            opacity: 1.0,
            contents: None,
            author: None,
            subject: None,
            date: None,
        }
    }

    /// A text markup of `kind` over `quads`, with the color a highlighter has
    /// (yellow) for a highlight, and red for the other kinds.
    pub fn text_markup(kind: MarkupKind, quads: Vec<[f64; 8]>) -> Self {
        Self {
            shape: AnnotationShape::TextMarkup { kind, quads },
            color: if kind == MarkupKind::Highlight {
                [255, 255, 0]
            } else {
                [255, 0, 0]
            },
            opacity: 1.0,
            contents: None,
            author: None,
            subject: None,
            date: None,
        }
    }
}

/// Which annotations of a page a change or a removal reaches: those that
/// match every part that is set. Form fields (`Widget`) are never reached,
/// and a filter with nothing set reaches all the others.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnotationFilter {
    /// Any of these subtypes (`Highlight`, `Link`...); any if empty.
    pub subtypes: Vec<String>,
    /// The name (`/NM`) of the annotation.
    pub name: Option<String>,
    /// The author (`/T`).
    pub author: Option<String>,
    /// Text its `/Contents` have.
    pub contents_contain: Option<String>,
}

/// What [`Editor::update_annotations`](super::Editor::update_annotations)
/// changes. `None` leaves a part as it is; `Some(None)` removes a text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotationChanges {
    pub contents: Option<Option<String>>,
    pub author: Option<Option<String>>,
    pub subject: Option<Option<String>>,
    /// Hide the annotation or show it again.
    pub hidden: Option<bool>,
}
