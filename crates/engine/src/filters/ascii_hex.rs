use super::Decoded;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limits, Result};
use crate::syntax::chars::{hex_value, is_whitespace};

/// Decodes `/ASCIIHexDecode` data (ISO 32000-2, section 7.4.2): two hexadecimal
/// digits per byte, in either case, with whitespace ignored and `>` marking the
/// end. A last digit without a pair counts as if it were followed by `0`.
///
/// `offset` is where the data starts in the file, for errors and diagnostics. Data
/// without `>` is decoded to its end, with a diagnostic. Any other character is an
/// error.
pub fn decode_ascii_hex(data: &[u8], offset: usize, limits: Limits) -> Result<Decoded> {
    let mut output = Vec::with_capacity(data.len() / 2);
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    let mut high: Option<u8> = None;
    let mut ended = false;

    for &byte in data {
        if byte == b'>' {
            ended = true;
            break;
        }
        if is_whitespace(byte) {
            continue;
        }
        let digit =
            hex_value(byte).ok_or_else(|| Error::new(ErrorKind::InvalidAsciiHexData, offset))?;
        match high.take() {
            None => high = Some(digit),
            Some(high) => output.push(high << 4 | digit),
        }
    }
    if let Some(high) = high {
        output.push(high << 4);
    }
    if !ended {
        diagnostics.push(DiagnosticKind::MissingEndOfDataMarker, offset);
    }
    Ok(Decoded {
        data: output,
        diagnostics,
    })
}
