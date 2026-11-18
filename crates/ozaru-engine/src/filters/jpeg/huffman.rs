use super::invalid;
use crate::base::Result;

/// How many bits of a code the fast lookup table reads at once.
const FAST_BITS: u32 = 9;

/// A Huffman table of a JPEG (ITU-T T.81, Annex C and F.2.2.3): the symbols
/// are given by how many codes there are of each length from 1 to 16 bits.
#[derive(Debug, Clone)]
pub(super) struct HuffmanTable {
    /// For each `FAST_BITS` bits, the length of the code they start, or 0, and
    /// its symbol.
    fast: Vec<(u8, u8)>,
    /// For each length, the largest code of it, or -1 where there is none.
    max_code: [i32; 18],
    /// For each length, the first code of it minus the index of its symbol.
    offset: [i32; 17],
    symbols: Vec<u8>,
}

impl HuffmanTable {
    /// Makes the table of `counts` codes of each length (from 1) for
    /// `symbols`.
    pub fn new(counts: &[u8; 16], symbols: &[u8]) -> Result<Self> {
        let total: usize = counts.iter().map(|&c| usize::from(c)).sum();
        if total > 256 || total > symbols.len() {
            return Err(invalid());
        }
        let mut fast = vec![(0u8, 0u8); 1 << FAST_BITS];
        let mut max_code = [-1i32; 18];
        let mut offset = [0i32; 17];
        let mut code = 0i32;
        let mut index = 0usize;
        for length in 1..=16usize {
            let count = i32::from(counts[length - 1]);
            offset[length] = code - index as i32;
            for i in 0..count {
                if length as u32 <= FAST_BITS {
                    // Every run of bits that starts with this code.
                    let first = ((code + i) as usize) << (FAST_BITS as usize - length);
                    let last = first + (1 << (FAST_BITS as usize - length));
                    if last > fast.len() {
                        return Err(invalid());
                    }
                    for entry in &mut fast[first..last] {
                        *entry = (length as u8, symbols[index + i as usize]);
                    }
                }
            }
            index += count as usize;
            code += count;
            // The codes of a length must fit its bits.
            if code > 1 << length {
                return Err(invalid());
            }
            max_code[length] = if count > 0 { code - 1 } else { -1 };
            code <<= 1;
        }
        Ok(Self {
            fast,
            max_code,
            offset,
            symbols: symbols[..total].to_vec(),
        })
    }

    /// Decodes a symbol, or `None` for bits that are no code.
    pub fn decode(&self, bits: &mut Bits<'_>) -> Option<u8> {
        bits.fill();
        let peek = bits.peek(FAST_BITS);
        let (length, symbol) = self.fast[peek as usize];
        if length > 0 {
            bits.skip(u32::from(length));
            return Some(symbol);
        }
        let code = bits.peek(16) as i32;
        for length in (FAST_BITS + 1)..=16 {
            let value = code >> (16 - length);
            let max = self.max_code[length as usize];
            if max >= 0 && value <= max {
                let index = value - self.offset[length as usize];
                let symbol = *self.symbols.get(usize::try_from(index).ok()?)?;
                bits.skip(length);
                return Some(symbol);
            }
        }
        None
    }
}

/// The bits of the entropy-coded data of a scan: bytes with `0xFF` stuffed
/// with a zero after it, until a marker.
pub(super) struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    buffer: u64,
    count: u32,
    /// The marker the data ended at, when it did.
    pub marker: Option<u8>,
    /// How many zero bytes were supplied past the end of the data.
    pub overrun: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8], position: usize) -> Self {
        Self {
            data,
            position,
            buffer: 0,
            count: 0,
            marker: None,
            overrun: 0,
        }
    }

    /// Where the next byte not read yet is.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Makes sure at least 32 bits are in the buffer, with zeros past the end
    /// of the data or at a marker.
    fn fill(&mut self) {
        while self.count <= 32 {
            let byte = if self.marker.is_some() {
                self.overrun += 1;
                0
            } else {
                match self.data.get(self.position) {
                    None => {
                        self.marker = Some(0xD9);
                        self.overrun += 1;
                        0
                    }
                    Some(&0xFF) => match self.data.get(self.position + 1) {
                        Some(&0) => {
                            self.position += 2;
                            0xFF
                        }
                        Some(&0xFF) => {
                            // Fill bytes before a marker.
                            self.position += 1;
                            continue;
                        }
                        Some(&marker) => {
                            self.marker = Some(marker);
                            self.overrun += 1;
                            0
                        }
                        None => {
                            self.marker = Some(0xD9);
                            self.overrun += 1;
                            0
                        }
                    },
                    Some(&byte) => {
                        self.position += 1;
                        byte
                    }
                }
            };
            self.buffer |= u64::from(byte) << (56 - self.count);
            self.count += 8;
        }
    }

    fn peek(&self, count: u32) -> u32 {
        (self.buffer >> (64 - count)) as u32
    }

    fn skip(&mut self, count: u32) {
        self.buffer <<= count;
        self.count = self.count.saturating_sub(count);
    }

    /// `count` bits, 0 to 16, as a number.
    pub fn bits(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        self.fill();
        let value = self.peek(count);
        self.skip(count);
        value
    }

    pub fn bit(&mut self) -> bool {
        self.bits(1) == 1
    }

    /// The bits that were read before a restart marker are dropped, and the
    /// marker is read: `true` if it was the one expected, `RST` `index`.
    pub fn restart(&mut self, index: u8) -> bool {
        self.buffer = 0;
        self.count = 0;
        self.overrun = 0;
        // Without a marker found yet, look for the next one.
        if self.marker.is_none() {
            while self.position + 1 < self.data.len() {
                if self.data[self.position] == 0xFF
                    && !matches!(self.data[self.position + 1], 0 | 0xFF)
                {
                    self.marker = Some(self.data[self.position + 1]);
                    break;
                }
                self.position += 1;
            }
        }
        match self.marker {
            Some(marker) if marker == 0xD0 + (index & 7) => {
                self.position += 2;
                self.marker = None;
                true
            }
            _ => false,
        }
    }
}
