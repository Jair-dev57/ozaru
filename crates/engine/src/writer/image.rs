//! Pictures to put on pages: a JPEG or PNG file read, and the image XObject
//! it makes (ISO 32000-2, section 8.9).
//!
//! JPEG files go into the PDF as they are (`/DCTDecode`), so they aren't
//! recompressed; PNG files are decoded and their pixels compressed with Flate,
//! with the transparency as a soft mask.

use super::Editor;
use crate::base::{Error, ErrorKind, Result};
use crate::filters::deflate_zlib;
use crate::images::{PngColor, read_png};
use crate::syntax::{Dictionary, Name, Object, ObjectId};

/// The name an image has in the resources of the page it is drawn on, which
/// depends only on its bytes, so drawing it twice on a page shares it.
pub fn image_resource(image: &EmbeddedImage) -> String {
    format!("OzI{:016x}", image.fingerprint)
}

#[derive(Clone, PartialEq)]
enum Source {
    // The file as it is, with the number of its components and whether its
    // four are inverted, as Adobe's CMYK files have them.
    Jpeg {
        bytes: Vec<u8>,
        components: u8,
        inverted: bool,
    },
    Pixels {
        gray: bool,
        pixels: Vec<u8>,
        alpha: Option<Vec<u8>>,
    },
}

/// A picture read from a JPEG or PNG file.
#[derive(Clone)]
pub struct EmbeddedImage {
    width: u32,
    height: u32,
    source: Source,
    fingerprint: u64,
}

impl std::fmt::Debug for EmbeddedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl PartialEq for EmbeddedImage {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint && self.source == other.source
    }
}

fn fingerprint(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn unsupported() -> Error {
    Error::new(ErrorKind::UnsupportedImage, 0)
}

impl EmbeddedImage {
    /// Reads a JPEG or PNG file, told apart by its first bytes.
    ///
    /// It is an `UnsupportedImage` error if it is neither, and an
    /// `InvalidImage` one if it is damaged before its pixels. The pixels of a
    /// JPEG aren't decoded here, so a damaged one is found by whoever reads it;
    /// its orientation tag is ignored.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let hash = fingerprint(data);
        if data.starts_with(&[0xFF, 0xD8]) {
            let (width, height, components, inverted) = jpeg_header(data)?;
            return Ok(Self {
                width,
                height,
                source: Source::Jpeg {
                    bytes: data.to_vec(),
                    components,
                    inverted,
                },
                fingerprint: hash,
            });
        }
        if data.starts_with(&[0x89, b'P', b'N', b'G']) {
            let png = read_png(data)?;
            return Ok(Self {
                width: png.width,
                height: png.height,
                source: Source::Pixels {
                    gray: png.color == PngColor::Gray,
                    pixels: png.pixels,
                    alpha: png.alpha,
                },
                fingerprint: hash,
            });
        }
        Err(unsupported())
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Adds the image to `editor` as an image XObject (and its soft mask, if
    /// it has transparency), and gives its reference.
    pub fn embed(&self, editor: &mut Editor<'_, '_>) -> ObjectId {
        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Type"), Object::Name(Name::from("XObject")));
        dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("Image")));
        dictionary.insert(Name::from("Width"), Object::Integer(i64::from(self.width)));
        dictionary.insert(
            Name::from("Height"),
            Object::Integer(i64::from(self.height)),
        );
        dictionary.insert(Name::from("BitsPerComponent"), Object::Integer(8));
        let name = |text: &str| Object::Name(Name::from(text));
        match &self.source {
            Source::Jpeg {
                bytes,
                components,
                inverted,
            } => {
                dictionary.insert(
                    Name::from("ColorSpace"),
                    name(match components {
                        1 => "DeviceGray",
                        4 => "DeviceCMYK",
                        _ => "DeviceRGB",
                    }),
                );
                dictionary.insert(Name::from("Filter"), name("DCTDecode"));
                if *inverted {
                    dictionary.insert(
                        Name::from("Decode"),
                        Object::Array(
                            [1, 0, 1, 0, 1, 0, 1, 0]
                                .iter()
                                .map(|&v| Object::Integer(v))
                                .collect(),
                        ),
                    );
                }
                editor.add_stream(dictionary, bytes.clone())
            }
            Source::Pixels {
                gray,
                pixels,
                alpha,
            } => {
                dictionary.insert(
                    Name::from("ColorSpace"),
                    name(if *gray { "DeviceGray" } else { "DeviceRGB" }),
                );
                dictionary.insert(Name::from("Filter"), name("FlateDecode"));
                if let Some(alpha) = alpha {
                    let mut mask = Dictionary::new();
                    mask.insert(Name::from("Type"), name("XObject"));
                    mask.insert(Name::from("Subtype"), name("Image"));
                    mask.insert(Name::from("Width"), Object::Integer(i64::from(self.width)));
                    mask.insert(
                        Name::from("Height"),
                        Object::Integer(i64::from(self.height)),
                    );
                    mask.insert(Name::from("ColorSpace"), name("DeviceGray"));
                    mask.insert(Name::from("BitsPerComponent"), Object::Integer(8));
                    mask.insert(Name::from("Filter"), name("FlateDecode"));
                    let mask = editor.add_stream(mask, deflate_zlib(alpha, 6));
                    dictionary.insert(Name::from("SMask"), Object::Reference(mask));
                }
                editor.add_stream(dictionary, deflate_zlib(pixels, 6))
            }
        }
    }
}

// The size, number of components and Adobe inversion of a JPEG, from its
// frame header (ITU-T T.81, B.2.2). Only 8 bit samples can go in a PDF as
// they are.
fn jpeg_header(data: &[u8]) -> Result<(u32, u32, u8, bool)> {
    let bad = || Error::new(ErrorKind::InvalidJpegData, 0);
    let mut at = 2;
    let mut adobe = None;
    while at + 4 <= data.len() {
        if data[at] != 0xFF {
            at += 1;
            continue;
        }
        let marker = data[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) || marker == 0 {
            at += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([data[at + 2], data[at + 3]]));
        let body = data.get(at + 4..at + 2 + length).ok_or_else(bad)?;
        match marker {
            // Adobe's marker: the transform is the last byte; 0 is
            // CMYK that is stored inverted.
            0xEE if body.starts_with(b"Adobe") => adobe = Some(true),
            // The frames: baseline, extended and progressive DCT with
            // Huffman coding.
            0xC0..=0xC2 => {
                let precision = *body.first().ok_or_else(bad)?;
                let height = u16::from_be_bytes([
                    *body.get(1).ok_or_else(bad)?,
                    *body.get(2).ok_or_else(bad)?,
                ]);
                let width = u16::from_be_bytes([
                    *body.get(3).ok_or_else(bad)?,
                    *body.get(4).ok_or_else(bad)?,
                ]);
                let components = *body.get(5).ok_or_else(bad)?;
                if precision != 8 || !matches!(components, 1 | 3 | 4) || width == 0 || height == 0 {
                    return Err(unsupported());
                }
                return Ok((
                    u32::from(width),
                    u32::from(height),
                    components,
                    components == 4 && adobe.is_some(),
                ));
            }
            // The other kinds of frame: arithmetic coding, lossless and
            // hierarchical.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err(unsupported()),
            _ => {}
        }
        at += 2 + length;
    }
    Err(bad())
}
