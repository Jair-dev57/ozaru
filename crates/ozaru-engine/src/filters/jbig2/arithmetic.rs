/// The probability estimation table of the MQ coder (ITU-T T.88, Table E.1):
/// for each state, its Qe, the next state after an MPS and after an LPS, and
/// whether an LPS switches the sense of the MPS.
const STATES: [(u32, u8, u8, bool); 47] = [
    (0x5601, 1, 1, true),
    (0x3401, 2, 6, false),
    (0x1801, 3, 9, false),
    (0x0AC1, 4, 12, false),
    (0x0521, 5, 29, false),
    (0x0221, 38, 33, false),
    (0x5601, 7, 6, true),
    (0x5401, 8, 14, false),
    (0x4801, 9, 14, false),
    (0x3801, 10, 14, false),
    (0x3001, 11, 17, false),
    (0x2401, 12, 18, false),
    (0x1C01, 13, 20, false),
    (0x1601, 29, 21, false),
    (0x5601, 15, 14, true),
    (0x5401, 16, 14, false),
    (0x5101, 17, 15, false),
    (0x4801, 18, 16, false),
    (0x3801, 19, 17, false),
    (0x3401, 20, 18, false),
    (0x3001, 21, 19, false),
    (0x2801, 22, 19, false),
    (0x2401, 23, 20, false),
    (0x2201, 24, 21, false),
    (0x1C01, 25, 22, false),
    (0x1801, 26, 23, false),
    (0x1601, 27, 24, false),
    (0x1401, 28, 25, false),
    (0x1201, 29, 26, false),
    (0x1101, 30, 27, false),
    (0x0AC1, 31, 28, false),
    (0x09C1, 32, 29, false),
    (0x08A1, 33, 30, false),
    (0x0521, 34, 31, false),
    (0x0441, 35, 32, false),
    (0x02A1, 36, 33, false),
    (0x0221, 37, 34, false),
    (0x0141, 38, 35, false),
    (0x0111, 39, 36, false),
    (0x0085, 40, 37, false),
    (0x0049, 41, 38, false),
    (0x0025, 42, 39, false),
    (0x0015, 43, 40, false),
    (0x0009, 44, 41, false),
    (0x0005, 45, 42, false),
    (0x0001, 45, 43, false),
    (0x5601, 46, 46, false),
];

/// The adaptive contexts of a decoding procedure: for each, its state in the
/// table and the value of its more probable symbol.
#[derive(Debug, Clone)]
pub(super) struct Contexts(Vec<u8>);

impl Contexts {
    pub fn new(count: usize) -> Self {
        Self(vec![0; count])
    }
}

/// The MQ arithmetic decoder (ITU-T T.88, Annex E.3), reading from `data`.
/// Past its end, it reads 0xFF bytes, as the standard says.
pub(super) struct Decoder<'a> {
    data: &'a [u8],
    position: usize,
    c: u32,
    a: u32,
    ct: u32,
    /// How many bits have been decoded, to bound the work on damaged data.
    pub decoded: u64,
}

impl<'a> Decoder<'a> {
    /// INITDEC.
    pub fn new(data: &'a [u8]) -> Self {
        let mut decoder = Self {
            data,
            position: 0,
            c: 0,
            a: 0,
            ct: 0,
            decoded: 0,
        };
        decoder.c = u32::from(decoder.byte(0)) << 16;
        decoder.byte_in();
        decoder.c <<= 7;
        decoder.ct = decoder.ct.saturating_sub(7);
        decoder.a = 0x8000;
        decoder
    }

    fn byte(&self, at: usize) -> u8 {
        self.data.get(at).copied().unwrap_or(0xFF)
    }

    /// BYTEIN: the next byte into C, unless it is a marker after 0xFF.
    fn byte_in(&mut self) {
        if self.byte(self.position) == 0xFF {
            if self.byte(self.position + 1) > 0x8F {
                self.c = self.c.wrapping_add(0xFF00);
                self.ct = 8;
            } else {
                self.position += 1;
                self.c = self
                    .c
                    .wrapping_add(u32::from(self.byte(self.position)) << 9);
                self.ct = 7;
            }
        } else {
            self.position += 1;
            self.c = self
                .c
                .wrapping_add(u32::from(self.byte(self.position)) << 8);
            self.ct = 8;
        }
    }

    /// DECODE: one bit, with the context `index` of `contexts`.
    pub fn decode(&mut self, contexts: &mut Contexts, index: usize) -> u32 {
        self.decoded += 1;
        let context = &mut contexts.0[index];
        let state = usize::from(*context >> 1);
        let mut mps = u32::from(*context & 1);
        let (qe, next_mps, next_lps, switch) = STATES[state];
        self.a -= qe;
        let (bit, next) = if (self.c >> 16) < qe {
            // LPS_EXCHANGE.
            if self.a < qe {
                self.a = qe;
                (mps, next_mps)
            } else {
                self.a = qe;
                let bit = 1 - mps;
                if switch {
                    mps = 1 - mps;
                }
                (bit, next_lps)
            }
        } else {
            self.c -= qe << 16;
            if self.a & 0x8000 != 0 {
                return mps;
            }
            // MPS_EXCHANGE.
            if self.a < qe {
                let bit = 1 - mps;
                if switch {
                    mps = 1 - mps;
                }
                (bit, next_lps)
            } else {
                (mps, next_mps)
            }
        };
        *context = next << 1 | mps as u8;
        // RENORMD.
        loop {
            if self.ct == 0 {
                self.byte_in();
            }
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
            if self.a & 0x8000 != 0 {
                break;
            }
        }
        bit
    }
}

/// The contexts of one integer decoding procedure, such as IADH (ITU-T T.88,
/// Annex A.2).
#[derive(Debug, Clone)]
pub(super) struct IntegerContexts(Contexts);

impl IntegerContexts {
    pub fn new() -> Self {
        Self(Contexts::new(512))
    }

    /// An integer, or `None` for OOB, the value that ends a sequence.
    pub fn decode(&mut self, decoder: &mut Decoder<'_>) -> Option<i64> {
        let mut previous = 1usize;
        let mut bit = |decoder: &mut Decoder<'_>| {
            let bit = decoder.decode(&mut self.0, previous);
            previous = if previous < 256 {
                previous << 1 | bit as usize
            } else {
                ((previous << 1 | bit as usize) & 511) | 256
            };
            bit
        };
        let sign = bit(decoder);
        let (bits, offset) = if bit(decoder) == 0 {
            (2, 0)
        } else if bit(decoder) == 0 {
            (4, 4)
        } else if bit(decoder) == 0 {
            (6, 20)
        } else if bit(decoder) == 0 {
            (8, 84)
        } else if bit(decoder) == 0 {
            (12, 340)
        } else {
            (32, 4436)
        };
        let mut value: i64 = 0;
        for _ in 0..bits {
            value = value << 1 | i64::from(bit(decoder));
        }
        value += offset;
        match (sign, value) {
            (1, 0) => None,
            (1, value) => Some(-value),
            (_, value) => Some(value),
        }
    }
}

/// The contexts of IAID, the decoding of symbol numbers of `length` bits
/// (ITU-T T.88, Annex A.3).
#[derive(Debug, Clone)]
pub(super) struct IdContexts {
    contexts: Contexts,
    length: u32,
}

impl IdContexts {
    pub fn new(length: u32) -> Self {
        Self {
            contexts: Contexts::new(1 << length.min(24)),
            length: length.min(24),
        }
    }

    pub fn decode(&mut self, decoder: &mut Decoder<'_>) -> usize {
        let mut previous = 1usize;
        for _ in 0..self.length {
            let bit = decoder.decode(&mut self.contexts, previous);
            previous = previous << 1 | bit as usize;
        }
        previous - (1 << self.length)
    }
}

#[cfg(test)]
mod tests {
    use super::{Contexts, Decoder};

    // The test sequence of the arithmetic coder in ITU-T T.88, Annex H.2:
    // the data coded with a single context, and the code it gives.
    const DATA: [u8; 32] = [
        0x00, 0x02, 0x00, 0x51, 0x00, 0x00, 0x00, 0xC0, 0x03, 0x52, 0x87, 0x2A, 0xAA, 0xAA, 0xAA,
        0xAA, 0x82, 0xC0, 0x20, 0x00, 0xFC, 0xD7, 0x9E, 0xF6, 0xBF, 0x7F, 0xED, 0x90, 0x4F, 0x46,
        0xA3, 0xBF,
    ];
    const CODE: [u8; 30] = [
        0x84, 0xC7, 0x3B, 0xFC, 0xE1, 0xA1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0D, 0xBB,
        0x86, 0xF4, 0x31, 0x7F, 0xFF, 0x88, 0xFF, 0x37, 0x47, 0x1A, 0xDB, 0x6A, 0xDF, 0xFF, 0xAC,
    ];

    #[test]
    fn the_test_sequence_of_the_standard_decodes() {
        let mut decoder = Decoder::new(&CODE);
        let mut contexts = Contexts::new(1);
        let mut decoded = Vec::new();
        for _ in 0..DATA.len() {
            let mut byte = 0u8;
            for _ in 0..8 {
                byte = byte << 1 | decoder.decode(&mut contexts, 0) as u8;
            }
            decoded.push(byte);
        }
        assert_eq!(decoded, DATA);
    }
}
