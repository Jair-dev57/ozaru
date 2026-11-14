use super::arithmetic::{Contexts, Decoder, IdContexts, IntegerContexts};
use super::bitmap::{Bitmap, Combination};
use super::generic::{RefinementParameters, decode_refinement, refinement_contexts};
use super::{Budget, invalid};
use crate::base::Result;

/// What a text region is decoded with (ITU-T T.88, section 6.4.2).
#[derive(Debug, Clone)]
pub(super) struct TextParameters<'s> {
    pub width: usize,
    pub height: usize,
    pub instances: usize,
    pub strips: i64,
    pub symbols: &'s [Bitmap],
    pub default_pixel: bool,
    pub combination: Combination,
    pub transposed: bool,
    /// REFCORNER: 0 bottom left, 1 top left, 2 bottom right, 3 top right.
    pub corner: u8,
    pub s_offset: i64,
    pub refine: bool,
    pub refinement_template: u8,
    pub refinement_adaptive: Vec<(i64, i64)>,
}

/// The decoding procedures of text regions, and of the refinements in them,
/// which a symbol dictionary keeps from one symbol to the next.
pub(super) struct TextContexts {
    dt: IntegerContexts,
    fs: IntegerContexts,
    ds: IntegerContexts,
    it: IntegerContexts,
    ri: IntegerContexts,
    rdw: IntegerContexts,
    rdh: IntegerContexts,
    rdx: IntegerContexts,
    rdy: IntegerContexts,
    pub id: IdContexts,
    pub refinement: Contexts,
}

impl TextContexts {
    pub fn new(id_length: u32, refinement_template: u8) -> Self {
        Self {
            dt: IntegerContexts::new(),
            fs: IntegerContexts::new(),
            ds: IntegerContexts::new(),
            it: IntegerContexts::new(),
            ri: IntegerContexts::new(),
            rdw: IntegerContexts::new(),
            rdh: IntegerContexts::new(),
            rdx: IntegerContexts::new(),
            rdy: IntegerContexts::new(),
            id: IdContexts::new(id_length),
            refinement: Contexts::new(refinement_contexts(refinement_template)),
        }
    }
}

/// How many bits a symbol number of `count` symbols takes: SBSYMCODELEN.
pub(super) fn symbol_code_length(count: usize) -> u32 {
    let mut length = 0;
    while (1usize << length) < count {
        length += 1;
    }
    length
}

/// Decodes a text region with the arithmetic decoder (section 6.4.5).
pub(super) fn decode_text(
    parameters: &TextParameters<'_>,
    decoder: &mut Decoder<'_>,
    contexts: &mut TextContexts,
    budget: &mut Budget,
) -> Result<Bitmap> {
    let p = parameters;
    budget.spend(p.width.saturating_mul(p.height).saturating_add(p.height))?;
    let mut region = Bitmap::new(p.width, p.height, p.default_pixel);
    let oob = invalid;
    let mut strip_t = -contexts.dt.decode(decoder).ok_or_else(oob)? * p.strips;
    let mut first_s: i64 = 0;
    let mut instances = 0;
    while instances < p.instances {
        strip_t += contexts.dt.decode(decoder).ok_or_else(oob)? * p.strips;
        let mut current_s: Option<i64> = None;
        // The symbols of one strip, until OOB.
        loop {
            budget.spend(1)?;
            let s = match current_s {
                None => {
                    first_s += contexts.fs.decode(decoder).ok_or_else(oob)?;
                    first_s
                }
                Some(s) => match contexts.ds.decode(decoder) {
                    Some(ds) => s + ds + p.s_offset,
                    None => break,
                },
            };
            let t_in_strip = if p.strips == 1 {
                0
            } else {
                contexts.it.decode(decoder).ok_or_else(oob)?
            };
            let t = strip_t + t_in_strip;
            let id = contexts.id.decode(decoder);
            let symbol = p.symbols.get(id).ok_or_else(invalid)?;
            let refined = p.refine && contexts.ri.decode(decoder).ok_or_else(oob)? != 0;
            let refined_symbol;
            let symbol = if refined {
                let rdw = contexts.rdw.decode(decoder).ok_or_else(oob)?;
                let rdh = contexts.rdh.decode(decoder).ok_or_else(oob)?;
                let rdx = contexts.rdx.decode(decoder).ok_or_else(oob)?;
                let rdy = contexts.rdy.decode(decoder).ok_or_else(oob)?;
                let width = usize::try_from(symbol.width as i64 + rdw).map_err(|_| invalid())?;
                let height = usize::try_from(symbol.height as i64 + rdh).map_err(|_| invalid())?;
                let refinement = RefinementParameters {
                    width,
                    height,
                    template: p.refinement_template,
                    reference: symbol,
                    dx: rdw.div_euclid(2) + rdx,
                    dy: rdh.div_euclid(2) + rdy,
                    typical: false,
                    adaptive: p.refinement_adaptive.clone(),
                };
                refined_symbol =
                    decode_refinement(&refinement, decoder, &mut contexts.refinement, budget)?;
                &refined_symbol
            } else {
                symbol
            };
            let (w, h) = (symbol.width as i64, symbol.height as i64);
            // Where the symbol goes, from the reference corner (section
            // 6.4.5, steps 3 c x and xi).
            let mut s = s;
            let right = matches!(p.corner, 2 | 3);
            let bottom = matches!(p.corner, 0 | 2);
            if !p.transposed && right {
                s += w - 1;
            } else if p.transposed && bottom {
                s += h - 1;
            }
            let (x, y) = if p.transposed {
                (
                    if right { t - w + 1 } else { t },
                    if bottom { s - h + 1 } else { s },
                )
            } else {
                (
                    if right { s - w + 1 } else { s },
                    if bottom { t - h + 1 } else { t },
                )
            };
            // Drawing goes pixel by pixel.
            budget.spend(symbol.width.saturating_mul(symbol.height).saturating_add(1))?;
            region.draw(symbol, x, y, p.combination);
            if !p.transposed && !right {
                s += w - 1;
            } else if p.transposed && !bottom {
                s += h - 1;
            }
            current_s = Some(s);
            instances += 1;
        }
    }
    Ok(region)
}
