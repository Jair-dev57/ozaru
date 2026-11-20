//! `/JBIG2Decode`: black and white images coded with JBIG2 (ITU-T T.88), as
//! PDF embeds them (ISO 32000-2, section 7.4.7): the segments of one page,
//! after those of `/JBIG2Globals`, without the file header.
//!
//! Written for the engine from the standard: the MQ arithmetic decoder,
//! generic regions with their four templates, MMR, refinement, symbol
//! dictionaries and text regions with the arithmetic coder or Huffman tables,
//! and halftones with their pattern dictionaries.

mod arithmetic;
mod bitmap;
mod generic;
mod halftone;
mod huffman;
mod symbols;
mod text;

use std::collections::HashMap;

use arithmetic::{Contexts, Decoder};
use bitmap::{Bitmap, Combination};
use generic::{
    GenericParameters, RefinementParameters, decode_generic, decode_mmr, decode_refinement,
    generic_contexts, refinement_contexts,
};
use halftone::{HalftoneParameters, decode_halftone, decode_patterns};
use huffman::{Bits, Table, standard};
use symbols::{
    SymbolContexts, SymbolParameters, SymbolTables, decode_symbols, decode_symbols_huffman,
    dictionary_code_length,
};
use text::{
    TextContexts, TextParameters, TextSource, TextTables, decode_text, read_symbol_codes,
    symbol_code_length,
};

use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidJbig2Data, 0)
}

/// How much work decoding may do: pixels decoded and symbols placed, so that
/// damaged data can't make it run for ever.
pub(super) struct Budget {
    left: usize,
}

impl Budget {
    fn spend(&mut self, amount: usize) -> Result<()> {
        self.left = self.left.checked_sub(amount).ok_or(Error::new(
            ErrorKind::LimitExceeded(Limit::DecodedLength),
            0,
        ))?;
        Ok(())
    }
}

/// One segment's header (ITU-T T.88, section 7.2).
#[derive(Debug, Clone)]
struct Segment<'a> {
    number: u32,
    kind: u8,
    referred: Vec<u32>,
    data: &'a [u8],
}

/// What a segment left for the segments that refer to it.
enum Product {
    Symbols {
        symbols: Vec<Bitmap>,
        contexts: Option<Box<SymbolContexts>>,
    },
    Region(Bitmap),
    Table(Table),
    Patterns(Vec<Bitmap>),
}

/// A JBIG2 page, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jbig2Page {
    pub width: usize,
    pub height: usize,
    /// The rows of pixels, each starting on a byte, where 0 is black, as a
    /// `/DeviceGray` image of 1 bit has them.
    pub data: Vec<u8>,
    /// Problems found and worked around while decoding it.
    pub diagnostics: Diagnostics,
}

/// Decodes the JBIG2 `data` of an image, after the segments of `globals`.
///
/// A region that can't be decoded is left out, with a `DamagedJbig2Data`
/// diagnostic at `offset`, and the others are drawn. It is an
/// `InvalidJbig2Data` error if the data has no page information, and a
/// `LimitExceeded` error if the page or the work to decode it is larger than
/// `limits` allow.
pub fn decode_jbig2(
    data: &[u8],
    globals: Option<&[u8]>,
    offset: usize,
    limits: Limits,
) -> Result<Jbig2Page> {
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    let mut segments = Vec::new();
    for stream in globals.into_iter().chain([data]) {
        read_segments(stream, &mut segments, &mut diagnostics, offset);
    }
    let mut page: Option<Page> = None;
    let mut results: HashMap<u32, Product> = HashMap::new();
    let mut budget = Budget {
        left: limits.max_decoded_length.saturating_mul(16),
    };
    for segment in &segments {
        let outcome = match segment.kind {
            48 => {
                page = Some(Page::read(segment.data, limits)?);
                Ok(())
            }
            50 => {
                if let (Some(page), Some(end)) = (page.as_mut(), u32_at(segment.data, 0)) {
                    page.end_stripe(end as usize, limits)?;
                }
                Ok(())
            }
            _ => decode_segment(segment, page.as_mut(), &mut results, limits, &mut budget),
        };
        match outcome {
            Ok(()) => {}
            Err(error) if matches!(error.kind(), ErrorKind::LimitExceeded(_)) => {
                return Err(Error::new(error.kind(), offset));
            }
            Err(_) => diagnostics.push(DiagnosticKind::DamagedJbig2Data, offset),
        }
    }
    let page = page.ok_or(Error::new(ErrorKind::InvalidJbig2Data, offset))?;
    let (width, height) = (page.bitmap.width, page.bitmap.height);
    Ok(Jbig2Page {
        width,
        height,
        data: page.bitmap.into_pdf_data(),
        diagnostics,
    })
}

// The page being decoded, from its page information segment (section
// 7.4.8).
struct Page {
    bitmap: Bitmap,
    default_pixel: bool,
    // The page's height grows with its stripes: its height wasn't known.
    growing: bool,
}

impl Page {
    fn read(data: &[u8], limits: Limits) -> Result<Self> {
        let width = u32_at(data, 0).ok_or_else(invalid)? as usize;
        let height = u32_at(data, 4).ok_or_else(invalid)?;
        let flags = *data.get(16).ok_or_else(invalid)?;
        let default_pixel = flags & 4 != 0;
        let growing = height == u32::MAX;
        let height = if growing { 0 } else { height as usize };
        check_size(width, height, limits)?;
        Ok(Self {
            bitmap: Bitmap::new(width, height, default_pixel),
            default_pixel,
            growing,
        })
    }

    fn end_stripe(&mut self, end: usize, limits: Limits) -> Result<()> {
        if self.growing {
            let height = end.saturating_add(1);
            check_size(self.bitmap.width, height, limits)?;
            self.bitmap.grow(height, self.default_pixel);
        }
        Ok(())
    }

    // Draws a region, growing a page of unknown height to hold it.
    fn draw(&mut self, region: &Bitmap, info: &RegionInfo, limits: Limits) -> Result<()> {
        if self.growing {
            let bottom = (info.y as usize).saturating_add(region.height);
            check_size(self.bitmap.width, bottom, limits)?;
            self.bitmap.grow(bottom, self.default_pixel);
        }
        self.bitmap.draw(region, info.x, info.y, info.combination);
        Ok(())
    }
}

// A bitmap counts at least one byte for each row, even if it is empty.
fn check_size(width: usize, height: usize, limits: Limits) -> Result<()> {
    if width.div_ceil(8).max(1).saturating_mul(height) > limits.max_decoded_length {
        return Err(Error::new(
            ErrorKind::LimitExceeded(Limit::DecodedLength),
            0,
        ));
    }
    Ok(())
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at + 4)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at + 2)?;
    Some(u16::from_be_bytes(bytes.try_into().ok()?))
}

// The adaptive pixels at `at`: `count` pairs of signed bytes.
fn adaptive(data: &[u8], at: usize, count: usize) -> Option<Vec<(i64, i64)>> {
    let bytes = data.get(at..at + 2 * count)?;
    Some(
        bytes
            .chunks(2)
            .map(|pair| (i64::from(pair[0] as i8), i64::from(pair[1] as i8)))
            .collect(),
    )
}

// Reads the segment headers of `data` and their data (section 7.2). Reading
// stops at a header that is cut short.
fn read_segments<'a>(
    data: &'a [u8],
    segments: &mut Vec<Segment<'a>>,
    diagnostics: &mut Diagnostics,
    offset: usize,
) {
    let mut at = 0;
    while at < data.len() {
        let Some((segment, end)) = read_segment(data, at) else {
            diagnostics.push(DiagnosticKind::DamagedJbig2Data, offset);
            return;
        };
        let last = segment.kind == 51;
        segments.push(segment);
        at = end;
        if last {
            return;
        }
    }
}

fn read_segment(data: &[u8], mut at: usize) -> Option<(Segment<'_>, usize)> {
    let number = u32_at(data, at)?;
    let flags = *data.get(at + 4)?;
    at += 5;
    let kind = flags & 0x3F;
    let long_page = flags & 0x40 != 0;
    let first = *data.get(at)?;
    let count = if first >> 5 == 7 {
        let count = (u32_at(data, at)? & 0x1FFF_FFFF) as usize;
        at += 4 + (count + 1).div_ceil(8);
        count
    } else {
        at += 1;
        usize::from(first >> 5)
    };
    let size = if number <= 256 {
        1
    } else if number <= 65536 {
        2
    } else {
        4
    };
    let mut referred = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let bytes = data.get(at..at + size)?;
        referred.push(bytes.iter().fold(0u32, |n, &b| n << 8 | u32::from(b)));
        at += size;
    }
    at += if long_page { 4 } else { 1 };
    let length = u32_at(data, at)?;
    at += 4;
    let end = if length == u32::MAX {
        // An immediate generic region of unknown length, which a PDF
        // shouldn't have: it runs to the end.
        data.len()
    } else {
        at.checked_add(length as usize)?.min(data.len())
    };
    let segment = Segment {
        number,
        kind,
        referred,
        data: data.get(at..end)?,
    };
    Some((segment, end))
}

/// The region segment information field (section 7.4.1).
struct RegionInfo {
    width: usize,
    height: usize,
    x: i64,
    y: i64,
    combination: Combination,
}

const REGION_INFO: usize = 17;

fn region_info(data: &[u8]) -> Result<RegionInfo> {
    let field = |at| u32_at(data, at).ok_or_else(invalid);
    Ok(RegionInfo {
        width: field(0)? as usize,
        height: field(4)? as usize,
        x: i64::from(field(8)?),
        y: i64::from(field(12)?),
        combination: Combination::from_bits(*data.get(16).ok_or_else(invalid)?),
    })
}

fn decode_segment(
    segment: &Segment<'_>,
    page: Option<&mut Page>,
    results: &mut HashMap<u32, Product>,
    limits: Limits,
    budget: &mut Budget,
) -> Result<()> {
    let data = segment.data;
    match segment.kind {
        // Symbol dictionary.
        0 => {
            let flags = u16_at(data, 0).ok_or_else(invalid)?;
            let huffman = flags & 1 != 0;
            let refine_aggregate = flags & 2 != 0;
            let reuse_contexts = flags & 0x100 != 0;
            let retain_contexts = flags & 0x200 != 0;
            let template = ((flags >> 10) & 3) as u8;
            let refinement_template = ((flags >> 12) & 1) as u8;
            let mut at = 2;
            // With Huffman tables there are no adaptive pixels.
            let count = match (huffman, template) {
                (true, _) => 0,
                (false, 0) => 4,
                (false, _) => 1,
            };
            let adaptive_pixels = adaptive(data, at, count).ok_or_else(invalid)?;
            at += 2 * count;
            let refinement_adaptive = if refine_aggregate && refinement_template == 0 {
                let pixels = adaptive(data, at, 2).ok_or_else(invalid)?;
                at += 4;
                pixels
            } else {
                Vec::new()
            };
            let exported = u32_at(data, at).ok_or_else(invalid)? as usize;
            let new_symbols = u32_at(data, at + 4).ok_or_else(invalid)? as usize;
            at += 8;
            let mut input = Vec::new();
            let mut previous_contexts = None;
            for referred in &segment.referred {
                if let Some(Product::Symbols { symbols, contexts }) = results.get_mut(referred) {
                    input.extend_from_slice(symbols);
                    if reuse_contexts {
                        previous_contexts = contexts.take().map(|contexts| *contexts);
                    }
                }
            }
            budget.spend(input.len().saturating_add(new_symbols))?;
            let id_length = dictionary_code_length(input.len(), new_symbols);
            let mut contexts = previous_contexts
                .unwrap_or_else(|| SymbolContexts::new(template, refinement_template, id_length));
            let parameters = SymbolParameters {
                input: &input,
                new_symbols,
                exported,
                template,
                adaptive: adaptive_pixels,
                refine_aggregate,
                refinement_template,
                refinement_adaptive,
            };
            let rest = data.get(at..).unwrap_or(&[]);
            let symbols = if huffman {
                let mut custom = custom_tables(segment, results);
                let mut pick = |bits: u16, choices: &[u8]| -> Result<Table> {
                    match choices.get(usize::from(bits)) {
                        Some(&number) => Ok(standard(number)),
                        None => custom.next().ok_or_else(invalid),
                    }
                };
                let tables = SymbolTables {
                    dh: pick((flags >> 2) & 3, &[4, 5])?,
                    dw: pick((flags >> 4) & 3, &[2, 3])?,
                    bitmap_size: pick((flags >> 6) & 1, &[1])?,
                };
                let mut bits = Bits::new(rest);
                decode_symbols_huffman(&parameters, &mut bits, &tables, limits, budget)?
            } else {
                let mut decoder = Decoder::new(rest);
                decode_symbols(&parameters, &mut decoder, &mut contexts, budget)?
            };
            results.insert(
                segment.number,
                Product::Symbols {
                    symbols,
                    contexts: retain_contexts.then(|| Box::new(contexts)),
                },
            );
            Ok(())
        }
        // Text regions: intermediate, immediate and immediate lossless.
        4 | 6 | 7 => {
            let info = region_info(data)?;
            let mut at = REGION_INFO;
            let flags = u16_at(data, at).ok_or_else(invalid)?;
            at += 2;
            let huffman = flags & 1 != 0;
            let refine = flags & 2 != 0;
            let strips = 1i64 << ((flags >> 2) & 3);
            let corner = ((flags >> 4) & 3) as u8;
            let transposed = flags & 0x40 != 0;
            let combination = Combination::from_bits(((flags >> 7) & 3) as u8);
            let default_pixel = flags & 0x200 != 0;
            // A signed number of 5 bits.
            let s_offset = (((flags >> 10) & 0x1F) as i64 ^ 0x10) - 0x10;
            let refinement_template = ((flags >> 15) & 1) as u8;
            let huffman_flags = if huffman {
                let flags = u16_at(data, at).ok_or_else(invalid)?;
                at += 2;
                flags
            } else {
                0
            };
            let refinement_adaptive = if refine && refinement_template == 0 {
                let pixels = adaptive(data, at, 2).ok_or_else(invalid)?;
                at += 4;
                pixels
            } else {
                Vec::new()
            };
            let instances = u32_at(data, at).ok_or_else(invalid)? as usize;
            at += 4;
            let mut symbols = Vec::new();
            for referred in &segment.referred {
                if let Some(Product::Symbols { symbols: found, .. }) = results.get(referred) {
                    symbols.extend_from_slice(found);
                }
            }
            check_size(info.width, info.height, limits)?;
            let parameters = TextParameters {
                width: info.width,
                height: info.height,
                instances,
                strips,
                symbols: &symbols,
                default_pixel,
                combination,
                transposed,
                corner,
                s_offset,
                refine,
                refinement_template,
                refinement_adaptive,
            };
            let rest = data.get(at..).unwrap_or(&[]);
            let region = if huffman {
                let f = huffman_flags;
                let mut custom = custom_tables(segment, results);
                let mut pick = |bits: u16, choices: &[u8]| -> Result<Table> {
                    match choices.get(usize::from(bits)) {
                        Some(&number) => Ok(standard(number)),
                        None => custom.next().ok_or_else(invalid),
                    }
                };
                let fs = pick(f & 3, &[6, 7])?;
                let ds = pick((f >> 2) & 3, &[8, 9, 10])?;
                let dt = pick((f >> 4) & 3, &[11, 12, 13])?;
                let rdw = pick((f >> 6) & 3, &[14, 15])?;
                let rdh = pick((f >> 8) & 3, &[14, 15])?;
                let rdx = pick((f >> 10) & 3, &[14, 15])?;
                let rdy = pick((f >> 12) & 3, &[14, 15])?;
                let rsize = pick((f >> 14) & 1, &[1])?;
                let mut bits = Bits::new(rest);
                let ids = read_symbol_codes(&mut bits, symbols.len())?;
                let tables = TextTables {
                    fs,
                    ds,
                    dt,
                    rdw,
                    rdh,
                    rdx,
                    rdy,
                    rsize,
                    ids,
                };
                let mut source = TextSource::Huffman {
                    bits: &mut bits,
                    tables: &tables,
                    refinement: Contexts::new(refinement_contexts(refinement_template)),
                };
                decode_text(&parameters, &mut source, budget)?
            } else {
                let mut contexts =
                    TextContexts::new(symbol_code_length(symbols.len()), refinement_template);
                let mut decoder = Decoder::new(rest);
                let mut source = TextSource::Arithmetic {
                    decoder: &mut decoder,
                    contexts: &mut contexts,
                };
                decode_text(&parameters, &mut source, budget)?
            };
            place(segment, region, &info, page, results, limits)
        }
        // Generic regions: intermediate, immediate and immediate lossless.
        36 | 38 | 39 => {
            let info = region_info(data)?;
            let flags = *data.get(REGION_INFO).ok_or_else(invalid)?;
            let mmr = flags & 1 != 0;
            let template = (flags >> 1) & 3;
            let typical = flags & 8 != 0;
            let mut at = REGION_INFO + 1;
            check_size(info.width, info.height, limits)?;
            let region = if mmr {
                decode_mmr(
                    data.get(at..).unwrap_or(&[]),
                    info.width,
                    info.height,
                    limits,
                    budget,
                )?
            } else {
                let count = if template == 0 { 4 } else { 1 };
                let adaptive_pixels = adaptive(data, at, count).ok_or_else(invalid)?;
                at += 2 * count;
                let parameters = GenericParameters {
                    width: info.width,
                    height: info.height,
                    template,
                    typical,
                    adaptive: adaptive_pixels,
                    skip: None,
                };
                let mut contexts = Contexts::new(generic_contexts(template));
                let mut decoder = Decoder::new(data.get(at..).unwrap_or(&[]));
                decode_generic(&parameters, &mut decoder, &mut contexts, budget)?
            };
            place(segment, region, &info, page, results, limits)
        }
        // Generic refinement regions.
        40 | 42 | 43 => {
            let info = region_info(data)?;
            let flags = *data.get(REGION_INFO).ok_or_else(invalid)?;
            let template = flags & 1;
            let typical = flags & 2 != 0;
            let mut at = REGION_INFO + 1;
            let adaptive_pixels = if template == 0 {
                let pixels = adaptive(data, at, 2).ok_or_else(invalid)?;
                at += 4;
                pixels
            } else {
                Vec::new()
            };
            check_size(info.width, info.height, limits)?;
            // The reference: the intermediate region it refers to, or the
            // page under it.
            let referred = segment
                .referred
                .iter()
                .find_map(|number| match results.get(number) {
                    Some(Product::Region(region)) => Some(region.clone()),
                    _ => None,
                });
            let reference = match (referred, page.as_deref()) {
                (Some(region), _) => region,
                (None, Some(page)) => page.bitmap.cut(info.x, info.y, info.width, info.height),
                (None, None) => return Err(invalid()),
            };
            let parameters = RefinementParameters {
                width: info.width,
                height: info.height,
                template,
                reference: &reference,
                dx: 0,
                dy: 0,
                typical,
                adaptive: adaptive_pixels,
            };
            let mut contexts = Contexts::new(refinement_contexts(template));
            let mut decoder = Decoder::new(data.get(at..).unwrap_or(&[]));
            let region = decode_refinement(&parameters, &mut decoder, &mut contexts, budget)?;
            place(segment, region, &info, page, results, limits)
        }
        // A pattern dictionary, for the halftones that refer to it.
        16 => {
            let patterns = decode_patterns(data, limits, budget)?;
            results.insert(segment.number, Product::Patterns(patterns));
            Ok(())
        }
        // Halftone regions: intermediate, immediate and immediate lossless.
        20 | 22 | 23 => {
            let info = region_info(data)?;
            let at = REGION_INFO;
            let flags = *data.get(at).ok_or_else(invalid)?;
            let field = |offset: usize| u32_at(data, at + 1 + offset).ok_or_else(invalid);
            let step = |offset: usize| u16_at(data, at + 17 + offset).ok_or_else(invalid);
            let patterns = segment
                .referred
                .iter()
                .find_map(|number| match results.get(number) {
                    Some(Product::Patterns(patterns)) => Some(patterns.clone()),
                    _ => None,
                })
                .ok_or_else(invalid)?;
            check_size(info.width, info.height, limits)?;
            let parameters = HalftoneParameters {
                width: info.width,
                height: info.height,
                patterns: &patterns,
                flags,
                grid_width: field(0)? as usize,
                grid_height: field(4)? as usize,
                grid_x: i64::from(field(8)? as i32),
                grid_y: i64::from(field(12)? as i32),
                step_x: i64::from(step(0)?),
                step_y: i64::from(step(2)?),
            };
            let region = decode_halftone(&parameters, data.get(at + 21..).unwrap_or(&[]), budget)?;
            place(segment, region, &info, page, results, limits)
        }
        // A table, for the Huffman coded segments that refer to it.
        53 => {
            results.insert(segment.number, Product::Table(Table::read_custom(data)?));
            Ok(())
        }
        // End of page, end of file, profiles, extensions and others: nothing
        // to draw.
        _ => Ok(()),
    }
}

// An intermediate region is kept for the refinement that refers to it; an
// immediate one is drawn on the page.
fn place(
    segment: &Segment<'_>,
    region: Bitmap,
    info: &RegionInfo,
    page: Option<&mut Page>,
    results: &mut HashMap<u32, Product>,
    limits: Limits,
) -> Result<()> {
    let intermediate = matches!(segment.kind, 4 | 20 | 36 | 40);
    if intermediate {
        results.insert(segment.number, Product::Region(region));
        return Ok(());
    }
    let page = page.ok_or_else(invalid)?;
    page.draw(&region, info, limits)
}

// The tables of the table segments `segment` refers to, in order: the custom
// tables its fields choose take them one after the other.
fn custom_tables(
    segment: &Segment<'_>,
    results: &HashMap<u32, Product>,
) -> std::vec::IntoIter<Table> {
    let tables: Vec<Table> = segment
        .referred
        .iter()
        .filter_map(|number| match results.get(number) {
            Some(Product::Table(table)) => Some(table.clone()),
            _ => None,
        })
        .collect();
    tables.into_iter()
}
