use super::color::{ColorSpace, read_color_space};
use crate::base::{Diagnostics, Error, ErrorKind, Limits, Result};
use crate::content::ImageSource;
use crate::filters::{ImageEncoding, decode_image_data};
use crate::syntax::{Dictionary, Name, Object};
use crate::xref::Resolver;

/// What an image is, from its dictionary (ISO 32000-2, section 8.9.5): its
/// size in pixels, the bits of each color component, its color space, and
/// whether it is a stencil mask.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageDescription {
    pub width: u32,
    pub height: u32,
    /// 1, 2, 4, 8 or 16. A stencil mask has 1.
    pub bits_per_component: u8,
    /// `/ColorSpace`, resolved, with the names of an inline image written
    /// out: `/DeviceGray`, `[/ICCBased 5 0 R]` and so on. `None` for a
    /// stencil mask, and for a JPEG 2000 image that leaves it to its data.
    pub color_space: Option<Object>,
    /// `/ImageMask`: a stencil mask, painted with the fill color where its
    /// bits say.
    pub image_mask: bool,
    /// `/Decode`, if the image has one.
    pub decode: Option<Vec<f64>>,
}

/// An image, read: its description and its data, with every filter decoded
/// but the image filter at the end, if there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    pub description: ImageDescription,
    /// Its dictionary, with the names of an inline image written out.
    pub dictionary: Dictionary,
    /// The filter that still encodes the data, such as `/DCTDecode`, with its
    /// `/DecodeParms`. `None` if the data is pixels already.
    pub encoding: Option<ImageEncoding>,
    /// Its color space, read: `None` for a stencil mask, or if it isn't one.
    pub colors: Option<ColorSpace>,
    pub data: Vec<u8>,
    /// Problems found and worked around while decoding the data.
    pub diagnostics: Diagnostics,
}

// How large an image can be, in pixels on each side.
const MAX_SIDE: u32 = 1 << 16;

/// Reads the image of `source`.
///
/// It is an `InvalidImage` error if its size or bits per component aren't
/// usable, an `UnsupportedFilter` error if a filter can't be decoded, and a
/// `PasswordRequired` error if it is encrypted and the file is locked.
pub fn read_image(
    resolver: &mut Resolver<'_>,
    source: &ImageSource,
    limits: Limits,
) -> Result<ImageData> {
    let (dictionary, data, offset) = match source {
        ImageSource::XObject { id, .. } => {
            let offset = resolver.object_offset(id.number);
            let (object, data) = resolver.decrypted_stream(*id)?;
            let Some(stream) = object.as_stream() else {
                return Err(Error::new(ErrorKind::NotAStream, offset));
            };
            let dictionary = resolved_entries(resolver, stream.dictionary());
            (dictionary, data, stream.data_range().start)
        }
        ImageSource::Inline { dictionary, data } => {
            let dictionary = resolved_entries(resolver, &expanded(dictionary));
            (dictionary, data.clone(), 0)
        }
    };
    let description = describe(&dictionary, offset)?;
    let colors = description
        .color_space
        .as_ref()
        .and_then(|entry| read_color_space(resolver, entry));
    let (decoded, encoding) = decode_image_data(&dictionary, &data, offset, limits)?;
    Ok(ImageData {
        description,
        dictionary,
        encoding,
        colors,
        data: decoded.data,
        diagnostics: decoded.diagnostics,
    })
}

// The entries of an image dictionary that are references to objects other
// than streams, resolved: filters and their parameters must be direct to be
// decoded. A soft mask, a stream, stays a reference.
fn resolved_entries(resolver: &mut Resolver<'_>, dictionary: &Dictionary) -> Dictionary {
    let mut resolved = Dictionary::new();
    for (key, value) in dictionary.iter() {
        let value = match value {
            Object::Reference(id) => match resolver.resolve(*id) {
                Ok(object) if object.as_stream().is_none() => (*object).clone(),
                _ => value.clone(),
            },
            _ => value.clone(),
        };
        resolved.insert(key.clone(), value);
    }
    resolved
}

fn describe(dictionary: &Dictionary, offset: usize) -> Result<ImageDescription> {
    let invalid = || Error::new(ErrorKind::InvalidImage, offset);
    let side = |key: &str| {
        dictionary
            .get(key)
            .and_then(Object::as_integer)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| (1..=MAX_SIDE).contains(value))
    };
    let (Some(width), Some(height)) = (side("Width"), side("Height")) else {
        return Err(invalid());
    };
    let image_mask = matches!(dictionary.get("ImageMask"), Some(Object::Boolean(true)));
    let jpx = matches!(
        dictionary.get("Filter"),
        Some(Object::Name(name)) if name.as_bytes() == b"JPXDecode"
    ) || matches!(
        dictionary.get("Filter"),
        Some(Object::Array(filters)) if filters.last().and_then(Object::as_name).is_some_and(|n| n.as_bytes() == b"JPXDecode")
    );
    let bits = dictionary
        .get("BitsPerComponent")
        .and_then(Object::as_integer);
    let bits_per_component = match (image_mask, bits) {
        (true, _) => 1,
        (false, Some(bits @ (1 | 2 | 4 | 8 | 16))) => bits as u8,
        // JPEG 2000 images may leave it to their data.
        (false, None) if jpx => 8,
        _ => return Err(invalid()),
    };
    let color_space = if image_mask {
        None
    } else {
        match dictionary.get("ColorSpace") {
            Some(entry) => Some(entry.clone()),
            None if jpx => None,
            None => return Err(invalid()),
        }
    };
    let decode = dictionary
        .get("Decode")
        .and_then(Object::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Object::as_number)
                .collect::<Vec<f64>>()
        });
    Ok(ImageDescription {
        width,
        height,
        bits_per_component,
        color_space,
        image_mask,
        decode,
    })
}

// The dictionary of an inline image, with its abbreviated keys and names
// written out (ISO 32000-2, tables 91 and 92).
fn expanded(dictionary: &Dictionary) -> Dictionary {
    let mut full = Dictionary::new();
    for (key, value) in dictionary.iter() {
        let key: &[u8] = match key.as_bytes() {
            b"BPC" => b"BitsPerComponent",
            b"CS" => b"ColorSpace",
            b"D" => b"Decode",
            b"DP" => b"DecodeParms",
            b"F" => b"Filter",
            b"H" => b"Height",
            b"IM" => b"ImageMask",
            b"I" => b"Interpolate",
            b"W" => b"Width",
            other => other,
        };
        let value = match (key, value) {
            (b"Filter", value) => names_written_out(value, filter_name),
            (b"ColorSpace", value) => names_written_out(value, color_space_name),
            (_, value) => value.clone(),
        };
        full.insert(Name::new(key), value);
    }
    full
}

fn names_written_out(value: &Object, full: fn(&[u8]) -> Option<&'static str>) -> Object {
    let one = |value: &Object| match value {
        Object::Name(name) => {
            full(name.as_bytes()).map_or(value.clone(), |n| Object::Name(n.into()))
        }
        other => other.clone(),
    };
    match value {
        Object::Array(items) => Object::Array(items.iter().map(one).collect()),
        value => one(value),
    }
}

fn filter_name(name: &[u8]) -> Option<&'static str> {
    Some(match name {
        b"AHx" => "ASCIIHexDecode",
        b"A85" => "ASCII85Decode",
        b"LZW" => "LZWDecode",
        b"Fl" => "FlateDecode",
        b"RL" => "RunLengthDecode",
        b"CCF" => "CCITTFaxDecode",
        b"DCT" => "DCTDecode",
        _ => return None,
    })
}

fn color_space_name(name: &[u8]) -> Option<&'static str> {
    Some(match name {
        b"G" => "DeviceGray",
        b"RGB" => "DeviceRGB",
        b"CMYK" => "DeviceCMYK",
        b"I" => "Indexed",
        _ => return None,
    })
}
