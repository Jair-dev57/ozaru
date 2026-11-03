use miniz_oxide::inflate::TINFLStatus;
use miniz_oxide::inflate::core::{DecompressorOxide, decompress, inflate_flags};

use super::Decoded;
use super::predictor::Predictor;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::Dictionary;

/// Decodes `/FlateDecode` data (ISO 32000-2, section 7.4.4), and undoes the
/// predictor named in `parameters`, the stream's `/DecodeParms`.
///
/// `offset` is where the data starts in the file, for errors and diagnostics.
/// Damaged data is common in real files, so whatever was decoded before the damage
/// is kept, with a diagnostic. It is only an error when nothing can be decoded.
pub fn decode_flate(
    data: &[u8],
    parameters: Option<&Dictionary>,
    offset: usize,
    limits: Limits,
) -> Result<Decoded> {
    let predictor = Predictor::from_parameters(parameters, offset)?;
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    if data.is_empty() {
        return Ok(Decoded {
            data: Vec::new(),
            diagnostics,
        });
    }

    let zlib = has_zlib_header(data);
    if !zlib {
        diagnostics.push(DiagnosticKind::MissingZlibHeader, offset);
    }
    let (inflated, status) = inflate(data, zlib, limits.max_decoded_length);
    match status {
        TINFLStatus::Done => {}
        TINFLStatus::HasMoreOutput => {
            return Err(Error::new(
                ErrorKind::LimitExceeded(Limit::DecodedLength),
                offset,
            ));
        }
        _ if inflated.is_empty() => {
            return Err(Error::new(ErrorKind::InvalidFlateData, offset));
        }
        _ => diagnostics.push(DiagnosticKind::DamagedFlateData, offset),
    }

    let data = match predictor {
        Some(predictor) => predictor.undo(&inflated, offset, &mut diagnostics)?,
        None => inflated,
    };
    Ok(Decoded { data, diagnostics })
}

// A zlib header is two bytes: the method, 8 for deflate, and a check that makes
// them a multiple of 31 (RFC 1950).
fn has_zlib_header(data: &[u8]) -> bool {
    match data {
        [method, flags, ..] => {
            method & 0x0F == 8 && (u16::from(*method) << 8 | u16::from(*flags)) % 31 == 0
        }
        _ => false,
    }
}

// Inflates up to `limit` bytes. Returns the output and the final status:
// `HasMoreOutput` if the limit was reached.
fn inflate(mut input: &[u8], zlib: bool, limit: usize) -> (Vec<u8>, TINFLStatus) {
    let mut flags = inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    if zlib {
        flags |= inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER;
    }
    let mut decompressor = Box::<DecompressorOxide>::default();
    let mut output = vec![0; input.len().saturating_mul(4).clamp(1, limit.max(1))];
    let mut position = 0;

    loop {
        let (status, consumed, produced) =
            decompress(&mut decompressor, input, &mut output, position, flags);
        position += produced;
        input = input.get(consumed..).unwrap_or_default();

        if status != TINFLStatus::HasMoreOutput || output.len() >= limit {
            output.truncate(position.min(limit));
            return (output, status);
        }
        let length = output.len().saturating_mul(2).min(limit);
        output.resize(length, 0);
    }
}
