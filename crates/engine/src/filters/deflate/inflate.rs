//! Inflate: decoding of Deflate data (RFC 1951), with or without the zlib
//! wrapper (RFC 1950).

use super::adler32;

/// How inflating ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflateStatus {
    /// The data ended where it should, and the checksum, if there is one, matched.
    Done,
    /// The data is cut short or damaged, or its checksum is wrong: what was
    /// decoded before the problem is kept.
    Damaged,
    /// The data has more bytes than the limit allows.
    LimitReached,
}

// Lengths and distances: base values and extra bits (RFC 1951, section 3.2.5).
pub(super) const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
pub(super) const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
pub(super) const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
pub(super) const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
// The order the code lengths of the code lengths are stored in.
pub(super) const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

const MAX_BITS: usize = 15;
// Codes up to this long are decoded with one table lookup.
const FAST_BITS: usize = 9;

// A canonical Huffman code, decoded with a table for short codes and by counting
// for long ones.
struct Huffman {
    // For each `FAST_BITS` of input: `(length << 12) | symbol`, or 0 if the code
    // is longer. Symbols are under 4096.
    fast: [u16; 1 << FAST_BITS],
    // How many codes there are of each length.
    count: [u16; MAX_BITS + 1],
    // The symbols ordered by code.
    symbols: Vec<u16>,
}

impl Huffman {
    // The code for `lengths`. It is `None` if the lengths over-subscribe the code
    // space; an incomplete code is allowed, as RFC 1951 allows it for one code.
    #[allow(clippy::needless_range_loop)]
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut count = [0u16; MAX_BITS + 1];
        for &length in lengths {
            count[usize::from(length)] += 1;
        }
        count[0] = 0;
        let mut left: i32 = 1;
        for length in 1..=MAX_BITS {
            left = (left << 1) - i32::from(count[length]);
            if left < 0 {
                return None;
            }
        }
        let mut offsets = [0u16; MAX_BITS + 2];
        for length in 1..=MAX_BITS {
            offsets[length + 1] = offsets[length] + count[length];
        }
        let mut symbols = vec![0u16; usize::from(offsets[MAX_BITS + 1])];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[usize::from(offsets[usize::from(length)])] = symbol as u16;
                offsets[usize::from(length)] += 1;
            }
        }
        // The fast table: every code of `FAST_BITS` or fewer, repeated for each
        // way its unused bits can be. Codes are read least significant bit first,
        // so the table is indexed by the bit-reversed code.
        let mut fast = [0u16; 1 << FAST_BITS];
        let mut code: u32 = 0;
        let mut index = 0;
        for length in 1..=FAST_BITS {
            for _ in 0..count[length] {
                let symbol = symbols[index];
                index += 1;
                let reversed = (code.reverse_bits() >> (32 - length)) as usize;
                let entry = ((length as u16) << 12) | symbol;
                let mut slot = reversed;
                while slot < (1 << FAST_BITS) {
                    fast[slot] = entry;
                    slot += 1 << length;
                }
                code += 1;
            }
            code <<= 1;
        }
        Some(Self {
            fast,
            count,
            symbols,
        })
    }
}

// Reads bits, least significant first.
struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    buffer: u64,
    count: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            buffer: 0,
            count: 0,
        }
    }

    // Fills the buffer to at least 32 bits when the data has them.
    #[inline]
    fn refill(&mut self) {
        // Bits above `count` may be left from an earlier word: they are the next
        // bytes, but data read directly from `position` skips them.
        self.buffer &= (1u64 << self.count) - 1;
        if let Some(word) = self.data.get(self.position..self.position + 8) {
            let word = u64::from_le_bytes(word.try_into().expect("8 bytes"));
            self.buffer |= word << self.count;
            let bytes = (63 - self.count) / 8;
            self.position += bytes as usize;
            self.count += bytes * 8;
            return;
        }
        while self.count < 56 && self.position < self.data.len() {
            self.buffer |= u64::from(self.data[self.position]) << self.count;
            self.position += 1;
            self.count += 8;
        }
    }

    // The next `n` bits (at most 32), or `None` if the data has fewer.
    #[inline]
    fn take(&mut self, n: u32) -> Option<u32> {
        if self.count < n {
            self.refill();
            if self.count < n {
                return None;
            }
        }
        let value = (self.buffer & ((1u64 << n) - 1)) as u32;
        self.buffer >>= n;
        self.count -= n;
        Some(value)
    }

    // The symbol of the next code, or `None` if the data ends or the code is
    // invalid.
    #[inline]
    fn symbol(&mut self, huffman: &Huffman) -> Option<u16> {
        if self.count < MAX_BITS as u32 {
            self.refill();
        }
        let entry = huffman.fast[(self.buffer & ((1 << FAST_BITS) - 1)) as usize];
        if entry != 0 {
            let length = u32::from(entry >> 12);
            if length > self.count {
                return None;
            }
            self.buffer >>= length;
            self.count -= length;
            return Some(entry & 0x0FFF);
        }
        // A longer code: read bit by bit, counting.
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        let mut buffer = self.buffer;
        for length in 1..=MAX_BITS {
            if length as u32 > self.count {
                return None;
            }
            code |= (buffer & 1) as i32;
            buffer >>= 1;
            let count = i32::from(huffman.count[length]);
            if code - count < first {
                self.buffer >>= length;
                self.count -= length as u32;
                return huffman
                    .symbols
                    .get((index + (code - first)) as usize)
                    .copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }

    // Drops the bits up to the next byte boundary.
    fn align(&mut self) {
        let drop = self.count % 8;
        self.buffer >>= drop;
        self.count -= drop;
    }

    // Copies `length` whole bytes, after `align`.
    fn bytes(&mut self, length: usize, out: &mut Vec<u8>) -> bool {
        let mut left = length;
        // First what the bit buffer holds.
        while left > 0 && self.count >= 8 {
            out.push(self.buffer as u8);
            self.buffer >>= 8;
            self.count -= 8;
            left -= 1;
        }
        let available = self.data.len() - self.position;
        let take = left.min(available);
        out.extend_from_slice(&self.data[self.position..self.position + take]);
        self.position += take;
        take == left
    }

    // Where reading stopped, in bytes, counting the bits left in the buffer as
    // not read.
    fn consumed(&self) -> usize {
        self.position - (self.count / 8) as usize
    }
}

/// Inflates `input`: with the zlib wrapper if `zlib` is set, which is checked, and
/// the checksum at its end; without it otherwise. At most `limit` bytes are
/// returned. Bytes after the end of the data are ignored.
pub fn inflate(input: &[u8], zlib: bool, limit: usize) -> (Vec<u8>, InflateStatus) {
    let mut out = Vec::with_capacity(input.len().saturating_mul(3).min(limit).min(1 << 26));
    let body = if zlib {
        match input {
            [method, flags, rest @ ..]
                if method & 0x0F == 8
                    && (u16::from(*method) << 8 | u16::from(*flags)) % 31 == 0
                    && flags & 0x20 == 0 =>
            {
                rest
            }
            _ => return (out, InflateStatus::Damaged),
        }
    } else {
        input
    };
    let mut bits = Bits::new(body);
    let status = blocks(&mut bits, &mut out, limit);
    if status != InflateStatus::Done || !zlib {
        return (out, status);
    }
    // The Adler-32 checksum follows, on a byte boundary.
    bits.align();
    let at = bits.consumed();
    let stored = body.get(at..at + 4);
    let matches = stored.is_some_and(|stored| {
        u32::from_be_bytes(stored.try_into().expect("4 bytes")) == adler32(&out)
    });
    (
        out,
        if matches {
            InflateStatus::Done
        } else {
            InflateStatus::Damaged
        },
    )
}

fn blocks(bits: &mut Bits<'_>, out: &mut Vec<u8>, limit: usize) -> InflateStatus {
    use InflateStatus::*;
    loop {
        let Some(header) = bits.take(3) else {
            return Damaged;
        };
        let last = header & 1 == 1;
        let result = match header >> 1 {
            0 => stored_block(bits, out, limit),
            1 => {
                let (literals, distances) = fixed_codes();
                codes_block(bits, out, limit, &literals, &distances)
            }
            2 => match dynamic_codes(bits) {
                Some((literals, distances)) => codes_block(bits, out, limit, &literals, &distances),
                None => Damaged,
            },
            _ => Damaged,
        };
        if result != Done {
            return result;
        }
        if last {
            return Done;
        }
    }
}

fn stored_block(bits: &mut Bits<'_>, out: &mut Vec<u8>, limit: usize) -> InflateStatus {
    use InflateStatus::*;
    bits.align();
    let (Some(length), Some(inverse)) = (bits.take(16), bits.take(16)) else {
        return Damaged;
    };
    if length != !inverse & 0xFFFF {
        return Damaged;
    }
    let length = length as usize;
    if out.len() + length > limit {
        let room = limit - out.len();
        bits.bytes(room, out);
        return LimitReached;
    }
    if bits.bytes(length, out) {
        Done
    } else {
        Damaged
    }
}

fn fixed_codes() -> (Huffman, Huffman) {
    let mut lengths = [8u8; 288];
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    let literals = Huffman::new(&lengths).expect("the fixed code is complete");
    let distances = Huffman::new(&[5u8; 30]).expect("the fixed distance code is valid");
    (literals, distances)
}

fn dynamic_codes(bits: &mut Bits<'_>) -> Option<(Huffman, Huffman)> {
    let literal_count = bits.take(5)? as usize + 257;
    let distance_count = bits.take(5)? as usize + 1;
    let length_count = bits.take(4)? as usize + 4;
    if literal_count > 286 || distance_count > 30 {
        return None;
    }
    let mut code_lengths = [0u8; 19];
    for &position in CODE_LENGTH_ORDER.iter().take(length_count) {
        code_lengths[position] = bits.take(3)? as u8;
    }
    let lengths_code = Huffman::new(&code_lengths)?;
    let mut lengths = vec![0u8; literal_count + distance_count];
    let mut at = 0;
    while at < lengths.len() {
        let symbol = bits.symbol(&lengths_code)?;
        match symbol {
            0..=15 => {
                lengths[at] = symbol as u8;
                at += 1;
            }
            16 => {
                let previous = *lengths.get(at.checked_sub(1)?)?;
                let repeat = 3 + bits.take(2)? as usize;
                fill(&mut lengths, &mut at, repeat, previous)?;
            }
            17 => {
                let repeat = 3 + bits.take(3)? as usize;
                fill(&mut lengths, &mut at, repeat, 0)?;
            }
            _ => {
                let repeat = 11 + bits.take(7)? as usize;
                fill(&mut lengths, &mut at, repeat, 0)?;
            }
        }
    }
    // The end of block code must exist.
    if lengths[256] == 0 {
        return None;
    }
    let literals = Huffman::new(&lengths[..literal_count])?;
    let distances = Huffman::new(&lengths[literal_count..])?;
    Some((literals, distances))
}

fn fill(lengths: &mut [u8], at: &mut usize, repeat: usize, value: u8) -> Option<()> {
    if *at + repeat > lengths.len() {
        return None;
    }
    lengths[*at..*at + repeat].fill(value);
    *at += repeat;
    Some(())
}

fn codes_block(
    bits: &mut Bits<'_>,
    out: &mut Vec<u8>,
    limit: usize,
    literals: &Huffman,
    distances: &Huffman,
) -> InflateStatus {
    use InflateStatus::*;
    loop {
        let Some(symbol) = bits.symbol(literals) else {
            return Damaged;
        };
        if symbol < 256 {
            if out.len() >= limit {
                return LimitReached;
            }
            out.push(symbol as u8);
            continue;
        }
        if symbol == 256 {
            return Done;
        }
        let index = usize::from(symbol) - 257;
        if index >= 29 {
            return Damaged;
        }
        let Some(extra) = bits.take(u32::from(LENGTH_EXTRA[index])) else {
            return Damaged;
        };
        let length = usize::from(LENGTH_BASE[index]) + extra as usize;
        let Some(code) = bits.symbol(distances) else {
            return Damaged;
        };
        let code = usize::from(code);
        if code >= 30 {
            return Damaged;
        }
        let Some(extra) = bits.take(u32::from(DISTANCE_EXTRA[code])) else {
            return Damaged;
        };
        let distance = usize::from(DISTANCE_BASE[code]) + extra as usize;
        if distance > out.len() {
            return Damaged;
        }
        let start = out.len() - distance;
        if out.len() + length > limit {
            // As much of the match as fits.
            for i in 0..limit - out.len() {
                let byte = out[start + i];
                out.push(byte);
            }
            return LimitReached;
        }
        if distance >= length {
            out.extend_from_within(start..start + length);
        } else {
            for i in 0..length {
                let byte = out[start + i];
                out.push(byte);
            }
        }
    }
}
