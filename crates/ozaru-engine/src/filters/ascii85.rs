use super::Decoded;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::chars::is_whitespace;

/// Decodes `/ASCII85Decode` data (ISO 32000-2, section 7.4.3): groups of five
/// characters from `!` to `u` give four bytes, in base 85. `z` alone stands for
/// four zero bytes, whitespace is ignored and `~>` marks the end. A last group of
/// two to four characters gives one to three bytes.
///
/// `offset` is where the data starts in the file, for errors and diagnostics. Data
/// without `~>` is decoded to its end, with a diagnostic. A leading `<~`, written
/// by some PostScript tools, is skipped. Any other character, a group worth more
/// than four bytes, or a last group of one character is an error.
pub fn decode_ascii85(data: &[u8], offset: usize, limits: Limits) -> Result<Decoded> {
    let invalid = || Error::new(ErrorKind::InvalidAscii85Data, offset);
    let too_long = || Error::new(ErrorKind::LimitExceeded(Limit::DecodedLength), offset);
    let data = data.strip_prefix(b"<~").unwrap_or(data);
    let mut output = Vec::with_capacity(data.len() / 5 * 4 + 4);
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    let mut group = [0u8; 5];
    let mut length = 0;
    let mut ended = false;

    let mut bytes = data.iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            b'~' => {
                if bytes.find(|&b| !is_whitespace(b)) != Some(b'>') {
                    return Err(invalid());
                }
                ended = true;
                break;
            }
            b'z' if length == 0 => output.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                group[length] = byte - b'!';
                length += 1;
                if length == 5 {
                    output.extend_from_slice(&group_value(&group).ok_or_else(invalid)?);
                    length = 0;
                }
            }
            _ if is_whitespace(byte) => {}
            _ => return Err(invalid()),
        }
        if output.len() > limits.max_decoded_length {
            return Err(too_long());
        }
    }

    // A last group of n characters is padded with `u`, the largest digit, and gives
    // n - 1 bytes.
    match length {
        0 => {}
        1 => return Err(invalid()),
        _ => {
            group[length..].fill(b'u' - b'!');
            let bytes = group_value(&group).ok_or_else(invalid)?;
            output.extend_from_slice(&bytes[..length - 1]);
        }
    }
    if !ended {
        diagnostics.push(DiagnosticKind::MissingEndOfDataMarker, offset);
    }
    Ok(Decoded {
        data: output,
        diagnostics,
    })
}

// The four bytes of a group of five base-85 digits, or `None` if it's worth more
// than four bytes can hold.
fn group_value(group: &[u8; 5]) -> Option<[u8; 4]> {
    let value = group
        .iter()
        .fold(0u64, |value, &digit| value * 85 + u64::from(digit));
    u32::try_from(value).ok().map(u32::to_be_bytes)
}
