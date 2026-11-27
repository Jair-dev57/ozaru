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

/// What an annotation is.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationShape {
    /// Marks text: one quadrilateral for each run of text, with its corners
    /// as the standard orders them in practice: top left, top right, bottom
    /// left, bottom right, each as `x, y` in default user space.
    TextMarkup {
        kind: MarkupKind,
        quads: Vec<[f64; 8]>,
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
