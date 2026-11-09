use super::StandardFont;
use super::resolved;
use crate::syntax::{Dictionary, Object};
use crate::xref::Resolver;

/// How far the glyphs of a font reach above and below the baseline, in text
/// space units: 0.7 for a font whose capitals are 0.7 of the font size tall.
/// It makes the box of each glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    /// How far above the baseline, at least 0.
    pub ascent: f64,
    /// How far below the baseline, as a negative number or 0.
    pub descent: f64,
}

impl Extent {
    /// What a font that says nothing gets: about what most text fonts have.
    pub const DEFAULT: Self = Self {
        ascent: 0.75,
        descent: -0.25,
    };

    // An extent whose height is between a fifth of an em and three ems, the
    // range real fonts are in. Other values are mistakes, like `/Ascent 0`.
    fn checked(ascent: f64, descent: f64) -> Option<Self> {
        let usable = ascent.is_finite()
            && descent.is_finite()
            && ascent > 0.0
            && descent <= 0.0
            && (0.2..=3.0).contains(&(ascent - descent));
        usable.then_some(Self { ascent, descent })
    }
}

/// The extent of a font with the descriptor `descriptor`: its `/Ascent` and
/// `/Descent` (ISO 32000-2, table 120), or else the ascender and descender of
/// the standard font `standard`, or else the top and bottom of its
/// `/FontBBox`, or else [`Extent::DEFAULT`]. `scale` turns glyph space into
/// text space: 0.001, or the vertical scale of a Type 3 font's matrix.
pub(crate) fn read_extent(
    resolver: &mut Resolver<'_>,
    descriptor: Option<&Dictionary>,
    standard: Option<StandardFont>,
    scale: f64,
) -> Extent {
    let scaled = |ascent: f64, descent: f64| {
        let (top, bottom) = (ascent * scale, descent * scale);
        Extent::checked(top.max(bottom), top.min(bottom))
    };
    let mut number = |key: &str| {
        let entry = descriptor?.get(key)?;
        resolved(resolver, entry).as_number()
    };
    let (ascent, descent) = (number("Ascent"), number("Descent"));
    if let (Some(ascent), Some(descent)) = (ascent, descent)
        && let Some(extent) = scaled(ascent, descent)
    {
        return extent;
    }
    if let Some((ascent, descent)) = standard.map(StandardFont::ascender_descender)
        && let Some(extent) = scaled(ascent, descent)
    {
        return extent;
    }
    let bbox = descriptor
        .and_then(|descriptor| descriptor.get("FontBBox"))
        .map(|entry| resolved(resolver, entry));
    let corners: Option<Vec<f64>> = bbox
        .as_deref()
        .and_then(Object::as_array)
        .filter(|items| items.len() == 4)
        .map(|items| items.iter().filter_map(Object::as_number).collect());
    if let Some([_, bottom, _, top]) = corners.as_deref()
        && let Some(extent) = scaled(*top, *bottom)
    {
        return extent;
    }
    Extent::DEFAULT
}
