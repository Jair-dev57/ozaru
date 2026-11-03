//! Stream filters: how the data of a stream is decoded (ISO 32000-2, section 7.4).
//!
//! Flate and the PNG predictors came in phase 3, because cross-reference streams
//! and object streams use them. The other filters come in phase 4.

mod ascii85;
mod ascii_hex;
mod flate;
mod predictor;

pub use ascii_hex::decode_ascii_hex;
pub use ascii85::decode_ascii85;
pub use flate::decode_flate;

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
/// Supported so far: `/FlateDecode`, `/ASCIIHexDecode` and `/ASCII85Decode`. Any
/// other filter is an [`ErrorKind::UnsupportedFilter`] error.
pub fn decode_stream(stream: &Stream, file: &[u8], limits: Limits) -> Result<Decoded> {
    let offset = stream.data_range().start;
    let unsupported = || Error::new(ErrorKind::UnsupportedFilter, offset);
    let invalid = || Error::new(ErrorKind::InvalidDecodeParameters, offset);
    let data = stream
        .raw_data(file)
        .ok_or_else(|| Error::new(ErrorKind::UnexpectedEnd, file.len()))?;

    let dictionary = stream.dictionary();
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
            Some(b"ASCIIHexDecode") => decode_ascii_hex(&decoded.data, offset, limits)?,
            Some(b"ASCII85Decode") => decode_ascii85(&decoded.data, offset, limits)?,
            _ => return Err(unsupported()),
        };
        decoded.diagnostics.append(&step.diagnostics);
        decoded.data = step.data;
    }
    Ok(decoded)
}

// `/Filter` and `/DecodeParms` hold one value or an array of them.
fn one_or_many(value: Option<&Object>) -> &[Object] {
    match value {
        None => &[],
        Some(Object::Array(items)) => items,
        Some(value) => std::slice::from_ref(value),
    }
}
