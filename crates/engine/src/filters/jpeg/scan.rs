use super::huffman::{Bits, HuffmanTable};

/// Where each coefficient of a block, from the lowest frequency, is in the
/// 8 by 8 block in rows (ITU-T T.81, Figure A.6).
pub(super) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// One component of the image and its coefficients.
pub(super) struct Component {
    pub id: u8,
    pub h: usize,
    pub v: usize,
    /// The number of its quantization table, and the table, in rows, once a
    /// scan has first used it.
    pub quantization: usize,
    pub table: Option<[u16; 64]>,
    /// The blocks that cover the component's own size.
    pub blocks_x: usize,
    pub blocks_y: usize,
    /// The blocks of the MCUs, which are more at the edges.
    pub stride: usize,
    pub rows: usize,
    pub coefficients: Vec<i16>,
}

/// The header of a scan (ITU-T T.81, section B.2.3).
pub(super) struct ScanHeader {
    /// Index of each component of the scan, with its DC and AC tables.
    pub components: Vec<(usize, usize, usize)>,
    pub start: usize,
    pub end: usize,
    pub high: u32,
    pub low: u32,
}

pub(super) struct Tables {
    pub dc: [Option<HuffmanTable>; 4],
    pub ac: [Option<HuffmanTable>; 4],
}

/// How the MCUs of the image are laid out, and how often the scan restarts.
pub(super) struct Layout {
    pub mcus_x: usize,
    pub mcus_y: usize,
    pub progressive: bool,
    pub restart_interval: usize,
}

/// How a scan ended.
pub(super) struct ScanEnd {
    pub position: usize,
    pub damaged: bool,
}

struct State<'t> {
    progressive: bool,
    start: usize,
    end: usize,
    high: u32,
    low: u32,
    end_of_band: u32,
    tables: &'t Tables,
}

fn extend(value: u32, bits: u32) -> i32 {
    if bits == 0 {
        0
    } else if value < 1 << (bits - 1) {
        value as i32 - (1 << bits) + 1
    } else {
        value as i32
    }
}

/// Decodes a scan into the coefficients of `components` (ITU-T T.81,
/// sections F.2 and G.1.2). `data` starts after the scan's header, at
/// `position`.
pub(super) fn decode_scan(
    components: &mut [Component],
    header: &ScanHeader,
    tables: &Tables,
    layout: &Layout,
    data: &[u8],
    position: usize,
) -> ScanEnd {
    let (mcus_x, mcus_y, progressive, restart_interval) = (
        layout.mcus_x,
        layout.mcus_y,
        layout.progressive,
        layout.restart_interval,
    );
    let mut bits = Bits::new(data, position);
    let mut state = State {
        progressive,
        start: header.start,
        end: header.end,
        high: header.high,
        low: header.low,
        end_of_band: 0,
        tables,
    };
    let mut predictions = vec![0i32; header.components.len()];
    let single = header.components.len() == 1;
    let (columns, rows) = if single {
        let c = &components[header.components[0].0];
        (c.blocks_x, c.blocks_y)
    } else {
        (mcus_x, mcus_y)
    };
    let total = columns * rows;
    let mut damaged = false;
    let mut restarts = 0u8;
    for mcu in 0..total {
        if restart_interval > 0 && mcu > 0 && mcu % restart_interval == 0 {
            if !bits.restart(restarts) {
                damaged = true;
                break;
            }
            restarts = restarts.wrapping_add(1);
            predictions.iter_mut().for_each(|p| *p = 0);
            state.end_of_band = 0;
        }
        let (mx, my) = (mcu % columns, mcu / columns);
        for (slot, &(index, dc, ac)) in header.components.iter().enumerate() {
            let component = &mut components[index];
            let (h, v) = if single {
                (1, 1)
            } else {
                (component.h, component.v)
            };
            for by in 0..v {
                for bx in 0..h {
                    let block = (my * v + by) * component.stride + mx * h + bx;
                    let coefficients = &mut component.coefficients[block * 64..block * 64 + 64];
                    let ok = if state.progressive {
                        decode_progressive(
                            coefficients,
                            &mut bits,
                            &mut state,
                            &mut predictions[slot],
                            dc,
                            ac,
                        )
                    } else {
                        decode_baseline(
                            coefficients,
                            &mut bits,
                            &state,
                            &mut predictions[slot],
                            dc,
                            ac,
                        )
                    };
                    if !ok {
                        damaged = true;
                    }
                }
            }
        }
        // Data past the end, or that isn't a code: the rest is lost.
        if damaged || bits.overrun > 16 {
            damaged = true;
            break;
        }
    }
    ScanEnd {
        position: bits.position(),
        damaged,
    }
}

fn decode_baseline(
    coefficients: &mut [i16],
    bits: &mut Bits<'_>,
    state: &State<'_>,
    prediction: &mut i32,
    dc: usize,
    ac: usize,
) -> bool {
    let (Some(dc_table), Some(ac_table)) = (&state.tables.dc[dc], &state.tables.ac[ac]) else {
        return false;
    };
    let Some(size) = dc_table.decode(bits) else {
        return false;
    };
    let size = u32::from(size);
    if size > 16 {
        return false;
    }
    *prediction = prediction.wrapping_add(extend(bits.bits(size), size));
    coefficients[0] = *prediction as i16;
    let mut k = 1;
    while k < 64 {
        let Some(symbol) = ac_table.decode(bits) else {
            return false;
        };
        let (run, size) = (usize::from(symbol >> 4), u32::from(symbol & 15));
        if size == 0 {
            if run == 15 {
                k += 16;
                continue;
            }
            break;
        }
        k += run;
        if k > 63 {
            break;
        }
        coefficients[ZIGZAG[k]] = extend(bits.bits(size), size) as i16;
        k += 1;
    }
    true
}

fn decode_progressive(
    coefficients: &mut [i16],
    bits: &mut Bits<'_>,
    state: &mut State<'_>,
    prediction: &mut i32,
    dc: usize,
    ac: usize,
) -> bool {
    if state.start == 0 {
        // The DC coefficient: its first bits, or one more of them.
        if state.high == 0 {
            let Some(table) = &state.tables.dc[dc] else {
                return false;
            };
            let Some(size) = table.decode(bits) else {
                return false;
            };
            let size = u32::from(size);
            if size > 16 {
                return false;
            }
            *prediction = prediction.wrapping_add(extend(bits.bits(size), size));
            coefficients[0] = (*prediction << state.low) as i16;
        } else if bits.bit() {
            coefficients[0] |= 1 << state.low;
        }
        return true;
    }
    let Some(table) = &state.tables.ac[ac] else {
        return false;
    };
    let (start, end, low) = (state.start, state.end.min(63), state.low);
    if state.high == 0 {
        // The first bits of AC coefficients.
        if state.end_of_band > 0 {
            state.end_of_band -= 1;
            return true;
        }
        let mut k = start;
        while k <= end {
            let Some(symbol) = table.decode(bits) else {
                return false;
            };
            let (run, size) = (usize::from(symbol >> 4), u32::from(symbol & 15));
            if size == 0 {
                if run < 15 {
                    state.end_of_band = (1 << run) - 1 + bits.bits(run as u32);
                    break;
                }
                k += 16;
                continue;
            }
            k += run;
            if k > end {
                break;
            }
            coefficients[ZIGZAG[k]] = (extend(bits.bits(size), size) << low) as i16;
            k += 1;
        }
        return true;
    }
    // One more bit of AC coefficients: those that are already not zero get
    // it, and the new ones are +1 or -1 at this bit (section G.1.2.3).
    let (plus, minus) = (1i16 << low, -1i16 << low);
    let refine = |value: &mut i16, bits: &mut Bits<'_>| {
        if bits.bit() && *value & plus == 0 {
            *value = value.wrapping_add(if *value >= 0 { plus } else { minus });
        }
    };
    let mut k = start;
    if state.end_of_band == 0 {
        while k <= end {
            let Some(symbol) = table.decode(bits) else {
                return false;
            };
            let (mut run, size) = (usize::from(symbol >> 4), symbol & 15);
            let mut new = 0i16;
            if size != 0 {
                new = if bits.bit() { plus } else { minus };
            } else if run != 15 {
                state.end_of_band = (1 << run) + bits.bits(run as u32);
                break;
            }
            while k <= end {
                let value = &mut coefficients[ZIGZAG[k]];
                if *value != 0 {
                    refine(value, bits);
                } else {
                    if run == 0 {
                        break;
                    }
                    run -= 1;
                }
                k += 1;
            }
            if new != 0 && k <= end {
                coefficients[ZIGZAG[k]] = new;
            }
            k += 1;
        }
    }
    if state.end_of_band > 0 {
        while k <= end {
            let value = &mut coefficients[ZIGZAG[k]];
            if *value != 0 {
                refine(value, bits);
            }
            k += 1;
        }
        state.end_of_band -= 1;
    }
    true
}
