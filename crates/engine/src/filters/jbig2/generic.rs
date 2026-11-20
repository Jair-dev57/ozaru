use super::arithmetic::{Contexts, Decoder};
use super::bitmap::Bitmap;
use super::{Budget, invalid};
use crate::base::{Limits, Result};
use crate::filters::{CcittParameters, decode_ccitt};

/// What a generic region is decoded with (ITU-T T.88, section 6.2.2).
#[derive(Debug, Clone)]
pub(super) struct GenericParameters {
    pub width: usize,
    pub height: usize,
    pub template: u8,
    /// Typical prediction: rows the same as the one above are marked.
    pub typical: bool,
    /// The adaptive pixels, as offsets from the pixel being decoded.
    pub adaptive: Vec<(i64, i64)>,
    /// Pixels to leave white without decoding, as halftones use: `None` for
    /// none.
    pub skip: Option<Bitmap>,
}

/// How many contexts each template has.
pub(super) fn generic_contexts(template: u8) -> usize {
    match template {
        0 => 1 << 16,
        1 => 1 << 13,
        _ => 1 << 10,
    }
}

/// Decodes a generic region with the arithmetic decoder (section 6.2.5),
/// with `contexts` of [`generic_contexts`].
pub(super) fn decode_generic(
    parameters: &GenericParameters,
    decoder: &mut Decoder<'_>,
    contexts: &mut Contexts,
    budget: &mut Budget,
) -> Result<Bitmap> {
    let (width, height) = (parameters.width, parameters.height);
    budget.spend(width.saturating_mul(height).saturating_add(height))?;
    let mut bitmap = Bitmap::new(width, height, false);
    let at = |i: usize, default: (i64, i64)| parameters.adaptive.get(i).copied().unwrap_or(default);
    // The typical prediction contexts of each template (section 6.2.5.7).
    let typical_context = match parameters.template {
        0 => 0x9B25,
        1 => 0x0795,
        2 => 0x00E5,
        _ => 0x0195,
    };
    let mut typical = false;
    for y in 0..height {
        if parameters.typical {
            typical ^= decoder.decode(contexts, typical_context) == 1;
            if typical {
                if y > 0 {
                    bitmap.copy_row(y - 1, y);
                }
                continue;
            }
        }
        let y = y as i64;
        for x in 0..width {
            let xi = x as i64;
            if let Some(skip) = &parameters.skip
                && skip.get(xi, y) == 1
            {
                continue;
            }
            let p = |dx: i64, dy: i64| bitmap.get(xi + dx, y + dy);
            let context = match parameters.template {
                0 => {
                    let (a1, a2, a3, a4) = (
                        at(0, (3, -1)),
                        at(1, (-3, -1)),
                        at(2, (2, -2)),
                        at(3, (-2, -2)),
                    );
                    p(-1, 0)
                        | p(-2, 0) << 1
                        | p(-3, 0) << 2
                        | p(-4, 0) << 3
                        | p(a1.0, a1.1) << 4
                        | p(2, -1) << 5
                        | p(1, -1) << 6
                        | p(0, -1) << 7
                        | p(-1, -1) << 8
                        | p(-2, -1) << 9
                        | p(a2.0, a2.1) << 10
                        | p(a3.0, a3.1) << 11
                        | p(1, -2) << 12
                        | p(0, -2) << 13
                        | p(-1, -2) << 14
                        | p(a4.0, a4.1) << 15
                }
                1 => {
                    let a1 = at(0, (3, -1));
                    p(-1, 0)
                        | p(-2, 0) << 1
                        | p(-3, 0) << 2
                        | p(a1.0, a1.1) << 3
                        | p(2, -1) << 4
                        | p(1, -1) << 5
                        | p(0, -1) << 6
                        | p(-1, -1) << 7
                        | p(-2, -1) << 8
                        | p(2, -2) << 9
                        | p(1, -2) << 10
                        | p(0, -2) << 11
                        | p(-1, -2) << 12
                }
                2 => {
                    let a1 = at(0, (2, -1));
                    p(-1, 0)
                        | p(-2, 0) << 1
                        | p(a1.0, a1.1) << 2
                        | p(1, -1) << 3
                        | p(0, -1) << 4
                        | p(-1, -1) << 5
                        | p(-2, -1) << 6
                        | p(1, -2) << 7
                        | p(0, -2) << 8
                        | p(-1, -2) << 9
                }
                _ => {
                    let a1 = at(0, (2, -1));
                    p(-1, 0)
                        | p(-2, 0) << 1
                        | p(-3, 0) << 2
                        | p(-4, 0) << 3
                        | p(a1.0, a1.1) << 4
                        | p(1, -1) << 5
                        | p(0, -1) << 6
                        | p(-1, -1) << 7
                        | p(-2, -1) << 8
                        | p(-3, -1) << 9
                }
            };
            if decoder.decode(contexts, context as usize) == 1 {
                bitmap.set(x, y as usize, true);
            }
        }
    }
    Ok(bitmap)
}

/// Decodes a generic region coded with MMR, the Group 4 fax encoding
/// (section 6.2.6), and gives the bytes it used.
pub(super) fn decode_mmr(
    data: &[u8],
    width: usize,
    height: usize,
    limits: Limits,
    budget: &mut Budget,
) -> Result<Bitmap> {
    budget.spend(width.saturating_mul(height).saturating_add(height))?;
    if width == 0 || height == 0 {
        return Ok(Bitmap::new(width, height, false));
    }
    let parameters = CcittParameters {
        k: -1,
        encoded_byte_align: false,
        columns: width,
        rows: height,
        black_is_1: true,
    };
    let decoded = decode_ccitt(data, &parameters, 0, limits).map_err(|_| invalid())?;
    Ok(Bitmap::from_rows(width, height, decoded.data))
}

/// What a refinement region is decoded with (section 6.3.2).
#[derive(Debug, Clone)]
pub(super) struct RefinementParameters<'r> {
    pub width: usize,
    pub height: usize,
    pub template: u8,
    pub reference: &'r Bitmap,
    /// Where the reference is, from the region: GRREFERENCEDX and DY.
    pub dx: i64,
    pub dy: i64,
    pub typical: bool,
    pub adaptive: Vec<(i64, i64)>,
}

/// How many contexts each refinement template has.
pub(super) fn refinement_contexts(template: u8) -> usize {
    if template == 0 { 1 << 13 } else { 1 << 10 }
}

/// Decodes a refinement region (section 6.3.5): each pixel from its
/// neighbors and from the reference bitmap around the same place.
pub(super) fn decode_refinement(
    parameters: &RefinementParameters<'_>,
    decoder: &mut Decoder<'_>,
    contexts: &mut Contexts,
    budget: &mut Budget,
) -> Result<Bitmap> {
    let (width, height) = (parameters.width, parameters.height);
    budget.spend(width.saturating_mul(height).saturating_add(height))?;
    let reference = parameters.reference;
    let (dx, dy) = (parameters.dx, parameters.dy);
    let mut bitmap = Bitmap::new(width, height, false);
    let at = |i: usize, default: (i64, i64)| parameters.adaptive.get(i).copied().unwrap_or(default);
    let (a1, a2) = (at(0, (-1, -1)), at(1, (-1, -1)));
    let typical_context = if parameters.template == 0 {
        0x0100
    } else {
        0x0080
    };
    let mut typical = false;
    for y in 0..height {
        if parameters.typical {
            typical ^= decoder.decode(contexts, typical_context) == 1;
        }
        let yi = y as i64;
        for x in 0..width {
            let xi = x as i64;
            let c = |ox: i64, oy: i64| bitmap.get(xi + ox, yi + oy);
            let r = |ox: i64, oy: i64| reference.get(xi - dx + ox, yi - dy + oy);
            // Typical prediction: where the reference is all one value around
            // the pixel, the pixel has it (section 6.3.5.6).
            if typical {
                let around = r(-1, -1)
                    + r(0, -1)
                    + r(1, -1)
                    + r(-1, 0)
                    + r(0, 0)
                    + r(1, 0)
                    + r(-1, 1)
                    + r(0, 1)
                    + r(1, 1);
                if around == 0 {
                    continue;
                }
                if around == 9 {
                    bitmap.set(x, y, true);
                    continue;
                }
            }
            let context = if parameters.template == 0 {
                c(-1, 0)
                    | c(1, -1) << 1
                    | c(0, -1) << 2
                    | c(a1.0, a1.1) << 3
                    | r(1, 1) << 4
                    | r(0, 1) << 5
                    | r(-1, 1) << 6
                    | r(1, 0) << 7
                    | r(0, 0) << 8
                    | r(-1, 0) << 9
                    | r(1, -1) << 10
                    | r(0, -1) << 11
                    | r(a2.0, a2.1) << 12
            } else {
                c(-1, 0)
                    | c(1, -1) << 1
                    | c(0, -1) << 2
                    | c(-1, -1) << 3
                    | r(1, 1) << 4
                    | r(0, 1) << 5
                    | r(1, 0) << 6
                    | r(0, 0) << 7
                    | r(-1, 0) << 8
                    | r(0, -1) << 9
            };
            if decoder.decode(contexts, context as usize) == 1 {
                bitmap.set(x, y, true);
            }
        }
    }
    Ok(bitmap)
}
