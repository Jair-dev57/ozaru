//! `/CCITTFaxDecode`: the fax encodings of ITU-T recommendations T.4, Group 3
//! one and two dimensional, and T.6, Group 4 (ISO 32000-2, section 7.4.6).

use std::sync::OnceLock;

use super::Decoded;
use crate::base::{Diagnostics, Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object};

/// The `/DecodeParms` of `/CCITTFaxDecode` (ISO 32000-2, table 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CcittParameters {
    /// Less than 0: Group 4. 0: Group 3, one-dimensional. More than 0: Group 3,
    /// each line one- or two-dimensional, as its tag bit says.
    pub k: i64,
    /// Each line starts on a byte.
    pub encoded_byte_align: bool,
    pub columns: usize,
    /// How many rows there are, if the parameters say: 0 if they don't.
    pub rows: usize,
    /// 1 bits are black, instead of white.
    pub black_is_1: bool,
}

impl CcittParameters {
    /// The parameters in `parameters`, with the defaults for those missing.
    pub fn read(parameters: Option<&Dictionary>) -> Self {
        let integer = |key: &str, default: i64| {
            parameters
                .and_then(|p| p.get(key))
                .and_then(Object::as_integer)
                .unwrap_or(default)
        };
        let boolean = |key: &str| {
            matches!(
                parameters.and_then(|p| p.get(key)),
                Some(Object::Boolean(true))
            )
        };
        Self {
            k: integer("K", 0),
            encoded_byte_align: boolean("EncodedByteAlign"),
            columns: usize::try_from(integer("Columns", 1728)).unwrap_or(0),
            rows: usize::try_from(integer("Rows", 0)).unwrap_or(0),
            black_is_1: boolean("BlackIs1"),
        }
    }
}

/// Decodes fax-encoded `data` into rows of one bit per pixel, each starting on
/// a byte, 0 for black and 1 for white, the colors of `/DeviceGray`.
///
/// It never fails on damaged data: rows are decoded until a code can't be
/// read, and the rest of the image, if the parameters give its rows, is
/// white. End-of-line codes are skipped wherever they are. It is an
/// `InvalidDecodeParameters` error if there are no columns, and a
/// `LimitExceeded` error past `max_decoded_length` bytes.
pub fn decode_ccitt(
    data: &[u8],
    parameters: &CcittParameters,
    offset: usize,
    limits: Limits,
) -> Result<Decoded> {
    let columns = parameters.columns;
    if columns == 0 || columns > 1 << 20 {
        return Err(Error::new(ErrorKind::InvalidDecodeParameters, offset));
    }
    let row_bytes = columns.div_ceil(8);
    let mut reader = Bits::new(data);
    let mut out = Vec::new();
    let too_long = || Error::new(ErrorKind::LimitExceeded(Limit::DecodedLength), offset);
    // The changing elements of the line above, the reference line: where each
    // run of a color starts, first a black one. An imaginary white line comes
    // before the first.
    let mut reference: Vec<usize> = Vec::new();
    let mut rows = 0;
    loop {
        if parameters.rows > 0 && rows == parameters.rows {
            break;
        }
        // An end of line comes with the fill bits that align it; without
        // one, a byte-aligned line starts on the next byte.
        let mut ends = reader.skip_end_of_lines();
        if parameters.encoded_byte_align && ends == 0 {
            reader.align();
            ends = reader.skip_end_of_lines();
        }
        if ends >= 2 || reader.at_end() {
            // RTC in Group 3, EOFB in Group 4, or the end of the data.
            break;
        }
        let two_dimensional = match parameters.k {
            k if k < 0 => true,
            0 => false,
            _ => match reader.bit() {
                Some(tag) => tag == 0,
                None => break,
            },
        };
        let line = if two_dimensional {
            line_2d(&mut reader, &reference, columns)
        } else {
            line_1d(&mut reader, columns)
        };
        let Some(line) = line else {
            break;
        };
        if out.len() + row_bytes > limits.max_decoded_length {
            return Err(too_long());
        }
        write_row(&mut out, &line, columns, parameters.black_is_1);
        reference = line;
        rows += 1;
    }
    // The rows the data didn't give are white.
    while rows < parameters.rows {
        if out.len() + row_bytes > limits.max_decoded_length {
            return Err(too_long());
        }
        write_row(&mut out, &[], columns, parameters.black_is_1);
        rows += 1;
    }
    Ok(Decoded {
        data: out,
        diagnostics: Diagnostics::new(limits.max_diagnostics),
    })
}

// Writes the row whose changing elements are `changes`, 0 for black and 1 for
// white, or the other way round with BlackIs1.
fn write_row(out: &mut Vec<u8>, changes: &[usize], columns: usize, black_is_1: bool) {
    let start = out.len();
    let white_bit: u8 = if black_is_1 { 0 } else { 1 };
    out.resize(
        start + columns.div_ceil(8),
        if white_bit == 1 { 0xFF } else { 0 },
    );
    let row = &mut out[start..];
    for pair in changes.chunks(2) {
        let from = pair[0].min(columns);
        let to = pair.get(1).copied().unwrap_or(columns).min(columns);
        for x in from..to {
            let mask = 0x80 >> (x % 8);
            if white_bit == 1 {
                row[x / 8] &= !mask;
            } else {
                row[x / 8] |= mask;
            }
        }
    }
    // The bits past the last column are 0, as rows are padded.
    let rest = columns % 8;
    if rest != 0 {
        let last = row.len() - 1;
        row[last] &= 0xFF << (8 - rest);
    }
}

// A one-dimensional line: runs of white and black in turn, from white, each a
// series of make-up codes and a terminating code. `None` if a code can't be
// read.
fn line_1d(reader: &mut Bits<'_>, columns: usize) -> Option<Vec<usize>> {
    let mut changes = Vec::new();
    let mut position = 0;
    let mut white = true;
    while position < columns {
        let run = reader.run(white)?;
        position = (position + run).min(columns);
        if position < columns {
            changes.push(position);
        }
        white = !white;
    }
    Some(cancelled(changes))
}

// The changes of a line without the pairs that cancel, from runs of 0.
fn cancelled(changes: Vec<usize>) -> Vec<usize> {
    let mut clean: Vec<usize> = Vec::with_capacity(changes.len());
    for p in changes {
        if clean.last() == Some(&p) {
            clean.pop();
        } else {
            clean.push(p);
        }
    }
    clean
}

// A two-dimensional line, coded against `reference`, the changing elements
// of the line above (T.4, section 4.2). `None` if a code can't be read.
fn line_2d(reader: &mut Bits<'_>, reference: &[usize], columns: usize) -> Option<Vec<usize>> {
    let mut changes: Vec<usize> = Vec::new();
    // a0, the start of the run being coded; before the line, at -1.
    let mut a0: Option<usize> = None;
    let mut white = true;
    loop {
        let start = a0.unwrap_or(0);
        if a0.is_some() && start >= columns {
            break;
        }
        // b1, the first changing element of the reference line after a0 of
        // the color opposite to a0's, and b2, the one after it.
        let first = reference
            .iter()
            .enumerate()
            .position(|(i, &p)| (i % 2 == 0) == white && a0.is_none_or(|a| p > a))
            .unwrap_or(reference.len());
        let b1 = reference.get(first).copied().unwrap_or(columns);
        let b2 = reference.get(first + 1).copied().unwrap_or(columns);
        // A line has at most a change for each column; damaged data that
        // goes on changing without moving is stopped.
        if changes.len() > 2 * columns + 2 {
            return None;
        }
        match reader.mode()? {
            Mode::Pass => {
                a0 = Some(b2);
            }
            Mode::Horizontal => {
                let first_run = reader.run(white)?;
                let second_run = reader.run(!white)?;
                let a1 = (start + first_run).min(columns);
                let a2 = (a1 + second_run).min(columns);
                changes.push(a1);
                changes.push(a2);
                a0 = Some(a2);
            }
            Mode::Vertical(delta) => {
                let a1 = usize::try_from(b1 as i64 + delta).ok()?.min(columns);
                if a0.is_some_and(|a| a1 < a) {
                    return None;
                }
                changes.push(a1);
                a0 = Some(a1);
                white = !white;
            }
        }
        if a0.is_some_and(|a| a >= columns) {
            break;
        }
    }
    // Changes at the end of the line aren't changes.
    while changes.last().is_some_and(|&p| p >= columns) {
        changes.pop();
    }
    // A horizontal mode may give runs of 0: the changes they make cancel.
    Some(cancelled(changes))
}

enum Mode {
    Pass,
    Horizontal,
    Vertical(i64),
}

// The data, read a bit at a time from the most significant bit of each byte.
struct Bits<'d> {
    data: &'d [u8],
    position: usize,
}

impl<'d> Bits<'d> {
    fn new(data: &'d [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn at_end(&self) -> bool {
        self.position >= self.data.len() * 8
    }

    fn bit(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.position / 8)?;
        let bit = (byte >> (7 - self.position % 8)) & 1;
        self.position += 1;
        Some(bit)
    }

    // The next `count` bits, up to 16, as a number; 0 bits past the end.
    fn peek(&self, count: usize) -> u32 {
        let mut value = 0;
        for i in 0..count {
            let at = self.position + i;
            let bit = self
                .data
                .get(at / 8)
                .map_or(0, |byte| (byte >> (7 - at % 8)) & 1);
            value = value << 1 | u32::from(bit);
        }
        value
    }

    fn align(&mut self) {
        self.position = self.position.div_ceil(8) * 8;
    }

    // Skips end-of-line codes, eleven 0 bits or more and a 1, with the fill
    // bits before them, and says how many there were.
    fn skip_end_of_lines(&mut self) -> usize {
        let mut count = 0;
        loop {
            let mut zeros = 0;
            while self.peek(1) == 0 && !self.at_end() && zeros < 64 {
                zeros += 1;
                self.position += 1;
            }
            if zeros >= 11 && !self.at_end() {
                self.position += 1;
                count += 1;
            } else {
                // Not an end of line: put the zeros back.
                self.position -= zeros;
                return count;
            }
        }
    }

    // The length of a run of white or black: make-up codes, then a
    // terminating code.
    fn run(&mut self, white: bool) -> Option<usize> {
        let table = if white { white_codes() } else { black_codes() };
        let mut total = 0;
        loop {
            let (length, run) = table[self.peek(13) as usize];
            if length == 0 {
                return None;
            }
            self.position += usize::from(length);
            total += usize::from(run);
            if run < 64 {
                return Some(total);
            }
        }
    }

    fn mode(&mut self) -> Option<Mode> {
        let bits = self.peek(7);
        let (length, mode) = if bits >> 6 == 1 {
            (1, Mode::Vertical(0))
        } else if bits >> 4 == 0b011 {
            (3, Mode::Vertical(1))
        } else if bits >> 4 == 0b010 {
            (3, Mode::Vertical(-1))
        } else if bits >> 4 == 0b001 {
            (3, Mode::Horizontal)
        } else if bits >> 3 == 0b0001 {
            (4, Mode::Pass)
        } else if bits >> 1 == 0b000011 {
            (6, Mode::Vertical(2))
        } else if bits >> 1 == 0b000010 {
            (6, Mode::Vertical(-2))
        } else if bits == 0b0000011 {
            (7, Mode::Vertical(3))
        } else if bits == 0b0000010 {
            (7, Mode::Vertical(-3))
        } else {
            // Uncompressed mode, an end of line or damage.
            return None;
        };
        self.position += length;
        Some(mode)
    }
}

// For each 13 bits, the length of the code they start with and its run; a
// length of 0 if they start with no code.
type CodeTable = Vec<(u8, u16)>;

fn table(codes: &[(&str, u16)]) -> CodeTable {
    let mut table = vec![(0, 0); 1 << 13];
    for &(bits, run) in codes.iter().chain(EXTENDED_MAKE_UP) {
        let length = bits.len();
        let code = u32::from_str_radix(bits, 2).expect("codes are binary");
        let first = (code << (13 - length)) as usize;
        for entry in &mut table[first..first + (1 << (13 - length))] {
            *entry = (length as u8, run);
        }
    }
    table
}

fn white_codes() -> &'static CodeTable {
    static TABLE: OnceLock<CodeTable> = OnceLock::new();
    TABLE.get_or_init(|| table(WHITE))
}

fn black_codes() -> &'static CodeTable {
    static TABLE: OnceLock<CodeTable> = OnceLock::new();
    TABLE.get_or_init(|| table(BLACK))
}

// The white run lengths of T.4, tables 2 and 3: terminating codes, then
// make-up codes.
const WHITE: &[(&str, u16)] = &[
    ("00110101", 0),
    ("000111", 1),
    ("0111", 2),
    ("1000", 3),
    ("1011", 4),
    ("1100", 5),
    ("1110", 6),
    ("1111", 7),
    ("10011", 8),
    ("10100", 9),
    ("00111", 10),
    ("01000", 11),
    ("001000", 12),
    ("000011", 13),
    ("110100", 14),
    ("110101", 15),
    ("101010", 16),
    ("101011", 17),
    ("0100111", 18),
    ("0001100", 19),
    ("0001000", 20),
    ("0010111", 21),
    ("0000011", 22),
    ("0000100", 23),
    ("0101000", 24),
    ("0101011", 25),
    ("0010011", 26),
    ("0100100", 27),
    ("0011000", 28),
    ("00000010", 29),
    ("00000011", 30),
    ("00011010", 31),
    ("00011011", 32),
    ("00010010", 33),
    ("00010011", 34),
    ("00010100", 35),
    ("00010101", 36),
    ("00010110", 37),
    ("00010111", 38),
    ("00101000", 39),
    ("00101001", 40),
    ("00101010", 41),
    ("00101011", 42),
    ("00101100", 43),
    ("00101101", 44),
    ("00000100", 45),
    ("00000101", 46),
    ("00001010", 47),
    ("00001011", 48),
    ("01010010", 49),
    ("01010011", 50),
    ("01010100", 51),
    ("01010101", 52),
    ("00100100", 53),
    ("00100101", 54),
    ("01011000", 55),
    ("01011001", 56),
    ("01011010", 57),
    ("01011011", 58),
    ("01001010", 59),
    ("01001011", 60),
    ("00110010", 61),
    ("00110011", 62),
    ("00110100", 63),
    ("11011", 64),
    ("10010", 128),
    ("010111", 192),
    ("0110111", 256),
    ("00110110", 320),
    ("00110111", 384),
    ("01100100", 448),
    ("01100101", 512),
    ("01101000", 576),
    ("01100111", 640),
    ("011001100", 704),
    ("011001101", 768),
    ("011010010", 832),
    ("011010011", 896),
    ("011010100", 960),
    ("011010101", 1024),
    ("011010110", 1088),
    ("011010111", 1152),
    ("011011000", 1216),
    ("011011001", 1280),
    ("011011010", 1344),
    ("011011011", 1408),
    ("010011000", 1472),
    ("010011001", 1536),
    ("010011010", 1600),
    ("011000", 1664),
    ("010011011", 1728),
];

// The black run lengths of T.4, tables 2 and 3.
const BLACK: &[(&str, u16)] = &[
    ("0000110111", 0),
    ("010", 1),
    ("11", 2),
    ("10", 3),
    ("011", 4),
    ("0011", 5),
    ("0010", 6),
    ("00011", 7),
    ("000101", 8),
    ("000100", 9),
    ("0000100", 10),
    ("0000101", 11),
    ("0000111", 12),
    ("00000100", 13),
    ("00000111", 14),
    ("000011000", 15),
    ("0000010111", 16),
    ("0000011000", 17),
    ("0000001000", 18),
    ("00001100111", 19),
    ("00001101000", 20),
    ("00001101100", 21),
    ("00000110111", 22),
    ("00000101000", 23),
    ("00000010111", 24),
    ("00000011000", 25),
    ("000011001010", 26),
    ("000011001011", 27),
    ("000011001100", 28),
    ("000011001101", 29),
    ("000001101000", 30),
    ("000001101001", 31),
    ("000001101010", 32),
    ("000001101011", 33),
    ("000011010010", 34),
    ("000011010011", 35),
    ("000011010100", 36),
    ("000011010101", 37),
    ("000011010110", 38),
    ("000011010111", 39),
    ("000001101100", 40),
    ("000001101101", 41),
    ("000011011010", 42),
    ("000011011011", 43),
    ("000001010100", 44),
    ("000001010101", 45),
    ("000001010110", 46),
    ("000001010111", 47),
    ("000001100100", 48),
    ("000001100101", 49),
    ("000001010010", 50),
    ("000001010011", 51),
    ("000000100100", 52),
    ("000000110111", 53),
    ("000000111000", 54),
    ("000000100111", 55),
    ("000000101000", 56),
    ("000001011000", 57),
    ("000001011001", 58),
    ("000000101011", 59),
    ("000000101100", 60),
    ("000001011010", 61),
    ("000001100110", 62),
    ("000001100111", 63),
    ("0000001111", 64),
    ("000011001000", 128),
    ("000011001001", 192),
    ("000001011011", 256),
    ("000000110011", 320),
    ("000000110100", 384),
    ("000000110101", 448),
    ("0000001101100", 512),
    ("0000001101101", 576),
    ("0000001001010", 640),
    ("0000001001011", 704),
    ("0000001001100", 768),
    ("0000001001101", 832),
    ("0000001110010", 896),
    ("0000001110011", 960),
    ("0000001110100", 1024),
    ("0000001110101", 1088),
    ("0000001110110", 1152),
    ("0000001110111", 1216),
    ("0000001010010", 1280),
    ("0000001010011", 1344),
    ("0000001010100", 1408),
    ("0000001010101", 1472),
    ("0000001011010", 1536),
    ("0000001011011", 1600),
    ("0000001100100", 1664),
    ("0000001100101", 1728),
];

// The make-up codes of T.4, table 3, shared by white and black runs.
const EXTENDED_MAKE_UP: &[(&str, u16)] = &[
    ("00000001000", 1792),
    ("00000001100", 1856),
    ("00000001101", 1920),
    ("000000010010", 1984),
    ("000000010011", 2048),
    ("000000010100", 2112),
    ("000000010101", 2176),
    ("000000010110", 2240),
    ("000000010111", 2304),
    ("000000011100", 2368),
    ("000000011101", 2432),
    ("000000011110", 2496),
    ("000000011111", 2560),
];
