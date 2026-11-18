use super::arithmetic::{Contexts, Decoder};
use super::bitmap::{Bitmap, Combination};
use super::generic::{GenericParameters, decode_generic, decode_mmr, generic_contexts};
use super::{Budget, invalid};
use crate::base::{Error, ErrorKind, Limits, Result};

/// Decodes a pattern dictionary (ITU-T T.88, section 6.7.5): one bitmap of
/// every pattern side by side, cut into them.
pub(super) fn decode_patterns(
    data: &[u8],
    limits: Limits,
    budget: &mut Budget,
) -> Result<Vec<Bitmap>> {
    let flags = *data.first().ok_or_else(invalid)?;
    let mmr = flags & 1 != 0;
    let template = (flags >> 1) & 3;
    let width = usize::from(*data.get(1).ok_or_else(invalid)?);
    let height = usize::from(*data.get(2).ok_or_else(invalid)?);
    let bytes = data.get(3..7).ok_or_else(invalid)?;
    let gray_max = u32::from_be_bytes(bytes.try_into().map_err(|_| invalid())?) as usize;
    let count = gray_max.checked_add(1).ok_or_else(invalid)?;
    let total_width = count.checked_mul(width).ok_or_else(invalid)?;
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    budget.spend(count)?;
    let rest = data.get(7..).unwrap_or(&[]);
    let collective = if mmr {
        decode_mmr(rest, total_width, height, limits, budget)?
    } else {
        let mut adaptive = vec![(-(width as i64), 0)];
        if template == 0 {
            adaptive.extend([(-3, -1), (2, -2), (-2, -2)]);
        }
        let parameters = GenericParameters {
            width: total_width,
            height,
            template,
            typical: false,
            adaptive,
            skip: None,
        };
        let mut contexts = Contexts::new(generic_contexts(template));
        let mut decoder = Decoder::new(rest);
        decode_generic(&parameters, &mut decoder, &mut contexts, budget)?
    };
    Ok((0..count)
        .map(|i| collective.cut((i * width) as i64, 0, width, height))
        .collect())
}

/// What a halftone region is decoded with (section 6.6.2).
pub(super) struct HalftoneParameters<'p> {
    pub width: usize,
    pub height: usize,
    pub patterns: &'p [Bitmap],
    pub flags: u8,
    pub grid_width: usize,
    pub grid_height: usize,
    pub grid_x: i64,
    pub grid_y: i64,
    pub step_x: i64,
    pub step_y: i64,
}

/// Decodes a halftone region (section 6.6.5): a grid of gray values, coded
/// as bit planes in Gray code, each cell drawn with the pattern of its value.
pub(super) fn decode_halftone(
    parameters: &HalftoneParameters<'_>,
    data: &[u8],
    budget: &mut Budget,
) -> Result<Bitmap> {
    let p = parameters;
    let mmr = p.flags & 1 != 0;
    let template = (p.flags >> 1) & 3;
    let enable_skip = p.flags & 8 != 0;
    let combination = Combination::from_bits((p.flags >> 4) & 7);
    let default_pixel = p.flags & 0x80 != 0;
    let first = p.patterns.first().ok_or_else(invalid)?;
    let (pattern_width, pattern_height) = (first.width as i64, first.height as i64);
    let cells = p.grid_width.saturating_mul(p.grid_height);
    budget.spend(cells.saturating_add(p.grid_height))?;
    let mut region = Bitmap::new(p.width, p.height, default_pixel);

    // Where the cell of grid row `m` and column `n` goes.
    let place = |m: i64, n: i64| {
        (
            (p.grid_x + m * p.step_y + n * p.step_x) >> 8,
            (p.grid_y + m * p.step_x - n * p.step_y) >> 8,
        )
    };
    // The cells that fall outside the region aren't coded (section 6.6.5.1).
    let skip = enable_skip.then(|| {
        let mut skip = Bitmap::new(p.grid_width, p.grid_height, false);
        for m in 0..p.grid_height {
            for n in 0..p.grid_width {
                let (x, y) = place(m as i64, n as i64);
                let outside = x + pattern_width <= 0
                    || x >= p.width as i64
                    || y + pattern_height <= 0
                    || y >= p.height as i64;
                if outside {
                    skip.set(n, m, true);
                }
            }
        }
        skip
    });

    // The gray values: the planes from the most significant, in Gray code
    // (Annex C.5).
    let planes_count = super::text::symbol_code_length(p.patterns.len());
    if mmr && planes_count > 0 {
        // The planes would follow each other with MMR, whose length the fax
        // decoder doesn't give.
        return Err(Error::new(ErrorKind::UnsupportedFilter, 0));
    }
    let mut adaptive = vec![(if template <= 1 { 3 } else { 2 }, -1)];
    if template == 0 {
        adaptive.extend([(-3, -1), (2, -2), (-2, -2)]);
    }
    let generic = GenericParameters {
        width: p.grid_width,
        height: p.grid_height,
        template,
        typical: false,
        adaptive,
        skip,
    };
    let mut contexts = Contexts::new(generic_contexts(template));
    let mut decoder = Decoder::new(data);
    let mut planes: Vec<Bitmap> = Vec::with_capacity(planes_count as usize);
    for _ in 0..planes_count {
        planes.push(decode_generic(
            &generic,
            &mut decoder,
            &mut contexts,
            budget,
        )?);
    }
    // The planes were decoded from the most significant: plane j is at
    // planes[count - 1 - j]. Each is XORed with the one above it.
    planes.reverse();
    for j in (0..planes.len().saturating_sub(1)).rev() {
        for m in 0..p.grid_height {
            for n in 0..p.grid_width {
                let above = planes[j + 1].get(n as i64, m as i64);
                if above == 1 {
                    let value = planes[j].get(n as i64, m as i64) == 0;
                    planes[j].set(n, m, value);
                }
            }
        }
    }

    for m in 0..p.grid_height {
        for n in 0..p.grid_width {
            let mut gray = 0usize;
            for (j, plane) in planes.iter().enumerate() {
                gray |= (plane.get(n as i64, m as i64) as usize) << j;
            }
            let pattern = &p.patterns[gray.min(p.patterns.len() - 1)];
            let (x, y) = place(m as i64, n as i64);
            budget.spend(
                pattern
                    .width
                    .saturating_mul(pattern.height)
                    .saturating_add(1),
            )?;
            region.draw(pattern, x, y, combination);
        }
    }
    Ok(region)
}
