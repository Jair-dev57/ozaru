use super::arithmetic::{Contexts, Decoder, IntegerContexts};
use super::bitmap::{Bitmap, Combination};
use super::generic::{
    GenericParameters, RefinementParameters, decode_generic, decode_refinement, generic_contexts,
};
use super::text::{TextContexts, TextParameters, decode_text, symbol_code_length};
use super::{Budget, invalid};
use crate::base::Result;

/// What a symbol dictionary is decoded with (ITU-T T.88, section 6.5.2), with
/// the arithmetic decoder.
#[derive(Debug, Clone)]
pub(super) struct SymbolParameters<'s> {
    pub input: &'s [Bitmap],
    pub new_symbols: usize,
    pub exported: usize,
    pub template: u8,
    pub adaptive: Vec<(i64, i64)>,
    pub refine_aggregate: bool,
    pub refinement_template: u8,
    pub refinement_adaptive: Vec<(i64, i64)>,
}

/// The contexts a symbol dictionary uses, kept when a later dictionary says
/// it continues with them.
pub(super) struct SymbolContexts {
    dh: IntegerContexts,
    dw: IntegerContexts,
    ex: IntegerContexts,
    ai: IntegerContexts,
    rdx: IntegerContexts,
    rdy: IntegerContexts,
    generic: Contexts,
    text: TextContexts,
}

impl SymbolContexts {
    pub fn new(template: u8, refinement_template: u8, id_length: u32) -> Self {
        Self {
            dh: IntegerContexts::new(),
            dw: IntegerContexts::new(),
            ex: IntegerContexts::new(),
            ai: IntegerContexts::new(),
            rdx: IntegerContexts::new(),
            rdy: IntegerContexts::new(),
            generic: Contexts::new(generic_contexts(template)),
            text: TextContexts::new(id_length, refinement_template),
        }
    }
}

// The widest and tallest a symbol can be.
const MAX_SIDE: i64 = 1 << 16;

/// Decodes a symbol dictionary (section 6.5.5), and gives its exported
/// symbols.
pub(super) fn decode_symbols(
    parameters: &SymbolParameters<'_>,
    decoder: &mut Decoder<'_>,
    contexts: &mut SymbolContexts,
    budget: &mut Budget,
) -> Result<Vec<Bitmap>> {
    let p = parameters;
    budget.spend(p.new_symbols)?;
    let total = p.input.len() + p.new_symbols;
    // The input symbols, then the new ones as they are decoded.
    let mut all: Vec<Bitmap> = p.input.to_vec();
    let new = |all: &Vec<Bitmap>| all.len() - p.input.len();
    let mut height: i64 = 0;
    while new(&all) < p.new_symbols {
        height += contexts.dh.decode(decoder).ok_or_else(invalid)?;
        if !(0..MAX_SIDE).contains(&height) {
            return Err(invalid());
        }
        let mut width: i64 = 0;
        // The symbols of one height class, until OOB.
        while let Some(dw) = contexts.dw.decode(decoder) {
            if new(&all) >= p.new_symbols {
                return Err(invalid());
            }
            width += dw;
            if !(0..MAX_SIDE).contains(&width) {
                return Err(invalid());
            }
            let (w, h) = (width as usize, height as usize);
            let symbol = if !p.refine_aggregate {
                let generic = GenericParameters {
                    width: w,
                    height: h,
                    template: p.template,
                    typical: false,
                    adaptive: p.adaptive.clone(),
                    skip: None,
                };
                decode_generic(&generic, decoder, &mut contexts.generic, budget)?
            } else {
                let count = contexts.ai.decode(decoder).ok_or_else(invalid)?;
                let known = &all;
                if count == 1 {
                    // One symbol, refined (section 6.5.8.2.2).
                    let id = contexts.text.id.decode(decoder);
                    let rdx = contexts.rdx.decode(decoder).ok_or_else(invalid)?;
                    let rdy = contexts.rdy.decode(decoder).ok_or_else(invalid)?;
                    let reference = known.get(id).ok_or_else(invalid)?;
                    let refinement = RefinementParameters {
                        width: w,
                        height: h,
                        template: p.refinement_template,
                        reference,
                        dx: rdx,
                        dy: rdy,
                        typical: false,
                        adaptive: p.refinement_adaptive.clone(),
                    };
                    decode_refinement(&refinement, decoder, &mut contexts.text.refinement, budget)?
                } else {
                    // Several symbols placed as a text region (section
                    // 6.5.8.2.1).
                    let instances = usize::try_from(count).map_err(|_| invalid())?;
                    let text = TextParameters {
                        width: w,
                        height: h,
                        instances,
                        strips: 1,
                        symbols: known,
                        default_pixel: false,
                        combination: Combination::Or,
                        transposed: false,
                        corner: 1,
                        s_offset: 0,
                        refine: true,
                        refinement_template: p.refinement_template,
                        refinement_adaptive: p.refinement_adaptive.clone(),
                    };
                    decode_text(&text, decoder, &mut contexts.text, budget)?
                }
            };
            all.push(symbol);
        }
    }

    // Which symbols are exported: runs of not exported and exported, in
    // turns, over the input symbols and the new ones (section 6.5.10).
    let mut exported = Vec::with_capacity(p.exported.min(total));
    let mut index = 0;
    let mut export = false;
    while index < total {
        let run = contexts.ex.decode(decoder).ok_or_else(invalid)?;
        let run = usize::try_from(run).map_err(|_| invalid())?;
        if run > total - index {
            return Err(invalid());
        }
        if export {
            exported.extend_from_slice(&all[index..index + run]);
        }
        index += run;
        export = !export;
        budget.spend(1)?;
    }
    Ok(exported)
}

/// The length of the symbol numbers of a dictionary with `input` symbols in
/// and `new` new ones.
pub(super) fn dictionary_code_length(input: usize, new: usize) -> u32 {
    symbol_code_length(input + new)
}
