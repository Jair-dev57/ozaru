//! Stream filters: how the data of a stream is decoded (ISO 32000-2, section 7.4).
//!
//! Flate and the PNG predictors came in phase 3, because cross-reference streams
//! and object streams use them. The other filters for text, and the TIFF
//! predictor, came in phase 4.

mod ascii85;
mod ascii_hex;
mod ccitt;
mod flate;
mod lzw;
mod predictor;
mod run_length;

pub use ascii_hex::decode_ascii_hex;
pub use ascii85::decode_ascii85;
pub use ccitt::{CcittParameters, decode_ccitt};
pub use flate::decode_flate;
pub use lzw::decode_lzw;
pub use run_length::decode_run_length;

use crate::base::{Diagnostics, Error, ErrorKind, Limits, Result};
use crate::syntax::{Dictionary, Object, Stream};

/// Data decoded by a filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub data: Vec<u8>,
    /// Problems found and worked around while decoding it.
    pub diagnostics: Diagnostics,
}

/// Decodes the data of `stream`, found in `file`, with the filters in its
/// `/Filter` entry and their `/DecodeParms`.
///
/// Supported: `/FlateDecode`, `/LZWDecode`, `/RunLengthDecode`, `/ASCIIHexDecode`
/// and `/ASCII85Decode`, with the TIFF and PNG predictors. The image filters,
/// `/DCTDecode`, `/JPXDecode`, `/CCITTFaxDecode` and `/JBIG2Decode`, come with
/// images, and `/Crypt` with decryption; they and any other filter are an
/// [`ErrorKind::UnsupportedFilter`] error.
pub fn decode_stream(stream: &Stream, file: &[u8], limits: Limits) -> Result<Decoded> {
    let offset = stream.data_range().start;
    let data = stream
        .raw_data(file)
        .ok_or_else(|| Error::new(ErrorKind::UnexpectedEnd, file.len()))?;
    decode_data(stream.dictionary(), data, offset, limits)
}

/// Like [`decode_stream`], for raw stream `data` that isn't in the file as it is,
/// such as decrypted data. `dictionary` is the stream's, and `offset` is where its
/// data starts in the file, for errors and diagnostics.
pub fn decode_data(
    dictionary: &Dictionary,
    data: &[u8],
    offset: usize,
    limits: Limits,
) -> Result<Decoded> {
    let unsupported = || Error::new(ErrorKind::UnsupportedFilter, offset);
    let invalid = || Error::new(ErrorKind::InvalidDecodeParameters, offset);
    let filters = one_or_many(dictionary.get("Filter"));
    let parameters = one_or_many(dictionary.get("DecodeParms"));
    if parameters.len() > filters.len() {
        return Err(invalid());
    }

    let mut decoded = Decoded {
        data: data.to_vec(),
        diagnostics: Diagnostics::new(limits.max_diagnostics),
    };
    for (index, filter) in filters.iter().enumerate() {
        let parameters: Option<&Dictionary> = match parameters.get(index) {
            None | Some(Object::Null) => None,
            Some(Object::Dictionary(parameters)) => Some(parameters),
            Some(_) => return Err(invalid()),
        };
        let step = match filter.as_name().map(|name| name.as_bytes()) {
            Some(b"FlateDecode") => decode_flate(&decoded.data, parameters, offset, limits)?,
            Some(b"LZWDecode") => decode_lzw(&decoded.data, parameters, offset, limits)?,
            Some(b"RunLengthDecode") => decode_run_length(&decoded.data, offset, limits)?,
            Some(b"ASCIIHexDecode") => decode_ascii_hex(&decoded.data, offset, limits)?,
            Some(b"ASCII85Decode") => decode_ascii85(&decoded.data, offset, limits)?,
            _ => return Err(unsupported()),
        };
        decoded.diagnostics.append(&step.diagnostics);
        decoded.data = step.data;
    }
    Ok(decoded)
}

/// A filter that decodes the data of an image into pixels, and isn't decoded
/// with the others: the data is given to the reader of its format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilter {
    /// `/DCTDecode`: JPEG.
    Dct,
    /// `/JPXDecode`: JPEG 2000.
    Jpx,
    /// `/CCITTFaxDecode`: the Group 3 and 4 fax encodings.
    CcittFax,
    /// `/JBIG2Decode`.
    Jbig2,
}

impl ImageFilter {
    fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"DCTDecode" => Some(Self::Dct),
            b"JPXDecode" => Some(Self::Jpx),
            b"CCITTFaxDecode" => Some(Self::CcittFax),
            b"JBIG2Decode" => Some(Self::Jbig2),
            _ => None,
        }
    }
}

/// An image filter that still encodes some data, with its `/DecodeParms`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageEncoding {
    pub filter: ImageFilter,
    pub parameters: Option<Dictionary>,
}

/// The data of an image with every filter decoded but an image filter at the
/// end, which is given with its `/DecodeParms`. Like [`decode_data`]
/// otherwise: an image filter anywhere else, or any filter it doesn't know,
/// is an [`ErrorKind::UnsupportedFilter`] error.
pub fn decode_image_data(
    dictionary: &Dictionary,
    data: &[u8],
    offset: usize,
    limits: Limits,
) -> Result<(Decoded, Option<ImageEncoding>)> {
    let filters = one_or_many(dictionary.get("Filter"));
    let parameters = one_or_many(dictionary.get("DecodeParms"));
    let last = filters
        .last()
        .and_then(Object::as_name)
        .and_then(|name| ImageFilter::from_name(name.as_bytes()));
    let Some(image) = last else {
        return Ok((decode_data(dictionary, data, offset, limits)?, None));
    };
    let count = filters.len() - 1;
    let mut rest = dictionary.clone();
    rest.insert("Filter".into(), Object::Array(filters[..count].to_vec()));
    rest.insert(
        "DecodeParms".into(),
        Object::Array(parameters.iter().take(count).cloned().collect()),
    );
    let image_parameters = match parameters.get(count) {
        Some(Object::Dictionary(parameters)) => Some(parameters.clone()),
        _ => None,
    };
    let decoded = decode_data(&rest, data, offset, limits)?;
    let encoding = ImageEncoding {
        filter: image,
        parameters: image_parameters,
    };
    Ok((decoded, Some(encoding)))
}

// `/Filter` and `/DecodeParms` hold one value or an array of them.
fn one_or_many(value: Option<&Object>) -> &[Object] {
    match value {
        None => &[],
        Some(Object::Array(items)) => items,
        Some(value) => std::slice::from_ref(value),
    }
}
