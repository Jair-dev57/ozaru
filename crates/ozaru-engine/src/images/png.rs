//! A PNG writer, for the pixels of images that aren't in a file format of
//! their own (PNG specification, third edition, W3C).

use miniz_oxide::deflate::compress_to_vec_zlib;

/// How the samples of each pixel are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngColor {
    Gray,
    Rgb,
    GrayAlpha,
    Rgba,
}

impl PngColor {
    fn samples(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::GrayAlpha => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    // The color type of the IHDR chunk.
    fn code(self) -> u8 {
        match self {
            Self::Gray => 0,
            Self::Rgb => 2,
            Self::GrayAlpha => 4,
            Self::Rgba => 6,
        }
    }
}

/// A PNG file of `width` by `height` pixels, 8 bits for each sample, whose
/// samples are `pixels`, row by row from the top. Missing samples are 0 and
/// extra ones are left out.
pub fn write_png(width: u32, height: u32, color: PngColor, pixels: &[u8]) -> Vec<u8> {
    let row = width as usize * color.samples();
    let mut filtered = Vec::with_capacity((row + 1) * height as usize);
    for y in 0..height as usize {
        // Filter type 0, none: the samples as they are.
        filtered.push(0);
        let start = (y * row).min(pixels.len());
        let end = (start + row).min(pixels.len());
        filtered.extend_from_slice(&pixels[start..end]);
        filtered.resize(filtered.len() + row - (end - start), 0);
    }

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, color.code(), 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &compress_to_vec_zlib(&filtered, 6));
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).expect("a chunk is less than 4 GB");
    png.extend_from_slice(&length.to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());
}

// The CRC-32 of PNG chunks, with the polynomial 0xEDB88320, one bit at a time
// through a table of the 256 bytes.
fn crc32(data: &[u8]) -> u32 {
    static TABLE: [u32; 256] = {
        let mut table = [0; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[n] = c;
            n += 1;
        }
        table
    };
    let mut c = 0xFFFF_FFFF_u32;
    for &byte in data {
        c = TABLE[((c ^ u32::from(byte)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn the_crc_of_iend_is_the_one_every_png_has() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
