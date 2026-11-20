//! `/DCTDecode`: JPEG images (ITU-T T.81), as PDF embeds them (ISO 32000-2,
//! section 7.4.8).
//!
//! Written for the engine from the standard: baseline, extended sequential
//! and progressive DCT with Huffman coding, 8 bits per sample, gray, color
//! and four-component images, any sampling and restart intervals. The
//! arithmetic coding, 12-bit samples and lossless JPEG are unsupported, as
//! in PDF readers.

mod huffman;
mod output;
mod scan;

use huffman::HuffmanTable;
use scan::{Component, Layout, ScanHeader, Tables, ZIGZAG, decode_scan};

use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidJpegData, 0)
}

/// The most scans of one image that are decoded, which is far more than any
/// writer makes.
const MAX_SCANS: usize = 64;

/// A JPEG image, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegImage {
    pub width: usize,
    pub height: usize,
    /// 1 for gray, 3 for color and 4 for CMYK.
    pub components: usize,
    /// The samples of each pixel one after the other, 8 bits each, in rows
    /// without padding. Color is RGB, and 4 components are as the file has
    /// them: Adobe's CMYK files are inverted, and a PDF's `/Decode` says so.
    pub data: Vec<u8>,
    /// Problems found and worked around while decoding it.
    pub diagnostics: Diagnostics,
}

struct Frame {
    width: usize,
    height: usize,
    progressive: bool,
    components: Vec<Component>,
    mcus_x: usize,
    mcus_y: usize,
    max_h: usize,
    max_v: usize,
}

fn u16_at(data: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(*data.get(at)?) << 8 | usize::from(*data.get(at + 1)?))
}

/// Decodes the JPEG `data` of an image.
///
/// `color_transform` is the `/ColorTransform` of the image's `/DecodeParms`
/// when it has one: whether three components are YCbCr and four are YCCK.
/// An Adobe marker in the data says it and wins over it.
///
/// Data that is cut short or damaged gives what was decoded, with a
/// `DamagedJpegData` diagnostic at `offset`. It is an `InvalidJpegData`
/// error if there is no frame or scan to decode, an `UnsupportedFilter` error
/// for the kinds of JPEG left out, and a `LimitExceeded` error if the image
/// is larger than `limits` allow.
pub fn decode_jpeg(
    data: &[u8],
    color_transform: Option<bool>,
    offset: usize,
    limits: Limits,
) -> Result<JpegImage> {
    decode_jpeg_scaled(data, color_transform, offset, limits, 8)
}

/// Like [`decode_jpeg`], for a picture `scale` eighths of its size: 8 is the
/// whole picture, and 4, 2 and 1 give one half, one quarter and one eighth of
/// its width and height (rounded up), much faster, for images drawn small. The
/// size is in the result's `width` and `height`.
pub fn decode_jpeg_scaled(
    data: &[u8],
    color_transform: Option<bool>,
    offset: usize,
    limits: Limits,
    scale: usize,
) -> Result<JpegImage> {
    let scale = if matches!(scale, 1 | 2 | 4) { scale } else { 8 };
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    let mut position = data
        .windows(2)
        .position(|w| w == [0xFF, 0xD8])
        .ok_or_else(invalid)?
        + 2;
    let mut quantization = [[1u16; 64]; 4];
    let mut quantization_set = [false; 4];
    let mut tables = Tables {
        dc: [None, None, None, None],
        ac: [None, None, None, None],
    };
    let mut restart_interval = 0usize;
    let mut adobe: Option<u8> = None;
    let mut frame: Option<Frame> = None;
    let mut scans = 0;
    let mut damaged = false;

    // The next marker: after any bytes of entropy-coded data left.
    while let Some(marker) = next_marker(data, &mut position) {
        if marker == 0xD9 {
            break;
        }
        // Markers without a length.
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            continue;
        }
        let Some(length) = u16_at(data, position) else {
            damaged = true;
            break;
        };
        if length < 2 {
            damaged = true;
            break;
        }
        let end = position + length;
        let Some(segment) = data.get(position + 2..end.min(data.len())) else {
            damaged = true;
            break;
        };
        if end > data.len() {
            damaged = true;
        }
        position = end.min(data.len());
        match marker {
            0xDB => {
                if read_quantization(segment, &mut quantization, &mut quantization_set).is_none() {
                    damaged = true;
                }
            }
            0xC4 => {
                if read_huffman(segment, &mut tables).is_none() {
                    damaged = true;
                }
            }
            0xDD => {
                restart_interval = u16_at(segment, 0).ok_or_else(invalid)?;
            }
            0xEE => {
                if segment.starts_with(b"Adobe") && segment.len() >= 12 {
                    adobe = Some(segment[11]);
                }
            }
            0xC0..=0xC2 => {
                if frame.is_some() {
                    return Err(invalid());
                }
                frame = Some(read_frame(segment, marker == 0xC2, limits)?);
            }
            // Arithmetic coding, lossless, hierarchical and differential
            // frames.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(Error::new(ErrorKind::UnsupportedFilter, offset));
            }
            0xDA => {
                let Some(frame) = frame.as_mut() else {
                    return Err(invalid());
                };
                if scans == MAX_SCANS {
                    damaged = true;
                    break;
                }
                let Some(header) = read_scan_header(segment, frame) else {
                    damaged = true;
                    break;
                };
                // A component takes the table it has when it is first used.
                for &(index, _, _) in &header.components {
                    let component = &mut frame.components[index];
                    if component.table.is_none() {
                        if !quantization_set[component.quantization] {
                            return Err(invalid());
                        }
                        component.table = Some(quantization[component.quantization]);
                    }
                }
                let layout = Layout {
                    mcus_x: frame.mcus_x,
                    mcus_y: frame.mcus_y,
                    progressive: frame.progressive,
                    restart_interval,
                };
                let end = decode_scan(
                    &mut frame.components,
                    &header,
                    &tables,
                    &layout,
                    data,
                    position,
                );
                scans += 1;
                position = end.position;
                damaged |= end.damaged;
            }
            _ => {}
        }
    }
    let frame = frame.ok_or_else(invalid)?;
    if scans == 0 {
        return Err(invalid());
    }
    if damaged {
        diagnostics.push(DiagnosticKind::DamagedJpegData, offset);
    }
    assemble(frame, adobe, color_transform, scale, diagnostics)
}

/// Moves `position` past the next marker and gives its code.
fn next_marker(data: &[u8], position: &mut usize) -> Option<u8> {
    loop {
        let at = data.get(*position..)?.iter().position(|&b| b == 0xFF)? + *position;
        let mut next = at + 1;
        while data.get(next) == Some(&0xFF) {
            next += 1;
        }
        let marker = *data.get(next)?;
        *position = next + 1;
        // 0xFF00 is a stuffed byte of the data, and the restart markers
        // belong to it.
        if marker != 0 && !(0xD0..=0xD7).contains(&marker) {
            return Some(marker);
        }
    }
}

fn read_quantization(
    mut segment: &[u8],
    tables: &mut [[u16; 64]; 4],
    set: &mut [bool; 4],
) -> Option<()> {
    while let Some((&info, rest)) = segment.split_first() {
        let (wide, id) = (info >> 4, usize::from(info & 15));
        let size = if wide == 0 { 64 } else { 128 };
        let values = rest.get(..size)?;
        let table = tables.get_mut(id)?;
        for (i, &zigzag) in ZIGZAG.iter().enumerate() {
            table[zigzag] = if wide == 0 {
                u16::from(values[i])
            } else {
                u16::from(values[2 * i]) << 8 | u16::from(values[2 * i + 1])
            };
        }
        set[id] = true;
        segment = &rest[size..];
    }
    Some(())
}

fn read_huffman(mut segment: &[u8], tables: &mut Tables) -> Option<()> {
    while let Some((&info, rest)) = segment.split_first() {
        let (class, id) = (info >> 4, usize::from(info & 15));
        let counts: [u8; 16] = rest.get(..16)?.try_into().ok()?;
        let total: usize = counts.iter().map(|&c| usize::from(c)).sum();
        let symbols = rest.get(16..16 + total)?;
        let table = HuffmanTable::new(&counts, symbols).ok()?;
        let slot = match class {
            0 => tables.dc.get_mut(id)?,
            1 => tables.ac.get_mut(id)?,
            _ => return None,
        };
        *slot = Some(table);
        segment = &rest[16 + total..];
    }
    Some(())
}

fn read_frame(segment: &[u8], progressive: bool, limits: Limits) -> Result<Frame> {
    let precision = *segment.first().ok_or_else(invalid)?;
    let height = u16_at(segment, 1).ok_or_else(invalid)?;
    let width = u16_at(segment, 3).ok_or_else(invalid)?;
    let count = usize::from(*segment.get(5).ok_or_else(invalid)?);
    if precision != 8 {
        return Err(Error::new(ErrorKind::UnsupportedFilter, 0));
    }
    if width == 0 || height == 0 || count == 0 || count > 4 || count == 2 {
        return Err(if count == 2 {
            Error::new(ErrorKind::UnsupportedFilter, 0)
        } else {
            invalid()
        });
    }
    if width.saturating_mul(height).saturating_mul(count) > limits.max_decoded_length {
        return Err(Error::new(
            ErrorKind::LimitExceeded(Limit::DecodedLength),
            0,
        ));
    }
    let mut specs: Vec<(u8, usize, usize, usize)> = Vec::new();
    for i in 0..count {
        let at = 6 + 3 * i;
        let id = *segment.get(at).ok_or_else(invalid)?;
        let sampling = *segment.get(at + 1).ok_or_else(invalid)?;
        let table = usize::from(*segment.get(at + 2).ok_or_else(invalid)?);
        let (h, v) = (usize::from(sampling >> 4), usize::from(sampling & 15));
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) || table > 3 {
            return Err(invalid());
        }
        if specs.iter().any(|&(other, ..)| other == id) {
            return Err(invalid());
        }
        specs.push((id, h, v, table));
    }
    let max_h = specs.iter().map(|s| s.1).max().unwrap_or(1);
    let max_v = specs.iter().map(|s| s.2).max().unwrap_or(1);
    // Samples are repeated by whole numbers.
    if specs.iter().any(|s| max_h % s.1 != 0 || max_v % s.2 != 0) {
        return Err(Error::new(ErrorKind::UnsupportedFilter, 0));
    }
    let mcus_x = width.div_ceil(8 * max_h);
    let mcus_y = height.div_ceil(8 * max_v);
    let components = specs
        .into_iter()
        .map(|(id, h, v, quantization)| {
            let (stride, rows) = (mcus_x * h, mcus_y * v);
            Component {
                id,
                h,
                v,
                quantization,
                table: None,
                blocks_x: (width * h).div_ceil(max_h).div_ceil(8),
                blocks_y: (height * v).div_ceil(max_v).div_ceil(8),
                stride,
                rows,
                coefficients: vec![0; stride * rows * 64],
            }
        })
        .collect();
    Ok(Frame {
        width,
        height,
        progressive,
        components,
        mcus_x,
        mcus_y,
        max_h,
        max_v,
    })
}

fn read_scan_header(segment: &[u8], frame: &Frame) -> Option<ScanHeader> {
    let count = usize::from(*segment.first()?);
    if count == 0 || count > 4 || count > frame.components.len() {
        return None;
    }
    let mut components = Vec::new();
    for i in 0..count {
        let id = *segment.get(1 + 2 * i)?;
        let tables = *segment.get(2 + 2 * i)?;
        let index = frame.components.iter().position(|c| c.id == id)?;
        if components.iter().any(|&(other, _, _)| other == index) {
            return None;
        }
        components.push((index, usize::from(tables >> 4), usize::from(tables & 15)));
    }
    let at = 1 + 2 * count;
    let (start, end, approximation) = (
        usize::from(*segment.get(at)?),
        usize::from(*segment.get(at + 1)?),
        *segment.get(at + 2)?,
    );
    if components.iter().any(|&(_, dc, ac)| dc > 3 || ac > 3) {
        return None;
    }
    if !frame.progressive {
        return Some(ScanHeader {
            components,
            start: 0,
            end: 63,
            high: 0,
            low: 0,
        });
    }
    let (high, low) = (u32::from(approximation >> 4), u32::from(approximation & 15));
    let valid = start <= end
        && end <= 63
        && low <= 13
        && high <= 13
        && (start > 0 || end == 0)
        && (start == 0 || count == 1);
    valid.then_some(ScanHeader {
        components,
        start,
        end,
        high,
        low,
    })
}

/// The image from the coefficients: samples, repeated to the size of the
/// image, with the color transform.
fn assemble(
    frame: Frame,
    adobe: Option<u8>,
    color_transform: Option<bool>,
    scale: usize,
    diagnostics: Diagnostics,
) -> Result<JpegImage> {
    // The size of the result: each block of 8 gives `scale` samples.
    let (width, height) = (
        (frame.width * scale).div_ceil(8),
        (frame.height * scale).div_ceil(8),
    );
    let count = frame.components.len();
    let transform = match count {
        3 => match adobe {
            Some(0) => false,
            Some(_) => true,
            None => color_transform.unwrap_or_else(|| {
                // Components named R, G and B aren't YCbCr.
                let ids: Vec<u8> = frame.components.iter().map(|c| c.id).collect();
                ids != *b"RGB"
            }),
        },
        4 => match adobe {
            Some(2) => true,
            Some(_) => false,
            None => color_transform.unwrap_or(false),
        },
        _ => false,
    };
    let mut data = vec![0u8; width * height * count];
    for (index, component) in frame.components.iter().enumerate() {
        let table = component.table.unwrap_or([1; 64]);
        let plane = if scale == 8 {
            output::samples(component, &table)
        } else {
            output::samples_scaled(component, &table, scale)
        };
        let real = (
            (width * component.h).div_ceil(frame.max_h),
            (height * component.v).div_ceil(frame.max_v),
        );
        let full = output::upsample(
            &plane,
            component.stride * scale,
            real,
            (frame.max_h / component.h, frame.max_v / component.v),
            (width, height),
        );
        for (pixel, &sample) in full.iter().enumerate() {
            data[pixel * count + index] = sample;
        }
    }
    if transform {
        for pixel in data.chunks_exact_mut(count) {
            output::ycc_to_rgb(pixel);
        }
    }
    Ok(JpegImage {
        width,
        height,
        components: count,
        data,
        diagnostics,
    })
}
