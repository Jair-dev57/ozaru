//! A PNG reader, for the pictures that are put on pages (PNG specification,
//! third edition, W3C): every color type, bit depths from 1 to 16, palettes
//! with transparency, and interlaced images.

use super::PngColor;
use crate::base::{Error, ErrorKind, Result};
use crate::filters::{InflateStatus, inflate};

/// The most pixels a picture may have.
const MAX_PIXELS: u64 = 1 << 26;

/// A PNG picture, decoded to 8 bits for each sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPng {
    pub width: u32,
    pub height: u32,
    /// `Gray` or `Rgb`: palettes are turned into colors, and the alpha is apart.
    pub color: PngColor,
    /// The samples of each pixel one after the other, row by row from the top.
    pub pixels: Vec<u8>,
    /// How opaque each pixel is (255 is solid), if any isn't.
    pub alpha: Option<Vec<u8>>,
}

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidImage, 0)
}

// The start and step of the pixels of each pass of Adam7 interlacing, as
// (x, y, x step, y step); a single pass if there is none.
const ADAM7: [(usize, usize, usize, usize); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// Reads a PNG file.
///
/// It is an `InvalidImage` error if it isn't a PNG, is damaged before its
/// pixels, or has more than 2^26 pixels. Pixel data that is cut short gives the
/// rows that were there, and black under them. 16-bit samples keep their high
/// byte; the gamma, the color profile and the animation of a file are ignored.
pub fn read_png(data: &[u8]) -> Result<DecodedPng> {
    if data.get(..8) != Some(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Err(invalid());
    }
    let mut at = 8;
    let mut header = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut transparency: Vec<u8> = Vec::new();
    let mut compressed = Vec::new();
    while let Some(length) = u32_at(data, at) {
        let kind = data.get(at + 4..at + 8).ok_or_else(invalid)?;
        let start = at + 8;
        let end = start + length as usize;
        // A chunk cut short by the end of the file keeps what is there, for the
        // pixels; the others are lost.
        let Some(body) = data
            .get(start..end)
            .or_else(|| data.get(start..).filter(|_| kind == b"IDAT"))
        else {
            break;
        };
        match kind {
            b"IHDR" => header = Some(Header::read(body)?),
            b"PLTE" => {
                palette = body.as_chunks::<3>().0.to_vec();
            }
            b"tRNS" => transparency = body.to_vec(),
            b"IDAT" => compressed.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at = end + 4;
    }
    let header = header.ok_or_else(invalid)?;
    if header.color_type == 3 && palette.is_empty() {
        return Err(invalid());
    }
    if compressed.is_empty() {
        return Err(invalid());
    }

    let passes: Vec<(usize, usize, usize, usize)> = if header.interlaced {
        ADAM7.to_vec()
    } else {
        vec![(0, 0, 1, 1)]
    };
    let size = |start: usize, step: usize, total: usize| (total + step - 1 - start) / step;
    let (width, height) = (header.width as usize, header.height as usize);
    let bits = header.samples * usize::from(header.depth);
    let row_bytes = |pass_width: usize| (pass_width * bits).div_ceil(8);
    let mut expected = 0;
    for &(x0, y0, dx, dy) in &passes {
        let (pw, ph) = (size(x0, dx, width), size(y0, dy, height));
        if pw > 0 && ph > 0 {
            expected += (row_bytes(pw) + 1) * ph;
        }
    }
    // Deflate can't make data more than about 1,030 times smaller, so a header
    // that asks for more pixels than the data could ever hold is lying.
    if expected > compressed.len().saturating_mul(1100) + 4096 {
        return Err(invalid());
    }
    let (raw, status) = inflate(&compressed, true, expected);
    if raw.is_empty() && !matches!(status, InflateStatus::Done) {
        return Err(invalid());
    }
    let mut raw = raw;
    raw.resize(expected, 0);

    let gray = matches!(header.color_type, 0 | 4);
    let out_samples = if gray { 1 } else { 3 };
    let mut pixels = vec![0_u8; width * height * out_samples];
    let has_alpha = matches!(header.color_type, 4 | 6) || !transparency.is_empty();
    let mut alpha = vec![255_u8; if has_alpha { width * height } else { 0 }];
    // The color a gray or RGB image makes transparent, as raw samples.
    let key: Option<Vec<u16>> = (matches!(header.color_type, 0 | 2) && transparency.len() >= 2)
        .then(|| {
            transparency
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect()
        });

    let bytes_per_pixel = bits.div_ceil(8).max(1);
    let mut offset = 0;
    for &(x0, y0, dx, dy) in &passes {
        let (pw, ph) = (size(x0, dx, width), size(y0, dy, height));
        if pw == 0 || ph == 0 {
            continue;
        }
        let stride = row_bytes(pw);
        let mut rows = vec![0_u8; stride * ph];
        for y in 0..ph {
            let filter = raw[offset];
            let line = &raw[offset + 1..offset + 1 + stride];
            offset += stride + 1;
            let (done, current) = rows.split_at_mut(y * stride);
            let previous = if y == 0 {
                None
            } else {
                Some(&done[(y - 1) * stride..])
            };
            unfilter(
                filter,
                line,
                &mut current[..stride],
                previous,
                bytes_per_pixel,
            )?;
        }
        for y in 0..ph {
            let row = &rows[y * stride..(y + 1) * stride];
            for x in 0..pw {
                let target = (y0 + y * dy) * width + x0 + x * dx;
                let sample =
                    |index: usize| read_sample(row, x * header.samples + index, header.depth);
                match header.color_type {
                    0 | 4 => {
                        let value = sample(0);
                        pixels[target] = to_byte(value, header.depth);
                        if header.color_type == 4 {
                            alpha[target] = to_byte(sample(1), header.depth);
                        } else if key.as_ref().is_some_and(|key| key[0] == value) {
                            alpha[target] = 0;
                        }
                    }
                    2 | 6 => {
                        let values = [sample(0), sample(1), sample(2)];
                        for (i, value) in values.iter().enumerate() {
                            pixels[target * 3 + i] = to_byte(*value, header.depth);
                        }
                        if header.color_type == 6 {
                            alpha[target] = to_byte(sample(3), header.depth);
                        } else if key
                            .as_ref()
                            .is_some_and(|key| key.len() >= 3 && key[..3] == values)
                        {
                            alpha[target] = 0;
                        }
                    }
                    _ => {
                        let index = usize::from(sample(0));
                        let color = palette.get(index).copied().unwrap_or([0, 0, 0]);
                        pixels[target * 3..target * 3 + 3].copy_from_slice(&color);
                        if has_alpha {
                            alpha[target] = transparency.get(index).copied().unwrap_or(255);
                        }
                    }
                }
            }
        }
    }
    let alpha = (has_alpha && alpha.iter().any(|&a| a != 255)).then_some(alpha);
    Ok(DecodedPng {
        width: header.width,
        height: header.height,
        color: if gray { PngColor::Gray } else { PngColor::Rgb },
        pixels,
        alpha,
    })
}

struct Header {
    width: u32,
    height: u32,
    depth: u8,
    color_type: u8,
    samples: usize,
    interlaced: bool,
}

impl Header {
    fn read(body: &[u8]) -> Result<Self> {
        let (width, height) = (
            u32_at(body, 0).ok_or_else(invalid)?,
            u32_at(body, 4).ok_or_else(invalid)?,
        );
        let [depth, color_type, compression, filter, interlace] =
            <[u8; 5]>::try_from(body.get(8..13).ok_or_else(invalid)?).map_err(|_| invalid())?;
        let allowed: &[u8] = match color_type {
            0 => &[1, 2, 4, 8, 16],
            3 => &[1, 2, 4, 8],
            2 | 4 | 6 => &[8, 16],
            _ => return Err(invalid()),
        };
        if !allowed.contains(&depth)
            || compression != 0
            || filter != 0
            || interlace > 1
            || width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > MAX_PIXELS
        {
            return Err(invalid());
        }
        let samples = match color_type {
            0 | 3 => 1,
            4 => 2,
            2 => 3,
            _ => 4,
        };
        Ok(Self {
            width,
            height,
            depth,
            color_type,
            samples,
            interlaced: interlace == 1,
        })
    }
}

// The sample number `index` of a row, in the bits it has.
fn read_sample(row: &[u8], index: usize, depth: u8) -> u16 {
    match depth {
        16 => u16::from_be_bytes([row[index * 2], row[index * 2 + 1]]),
        8 => u16::from(row[index]),
        _ => {
            let per_byte = 8 / usize::from(depth);
            let byte = row[index / per_byte];
            let shift = 8 - usize::from(depth) * (index % per_byte + 1);
            u16::from(byte >> shift) & ((1 << depth) - 1)
        }
    }
}

// A sample as 8 bits: the high byte of 16, or spread over 0 to 255.
fn to_byte(value: u16, depth: u8) -> u8 {
    match depth {
        16 => (value >> 8) as u8,
        8 => value as u8,
        _ => (u32::from(value) * 255 / ((1 << depth) - 1)) as u8,
    }
}

// Undoes the filter of a row (PNG specification, section 9).
fn unfilter(
    filter: u8,
    line: &[u8],
    out: &mut [u8],
    previous: Option<&[u8]>,
    step: usize,
) -> Result<()> {
    for i in 0..line.len() {
        let left = if i >= step { out[i - step] } else { 0 };
        let up = previous.map_or(0, |row| row[i]);
        let corner = if i >= step {
            previous.map_or(0, |row| row[i - step])
        } else {
            0
        };
        let guess = match filter {
            0 => 0,
            1 => left,
            2 => up,
            3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
            4 => paeth(left, up, corner),
            _ => return Err(invalid()),
        };
        out[i] = line[i].wrapping_add(guess);
    }
    Ok(())
}

fn paeth(left: u8, up: u8, corner: u8) -> u8 {
    let (a, b, c) = (i32::from(left), i32::from(up), i32::from(corner));
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        left
    } else if pb <= pc {
        up
    } else {
        corner
    }
}
