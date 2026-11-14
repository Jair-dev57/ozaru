use super::arithmetic::{Contexts, Decoder, IdContexts, IntegerContexts};
use super::bitmap::{Bitmap, Combination};
use super::generic::{RefinementParameters, decode_refinement, refinement_contexts};
use super::huffman::{Bits, Table};
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

/// The Huffman tables of a text region (section 7.4.3.1.6) and its table of
/// symbol numbers.
pub(super) struct TextTables {
    pub fs: Table,
    pub ds: Table,
    pub dt: Table,
    pub rdw: Table,
    pub rdh: Table,
    pub rdx: Table,
    pub rdy: Table,
    pub rsize: Table,
    pub ids: Table,
}

/// How many bits a symbol number of `count` symbols takes: SBSYMCODELEN.
pub(super) fn symbol_code_length(count: usize) -> u32 {
    let mut length = 0;
    while (1usize << length) < count {
        length += 1;
    }
    length
}

/// Reads the code lengths of the symbol numbers of a text region coded with
/// Huffman tables, and makes their table (section 7.4.3.1.7). The reading
/// ends on a byte.
pub(super) fn read_symbol_codes(bits: &mut Bits<'_>, count: usize) -> Result<Table> {
    let mut run_lengths = [0u32; 35];
    for length in &mut run_lengths {
        *length = bits.bits(4)?;
    }
    let runs = Table::of_lengths(&run_lengths);
    let mut lengths: Vec<u32> = Vec::with_capacity(count.min(1 << 20));
    while lengths.len() < count {
        let code = runs.decode(bits)?.ok_or_else(invalid)?;
        let (value, times) = match code {
            0..=31 => (code as u32, 1),
            32 => (
                *lengths.last().ok_or_else(invalid)?,
                3 + bits.bits(2)? as usize,
            ),
            33 => (0, 3 + bits.bits(3)? as usize),
            34 => (0, 11 + bits.bits(7)? as usize),
            _ => return Err(invalid()),
        };
        if lengths.len() + times > count {
            return Err(invalid());
        }
        lengths.extend(std::iter::repeat_n(value, times));
    }
    bits.align();
    Ok(Table::of_lengths(&lengths))
}

/// Where the numbers of a text region come from: the arithmetic decoder, or
/// Huffman tables.
pub(super) enum TextSource<'d, 'a> {
    Arithmetic {
        decoder: &'d mut Decoder<'a>,
        contexts: &'d mut TextContexts,
    },
    Huffman {
        bits: &'d mut Bits<'a>,
        tables: &'d TextTables,
        /// The contexts of the refinements, which are coded with the
        /// arithmetic coder.
        refinement: Contexts,
    },
}

#[derive(Clone, Copy)]
enum Which {
    Dt,
    Fs,
    Ds,
    Rdw,
    Rdh,
    Rdx,
    Rdy,
}

impl TextSource<'_, '_> {
    // An integer, or `None` for OOB.
    fn integer(&mut self, which: Which) -> Result<Option<i64>> {
        match self {
            Self::Arithmetic { decoder, contexts } => {
                let procedure = match which {
                    Which::Dt => &mut contexts.dt,
                    Which::Fs => &mut contexts.fs,
                    Which::Ds => &mut contexts.ds,
                    Which::Rdw => &mut contexts.rdw,
                    Which::Rdh => &mut contexts.rdh,
                    Which::Rdx => &mut contexts.rdx,
                    Which::Rdy => &mut contexts.rdy,
                };
                Ok(procedure.decode(decoder))
            }
            Self::Huffman { bits, tables, .. } => {
                let table = match which {
                    Which::Dt => &tables.dt,
                    Which::Fs => &tables.fs,
                    Which::Ds => &tables.ds,
                    Which::Rdw => &tables.rdw,
                    Which::Rdh => &tables.rdh,
                    Which::Rdx => &tables.rdx,
                    Which::Rdy => &tables.rdy,
                };
                table.decode(bits)
            }
        }
    }

    // An integer that can't be OOB.
    fn value(&mut self, which: Which) -> Result<i64> {
        self.integer(which)?.ok_or_else(invalid)
    }
}

/// Decodes a text region (section 6.4.5), with the arithmetic decoder or
/// Huffman tables.
pub(super) fn decode_text(
    parameters: &TextParameters<'_>,
    source: &mut TextSource<'_, '_>,
    budget: &mut Budget,
) -> Result<Bitmap> {
    let p = parameters;
    budget.spend(p.width.saturating_mul(p.height).saturating_add(p.height))?;
    let mut region = Bitmap::new(p.width, p.height, p.default_pixel);
    let strip_bits = p.strips.trailing_zeros();
    let mut strip_t = -source.value(Which::Dt)? * p.strips;
    let mut first_s: i64 = 0;
    let mut instances = 0;
    while instances < p.instances {
        strip_t += source.value(Which::Dt)? * p.strips;
        let mut current_s: Option<i64> = None;
        // The symbols of one strip, until OOB.
        loop {
            budget.spend(1)?;
            let s = match current_s {
                None => {
                    first_s += source.value(Which::Fs)?;
                    first_s
                }
                Some(s) => match source.integer(Which::Ds)? {
                    Some(ds) => s + ds + p.s_offset,
                    None => break,
                },
            };
            let t_in_strip = if p.strips == 1 {
                0
            } else {
                match source {
                    TextSource::Arithmetic { decoder, contexts } => {
                        contexts.it.decode(decoder).ok_or_else(invalid)?
                    }
                    TextSource::Huffman { bits, .. } => i64::from(bits.bits(strip_bits)?),
                }
            };
            let t = strip_t + t_in_strip;
            let (id, refined) = match source {
                TextSource::Arithmetic { decoder, contexts } => {
                    let id = contexts.id.decode(decoder);
                    let refined = p.refine && contexts.ri.decode(decoder).ok_or_else(invalid)? != 0;
                    (id, refined)
                }
                TextSource::Huffman { bits, tables, .. } => {
                    let id = tables.ids.decode(bits)?.ok_or_else(invalid)?;
                    let refined = p.refine && bits.bit()? == 1;
                    (usize::try_from(id).map_err(|_| invalid())?, refined)
                }
            };
            let symbol = p.symbols.get(id).ok_or_else(invalid)?;
            let refined_symbol;
            let symbol = if refined {
                refined_symbol = refine(p, symbol, source, budget)?;
                &refined_symbol
            } else {
                symbol
            };
            // Drawing goes pixel by pixel.
            budget.spend(symbol.width.saturating_mul(symbol.height).saturating_add(1))?;
            current_s = Some(place(&mut region, symbol, s, t, p));
            instances += 1;
        }
    }
    Ok(region)
}

// A symbol refined where it is placed (section 6.4.11).
fn refine(
    p: &TextParameters<'_>,
    symbol: &Bitmap,
    source: &mut TextSource<'_, '_>,
    budget: &mut Budget,
) -> Result<Bitmap> {
    let rdw = source.value(Which::Rdw)?;
    let rdh = source.value(Which::Rdh)?;
    let rdx = source.value(Which::Rdx)?;
    let rdy = source.value(Which::Rdy)?;
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
    match source {
        TextSource::Arithmetic { decoder, contexts } => {
            decode_refinement(&refinement, decoder, &mut contexts.refinement, budget)
        }
        // With Huffman tables, the size of the refinement's data comes first,
        // then the data, from a byte, coded with the arithmetic coder.
        TextSource::Huffman {
            bits,
            tables,
            refinement: contexts,
        } => {
            let size = tables.rsize.decode(bits)?.ok_or_else(invalid)?;
            let size = usize::try_from(size).map_err(|_| invalid())?;
            bits.align();
            let data = bits.rest().get(..size).ok_or_else(invalid)?;
            let mut decoder = Decoder::new(data);
            let bitmap = decode_refinement(&refinement, &mut decoder, contexts, budget)?;
            bits.skip(size);
            Ok(bitmap)
        }
    }
}

// Draws `symbol` at `s` and `t` as the reference corner says, and gives the
// S of its far side (section 6.4.5, steps 3 c x and xi).
fn place(region: &mut Bitmap, symbol: &Bitmap, s: i64, t: i64, p: &TextParameters<'_>) -> i64 {
    let (w, h) = (symbol.width as i64, symbol.height as i64);
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
    region.draw(symbol, x, y, p.combination);
    if !p.transposed && !right {
        s += w - 1;
    } else if p.transposed && !bottom {
        s += h - 1;
    }
    s
}
