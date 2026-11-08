use std::borrow::Cow;

use super::cmap::Code;
use super::composite::{CompositeFont, read_composite_font};
use super::simple::{SimpleFont, SimpleFontKind, read_simple_font};
use crate::base::{Diagnostics, Limits};
use crate::syntax::{Dictionary, Name};
use crate::xref::Resolver;

/// A font of any kind, as a content stream uses it: it splits strings into
/// character codes, and gives the text and width of each code.
#[derive(Debug, Clone, PartialEq)]
pub enum Font {
    Simple(SimpleFont),
    Composite(CompositeFont),
}

impl Font {
    /// Reads the font dictionary `font`, found at `offset`: a composite font if
    /// its `/Subtype` is `/Type0`, and a simple font otherwise. Problems are
    /// worked around and reported as each kind of font does, so it is only
    /// `None` if the `/Subtype` reads differently the second time.
    pub fn read(
        resolver: &mut Resolver<'_>,
        font: &Dictionary,
        limits: Limits,
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Option<Self> {
        match read_simple_font(resolver, font, limits, offset, diagnostics) {
            Some(simple) => Some(Self::Simple(simple)),
            None => read_composite_font(resolver, font, limits, offset, diagnostics)
                .map(Self::Composite),
        }
    }

    /// The PostScript name of the font, `/BaseFont`. A Type 3 font usually has
    /// none.
    pub fn base_font(&self) -> Option<&Name> {
        match self {
            Self::Simple(font) => font.base_font(),
            Self::Composite(font) => font.base_font(),
        }
    }

    /// The first code of `bytes`, and how many bytes it takes: one byte in a
    /// simple font, and as the CMap says in a composite font. `None` if
    /// `bytes` is empty.
    pub fn next_code(&self, bytes: &[u8]) -> Option<(Code, usize)> {
        match self {
            Self::Simple(_) => {
                let first = *bytes.first()?;
                Some((
                    Code {
                        value: u32::from(first),
                        length: 1,
                    },
                    1,
                ))
            }
            Self::Composite(font) => font.cmap().next_code(bytes),
        }
    }

    /// The text of `code`. `None` if the font doesn't say.
    pub fn text(&self, code: Code) -> Option<Cow<'_, str>> {
        match self {
            Self::Simple(font) => font.text(u8::try_from(code.value).ok()?),
            Self::Composite(font) => font.text(code),
        }
    }

    /// The horizontal width of `code`, in text space units: 0.5 for a glyph
    /// half as wide as the font size.
    pub fn width(&self, code: Code) -> f64 {
        match self {
            Self::Simple(font) => u8::try_from(code.value).map_or(0.0, |code| font.width(code)),
            Self::Composite(font) => font.width(code),
        }
    }

    /// Whether the font writes vertically: a composite font with a vertical
    /// CMap, like `Identity-V`.
    pub fn is_vertical(&self) -> bool {
        match self {
            Self::Simple(_) => false,
            Self::Composite(font) => font.cmap().is_vertical(),
        }
    }

    /// The font matrix, from glyph space to text space: `[0.001 0 0 0.001 0
    /// 0]` for every font but Type 3.
    pub fn font_matrix(&self) -> [f64; 6] {
        match self {
            Self::Simple(font) => font.font_matrix(),
            Self::Composite(_) => [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
        }
    }

    /// Whether the font is a Type 3 font, whose glyphs are content streams.
    pub fn is_type3(&self) -> bool {
        matches!(self, Self::Simple(font) if font.kind() == SimpleFontKind::Type3)
    }
}
