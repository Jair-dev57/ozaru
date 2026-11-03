use super::Decoded;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};

/// Decodes `/RunLengthDecode` data (ISO 32000-2, section 7.4.5). Each run starts
/// with a length byte: 0 to 127 copies the next 1 to 128 bytes, 129 to 255
/// repeats the next byte 257 minus that many times, and 128 marks the end.
///
/// `offset` is where the data starts in the file, for errors and diagnostics. Data
/// without the end marker is decoded to its end, and a run cut short keeps the
/// bytes it has, each with a diagnostic.
pub fn decode_run_length(data: &[u8], offset: usize, limits: Limits) -> Result<Decoded> {
    let mut output = Vec::with_capacity(data.len());
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    let mut position = 0;
    let mut ended = false;

    while let Some(&length) = data.get(position) {
        position += 1;
        match length {
            128 => {
                ended = true;
                break;
            }
            0..=127 => {
                let end = position + usize::from(length) + 1;
                if end > data.len() {
                    diagnostics.push(DiagnosticKind::TruncatedFilterData, offset);
                }
                output.extend_from_slice(&data[position..end.min(data.len())]);
                position = end;
            }
            129..=255 => {
                let Some(&byte) = data.get(position) else {
                    diagnostics.push(DiagnosticKind::TruncatedFilterData, offset);
                    break;
                };
                position += 1;
                output.resize(output.len() + 257 - usize::from(length), byte);
            }
        }
        if output.len() > limits.max_decoded_length {
            return Err(Error::new(
                ErrorKind::LimitExceeded(Limit::DecodedLength),
                offset,
            ));
        }
    }
    if !ended && diagnostics.is_empty() {
        diagnostics.push(DiagnosticKind::MissingEndOfDataMarker, offset);
    }
    Ok(Decoded {
        data: output,
        diagnostics,
    })
}
