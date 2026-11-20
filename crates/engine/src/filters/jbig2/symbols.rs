use super::arithmetic::{Contexts, Decoder, IntegerContexts};
use super::bitmap::{Bitmap, Combination};
use super::generic::decode_mmr;
use super::generic::{
    GenericParameters, RefinementParameters, decode_generic, decode_refinement, generic_contexts,
};
use super::huffman::{Bits, Table, standard};
use super::text::{TextContexts, TextParameters, TextSource, decode_text, symbol_code_length};
use super::{Budget, invalid};
use crate::base::Limits;
use crate::base::{Error, ErrorKind, Result};

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
                    let mut source = TextSource::Arithmetic {
                        decoder,
                        contexts: &mut contexts.text,
                    };
                    decode_text(&text, &mut source, budget)?
                }
            };
            all.push(symbol);
        }
    }

    exported_symbols(&all, p.exported, budget, || {
        contexts.ex.decode(decoder).ok_or_else(invalid)
    })
}

/// The length of the symbol numbers of a dictionary with `input` symbols in
/// and `new` new ones.
pub(super) fn dictionary_code_length(input: usize, new: usize) -> u32 {
    symbol_code_length(input + new)
}

// Which symbols of `all`, the input symbols and the new ones, are exported:
// runs of not exported and exported, in turns, given by `run` (section
// 6.5.10).
fn exported_symbols(
    all: &[Bitmap],
    exported: usize,
    budget: &mut Budget,
    mut run: impl FnMut() -> Result<i64>,
) -> Result<Vec<Bitmap>> {
    let total = all.len();
    let mut out = Vec::with_capacity(exported.min(total));
    let mut index = 0;
    let mut export = false;
    while index < total {
        let length = usize::try_from(run()?).map_err(|_| invalid())?;
        if length > total - index {
            return Err(invalid());
        }
        if export {
            out.extend_from_slice(&all[index..index + length]);
        }
        index += length;
        export = !export;
        budget.spend(1)?;
    }
    Ok(out)
}

/// The Huffman tables of a symbol dictionary (section 7.4.2.1.6).
pub(super) struct SymbolTables {
    pub dh: Table,
    pub dw: Table,
    pub bitmap_size: Table,
}

/// Decodes a symbol dictionary coded with Huffman tables (section 6.5.5):
/// the symbols of each height class come as one bitmap, uncompressed or
/// coded with MMR, cut by their widths. Refinement and aggregation, rare
/// with Huffman tables, aren't read yet.
pub(super) fn decode_symbols_huffman(
    parameters: &SymbolParameters<'_>,
    bits: &mut Bits<'_>,
    tables: &SymbolTables,
    limits: Limits,
    budget: &mut Budget,
) -> Result<Vec<Bitmap>> {
    let p = parameters;
    if p.refine_aggregate {
        return Err(Error::new(ErrorKind::UnsupportedFilter, 0));
    }
    budget.spend(p.new_symbols)?;
    let mut all: Vec<Bitmap> = p.input.to_vec();
    let new = |all: &Vec<Bitmap>| all.len() - p.input.len();
    let mut height: i64 = 0;
    while new(&all) < p.new_symbols {
        height += tables.dh.decode(bits)?.ok_or_else(invalid)?;
        if !(0..MAX_SIDE).contains(&height) {
            return Err(invalid());
        }
        let mut width: i64 = 0;
        let mut widths = Vec::new();
        while let Some(dw) = tables.dw.decode(bits)? {
            if new(&all) + widths.len() >= p.new_symbols {
                return Err(invalid());
            }
            width += dw;
            if !(0..MAX_SIDE).contains(&width) {
                return Err(invalid());
            }
            widths.push(width as usize);
        }
        let total_width: usize = widths.iter().sum();
        let h = height as usize;
        budget.spend(total_width.saturating_mul(h).saturating_add(h))?;
        let size = tables.bitmap_size.decode(bits)?.ok_or_else(invalid)?;
        let size = usize::try_from(size).map_err(|_| invalid())?;
        bits.align();
        // The collective bitmap of the class: uncompressed rows, or MMR.
        let collective = if size == 0 {
            let stride = total_width.div_ceil(8);
            let length = stride.saturating_mul(h);
            let data = bits.rest().get(..length).ok_or_else(invalid)?.to_vec();
            bits.skip(length);
            Bitmap::from_rows(total_width, h, data)
        } else {
            let data = bits.rest().get(..size).ok_or_else(invalid)?;
            let bitmap = decode_mmr(data, total_width, h, limits, budget)?;
            bits.skip(size);
            bitmap
        };
        let mut x = 0;
        for w in widths {
            all.push(collective.cut(x as i64, 0, w, h));
            x += w;
        }
    }
    let ex = standard(1);
    exported_symbols(&all, p.exported, budget, || {
        ex.decode(bits)?.ok_or_else(invalid)
    })
}
