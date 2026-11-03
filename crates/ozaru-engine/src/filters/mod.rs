//! Stream filters: how the data of a stream is decoded (ISO 32000-2, section 7.4).
//!
//! Phase 3 only needs Flate and the PNG predictors, which cross-reference streams
//! and object streams use. The other filters come in phase 4.

mod flate;
mod predictor;

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
/// Only `/FlateDecode` is supported for now. Any other filter is an
/// [`ErrorKind::UnsupportedFilter`] error.
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
        match filter.as_name().map(|name| name.as_bytes()) {
            Some(b"FlateDecode") => {
                let step = decode_flate(&decoded.data, parameters, offset, limits)?;
                decoded.diagnostics.append(&step.diagnostics);
                decoded.data = step.data;
            }
            _ => return Err(unsupported()),
        }
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
