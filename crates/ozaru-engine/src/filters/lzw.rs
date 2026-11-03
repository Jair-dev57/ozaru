use super::Decoded;
use super::predictor::Predictor;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object};

const CLEAR_TABLE: u16 = 256;
const END_OF_DATA: u16 = 257;
const FIRST_ENTRY: u16 = 258;
const TABLE_SIZE: usize = 4096;
const MAX_WIDTH: u32 = 12;

/// Decodes `/LZWDecode` data (ISO 32000-2, section 7.4.4), and undoes the
/// predictor named in `parameters`, the stream's `/DecodeParms`.
///
/// Codes are 9 to 12 bits wide. 256 clears the table and 257 ends the data; each
/// code from 258 on stands for an earlier code plus one byte. `/EarlyChange`, 1 by
/// default, makes codes one bit wider one code earlier, as the original LZW
/// writers did.
///
/// `offset` is where the data starts in the file, for errors and diagnostics. Data
/// without code 257 is decoded to its end, with a diagnostic. A code that isn't in
/// the table yet is an error.
pub fn decode_lzw(
    data: &[u8],
    parameters: Option<&Dictionary>,
    offset: usize,
    limits: Limits,
) -> Result<Decoded> {
    let invalid_parameters = || Error::new(ErrorKind::InvalidDecodeParameters, offset);
    let early_change = match parameters.and_then(|parameters| parameters.get("EarlyChange")) {
        None => 1,
        Some(Object::Integer(value @ (0 | 1))) => *value as usize,
        Some(_) => return Err(invalid_parameters()),
    };
    let predictor = Predictor::from_parameters(parameters, offset)?;
    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);

    let inflated = inflate(data, early_change, offset, limits, &mut diagnostics)?;
    let data = match predictor {
        Some(predictor) => predictor.undo(&inflated, offset, &mut diagnostics)?,
        None => inflated,
    };
    Ok(Decoded { data, diagnostics })
}

// An entry of the table: the code it extends, its last byte, its first byte and
// its length. Codes below 256 stand for themselves and aren't stored.
#[derive(Clone, Copy)]
struct Entry {
    prefix: u16,
    last: u8,
    first: u8,
    length: u16,
}

fn inflate(
    data: &[u8],
    early_change: usize,
    offset: usize,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Result<Vec<u8>> {
    let invalid = || Error::new(ErrorKind::InvalidLzwData, offset);
    let mut table: Vec<Entry> = Vec::with_capacity(TABLE_SIZE);
    let mut bits = Bits::new(data);
    let mut width = 9;
    let mut previous: Option<u16> = None;
    let mut output = Vec::with_capacity(data.len() * 2);
    let mut ended = false;

    while let Some(code) = bits.read(width) {
        if code == CLEAR_TABLE {
            table.clear();
            width = 9;
            previous = None;
            continue;
        }
        if code == END_OF_DATA {
            ended = true;
            break;
        }
        let next = FIRST_ENTRY + table.len() as u16;
        let first = match (code, previous) {
            (0..=255, _) => {
                output.push(code as u8);
                code as u8
            }
            _ if code < next => write_entry(&table, code, &mut output),
            // The code being defined right now: the previous one plus its own first
            // byte.
            (_, Some(previous)) if code == next => {
                let first = write_entry(&table, previous, &mut output);
                output.push(first);
                first
            }
            _ => return Err(invalid()),
        };
        if let Some(previous) = previous
            && table.len() < TABLE_SIZE - usize::from(FIRST_ENTRY)
        {
            table.push(Entry {
                prefix: previous,
                last: first,
                first: first_byte(&table, previous),
                length: length(&table, previous) + 1,
            });
        }
        let next = usize::from(FIRST_ENTRY) + table.len();
        while width < MAX_WIDTH && next + early_change >= 1 << width {
            width += 1;
        }
        previous = Some(code);

        if output.len() > limits.max_decoded_length {
            return Err(Error::new(
                ErrorKind::LimitExceeded(Limit::DecodedLength),
                offset,
            ));
        }
    }
    if !ended {
        diagnostics.push(DiagnosticKind::MissingEndOfDataMarker, offset);
    }
    Ok(output)
}

// Appends the bytes `code` stands for and returns the first of them.
fn write_entry(table: &[Entry], code: u16, output: &mut Vec<u8>) -> u8 {
    let start = output.len();
    output.resize(start + usize::from(length(table, code)), 0);
    let mut code = code;
    for slot in output[start..].iter_mut().rev() {
        match code {
            0..=255 => *slot = code as u8,
            _ => {
                let entry = table[usize::from(code - FIRST_ENTRY)];
                *slot = entry.last;
                code = entry.prefix;
            }
        }
    }
    output[start]
}

fn first_byte(table: &[Entry], code: u16) -> u8 {
    match code {
        0..=255 => code as u8,
        _ => table[usize::from(code - FIRST_ENTRY)].first,
    }
}

fn length(table: &[Entry], code: u16) -> u16 {
    match code {
        0..=255 => 1,
        _ => table[usize::from(code - FIRST_ENTRY)].length,
    }
}

// Reads codes of any width up to 16 bits, most significant bit first.
struct Bits<'a> {
    data: &'a [u8],
    buffer: u32,
    count: u32,
    position: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            buffer: 0,
            count: 0,
            position: 0,
        }
    }

    // `None` once fewer than `width` bits are left.
    fn read(&mut self, width: u32) -> Option<u16> {
        while self.count < width {
            let byte = *self.data.get(self.position)?;
            self.position += 1;
            self.buffer = self.buffer << 8 | u32::from(byte);
            self.count += 8;
        }
        self.count -= width;
        let code = (self.buffer >> self.count) & ((1 << width) - 1);
        self.buffer &= (1 << self.count) - 1;
        Some(code as u16)
    }
}
