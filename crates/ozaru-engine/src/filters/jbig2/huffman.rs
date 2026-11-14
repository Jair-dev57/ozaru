use std::collections::HashMap;

use super::invalid;
use crate::base::Result;

/// Reads bits, the most significant first, and bytes (ITU-T T.88, section
/// 6.2.2 and Annex B). Past the end it reads 0 bits, and says so in
/// `exhausted`.
pub(super) struct Bits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    pub fn bit(&mut self) -> Result<u32> {
        let byte = *self.data.get(self.bit / 8).ok_or_else(invalid)?;
        let value = u32::from(byte >> (7 - self.bit % 8) & 1);
        self.bit += 1;
        Ok(value)
    }

    pub fn bits(&mut self, count: u32) -> Result<u32> {
        let mut value: u64 = 0;
        for _ in 0..count {
            value = value << 1 | u64::from(self.bit()?);
        }
        Ok(value as u32)
    }

    /// Skips to the next byte.
    pub fn align(&mut self) {
        self.bit = self.bit.div_ceil(8) * 8;
    }

    /// The bytes from the current one, which must be on a byte.
    pub fn rest(&self) -> &'a [u8] {
        self.data.get(self.bit / 8..).unwrap_or(&[])
    }

    /// Skips `count` bytes, from a byte.
    pub fn skip(&mut self, count: usize) {
        self.bit = self.bit.saturating_add(count.saturating_mul(8));
    }
}

/// One line of a Huffman table: its prefix length, how many bits follow it,
/// and the lowest value it gives (section B.2).
#[derive(Debug, Clone, Copy)]
struct Line {
    prefix: u32,
    range: u32,
    low: i64,
    kind: LineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Normal,
    /// The lower range line: values below `low`, counting down.
    Lower,
    /// The upper range line: values from `low` up.
    Upper,
    /// The line of OOB.
    Oob,
}

/// A Huffman table, with the codes of its lines assigned (section B.3).
#[derive(Debug, Clone)]
pub(super) struct Table {
    // The line of each code, by its length and value. Lines with a prefix
    // length of 0 have no code and are never used.
    codes: HashMap<(u32, u32), Line>,
}

impl Table {
    // `normal` lines of (prefix, range, low), then the lower and upper range
    // lines, (prefix, low), and the prefix of OOB.
    fn standard(
        normal: &[(u32, u32, i64)],
        lower: Option<(u32, i64)>,
        upper: (u32, i64),
        oob: Option<u32>,
    ) -> Self {
        let mut lines: Vec<Line> = normal
            .iter()
            .map(|&(prefix, range, low)| Line {
                prefix,
                range,
                low,
                kind: LineKind::Normal,
            })
            .collect();
        if let Some((prefix, low)) = lower {
            lines.push(Line {
                prefix,
                range: 32,
                low,
                kind: LineKind::Lower,
            });
        }
        lines.push(Line {
            prefix: upper.0,
            range: 32,
            low: upper.1,
            kind: LineKind::Upper,
        });
        if let Some(prefix) = oob {
            lines.push(Line {
                prefix,
                range: 0,
                low: 0,
                kind: LineKind::Oob,
            });
        }
        Self::assign(lines)
    }

    // Assigns the codes of the lines from their prefix lengths, as the
    // canonical procedure of section B.3 does.
    fn assign(lines: Vec<Line>) -> Self {
        let longest = lines.iter().map(|line| line.prefix).max().unwrap_or(0);
        let mut counts = vec![0u32; longest as usize + 1];
        for line in &lines {
            counts[line.prefix as usize] += 1;
        }
        counts[0] = 0;
        let mut first = vec![0u32; longest as usize + 1];
        for length in 1..=longest as usize {
            first[length] = (first[length - 1] + counts[length - 1]) << 1;
        }
        let mut next = first;
        let mut codes = HashMap::with_capacity(lines.len());
        for line in lines {
            let length = line.prefix as usize;
            if length > 0 {
                codes.insert((line.prefix, next[length]), line);
                next[length] += 1;
            }
        }
        Self { codes }
    }

    /// The value of the next code of `bits`, or `None` for OOB.
    pub fn decode(&self, bits: &mut Bits<'_>) -> Result<Option<i64>> {
        let mut code = 0u32;
        let mut length = 0u32;
        loop {
            code = code << 1 | bits.bit()?;
            length += 1;
            if length > 32 {
                return Err(invalid());
            }
            let Some(line) = self.codes.get(&(length, code)) else {
                continue;
            };
            let offset = i64::from(bits.bits(line.range)?);
            return Ok(match line.kind {
                LineKind::Oob => None,
                LineKind::Lower => Some(line.low - offset),
                LineKind::Normal | LineKind::Upper => Some(line.low + offset),
            });
        }
    }

    /// A table of symbol numbers, from the code length of each symbol
    /// (section 7.4.3.1.7): `lengths[i]` bits for symbol `i`, 0 for none.
    pub fn of_lengths(lengths: &[u32]) -> Self {
        let lines = lengths
            .iter()
            .enumerate()
            .map(|(symbol, &prefix)| Line {
                prefix,
                range: 0,
                low: symbol as i64,
                kind: LineKind::Normal,
            })
            .collect();
        Self::assign(lines)
    }

    /// A table of a table segment (section 7.4.13).
    pub fn read_custom(data: &[u8]) -> Result<Self> {
        let flags = *data.first().ok_or_else(invalid)?;
        let oob = flags & 1 != 0;
        let prefix_bits = u32::from((flags >> 1) & 7) + 1;
        let range_bits = u32::from((flags >> 4) & 7) + 1;
        let number = |at: usize| -> Result<i64> {
            let bytes = data.get(at..at + 4).ok_or_else(invalid)?;
            Ok(i64::from(i32::from_be_bytes(
                bytes.try_into().map_err(|_| invalid())?,
            )))
        };
        let low = number(1)?;
        let high = number(5)?;
        let mut bits = Bits::new(data.get(9..).unwrap_or(&[]));
        let mut lines = Vec::new();
        let mut current = low;
        while current < high {
            if lines.len() > 1 << 16 {
                return Err(invalid());
            }
            let prefix = bits.bits(prefix_bits)?;
            let range = bits.bits(range_bits)?;
            if range > 32 {
                return Err(invalid());
            }
            lines.push(Line {
                prefix,
                range,
                low: current,
                kind: LineKind::Normal,
            });
            current += 1i64 << range;
        }
        let prefix = bits.bits(prefix_bits)?;
        lines.push(Line {
            prefix,
            range: 32,
            low: low - 1,
            kind: LineKind::Lower,
        });
        let prefix = bits.bits(prefix_bits)?;
        lines.push(Line {
            prefix,
            range: 32,
            low: high,
            kind: LineKind::Upper,
        });
        if oob {
            let prefix = bits.bits(prefix_bits)?;
            lines.push(Line {
                prefix,
                range: 0,
                low: 0,
                kind: LineKind::Oob,
            });
        }
        Ok(Self::assign(lines))
    }
}

/// The standard table B.`number` (Annex B.5), 1 to 15.
pub(super) fn standard(number: u8) -> Table {
    match number {
        1 => Table::standard(
            &[(1, 4, 0), (2, 8, 16), (3, 16, 272)],
            None,
            (3, 65808),
            None,
        ),
        2 => Table::standard(
            &[(1, 0, 0), (2, 0, 1), (3, 0, 2), (4, 3, 3), (5, 6, 11)],
            None,
            (6, 75),
            Some(6),
        ),
        3 => Table::standard(
            &[
                (8, 8, -256),
                (1, 0, 0),
                (2, 0, 1),
                (3, 0, 2),
                (4, 3, 3),
                (5, 6, 11),
            ],
            Some((8, -257)),
            (7, 75),
            Some(6),
        ),
        4 => Table::standard(
            &[(1, 0, 1), (2, 0, 2), (3, 0, 3), (4, 3, 4), (5, 6, 12)],
            None,
            (5, 76),
            None,
        ),
        5 => Table::standard(
            &[
                (7, 8, -255),
                (1, 0, 1),
                (2, 0, 2),
                (3, 0, 3),
                (4, 3, 4),
                (5, 6, 12),
            ],
            Some((7, -256)),
            (6, 76),
            None,
        ),
        6 => Table::standard(
            &[
                (5, 10, -2048),
                (4, 9, -1024),
                (4, 8, -512),
                (4, 7, -256),
                (5, 6, -128),
                (5, 5, -64),
                (4, 5, -32),
                (2, 7, 0),
                (3, 7, 128),
                (3, 8, 256),
                (4, 9, 512),
                (4, 10, 1024),
            ],
            Some((6, -2049)),
            (6, 2048),
            None,
        ),
        7 => Table::standard(
            &[
                (4, 9, -1024),
                (3, 8, -512),
                (4, 7, -256),
                (5, 6, -128),
                (5, 5, -64),
                (4, 5, -32),
                (4, 5, 0),
                (5, 5, 32),
                (5, 6, 64),
                (4, 7, 128),
                (3, 8, 256),
                (3, 9, 512),
                (3, 10, 1024),
            ],
            Some((5, -1025)),
            (5, 2048),
            None,
        ),
        8 => Table::standard(
            &[
                (8, 3, -15),
                (9, 1, -7),
                (8, 1, -5),
                (9, 0, -3),
                (7, 0, -2),
                (4, 0, -1),
                (2, 1, 0),
                (5, 0, 2),
                (6, 0, 3),
                (3, 4, 4),
                (6, 1, 20),
                (4, 4, 22),
                (4, 5, 38),
                (5, 6, 70),
                (5, 7, 134),
                (6, 7, 262),
                (7, 8, 390),
                (6, 10, 646),
            ],
            Some((9, -16)),
            (9, 1670),
            Some(2),
        ),
        9 => Table::standard(
            &[
                (8, 4, -31),
                (9, 2, -15),
                (8, 2, -11),
                (9, 1, -7),
                (7, 1, -5),
                (4, 1, -3),
                (3, 1, -1),
                (3, 1, 1),
                (5, 1, 3),
                (6, 1, 5),
                (3, 5, 7),
                (6, 2, 39),
                (4, 5, 43),
                (4, 6, 75),
                (5, 7, 139),
                (5, 8, 267),
                (6, 8, 523),
                (7, 9, 779),
                (6, 11, 1291),
            ],
            Some((9, -32)),
            (9, 3339),
            Some(2),
        ),
        10 => Table::standard(
            &[
                (7, 4, -21),
                (8, 0, -5),
                (7, 0, -4),
                (5, 0, -3),
                (2, 2, -2),
                (5, 0, 2),
                (6, 0, 3),
                (7, 0, 4),
                (8, 0, 5),
                (2, 6, 6),
                (5, 5, 70),
                (6, 5, 102),
                (6, 6, 134),
                (6, 7, 198),
                (6, 8, 326),
                (6, 9, 582),
                (6, 10, 1094),
                (7, 11, 2118),
            ],
            Some((8, -22)),
            (8, 4166),
            Some(2),
        ),
        11 => Table::standard(
            &[
                (1, 0, 1),
                (2, 1, 2),
                (4, 0, 4),
                (4, 1, 5),
                (5, 1, 7),
                (5, 2, 9),
                (6, 2, 13),
                (7, 2, 17),
                (7, 3, 21),
                (7, 4, 29),
                (7, 5, 45),
                (7, 6, 77),
            ],
            None,
            (7, 141),
            None,
        ),
        12 => Table::standard(
            &[
                (1, 0, 1),
                (2, 0, 2),
                (3, 1, 3),
                (5, 0, 5),
                (5, 1, 6),
                (6, 1, 8),
                (7, 0, 10),
                (7, 1, 11),
                (7, 2, 13),
                (7, 3, 17),
                (7, 4, 25),
                (8, 5, 41),
            ],
            None,
            (8, 73),
            None,
        ),
        13 => Table::standard(
            &[
                (1, 0, 1),
                (3, 0, 2),
                (4, 0, 3),
                (5, 0, 4),
                (4, 1, 5),
                (3, 3, 7),
                (6, 1, 15),
                (6, 2, 17),
                (6, 3, 21),
                (6, 4, 29),
                (6, 5, 45),
                (7, 6, 77),
            ],
            None,
            (7, 141),
            None,
        ),
        // B.14 has no range lines: its upper one, of prefix length 0, is
        // never used.
        14 => Table::standard(
            &[(3, 0, -2), (3, 0, -1), (1, 0, 0), (3, 0, 1), (3, 0, 2)],
            None,
            (0, 0),
            None,
        ),
        _ => Table::standard(
            &[
                (7, 4, -24),
                (6, 2, -8),
                (5, 1, -4),
                (4, 0, -2),
                (3, 0, -1),
                (1, 0, 0),
                (3, 0, 1),
                (4, 0, 2),
                (5, 1, 3),
                (6, 2, 5),
                (7, 4, 9),
            ],
            Some((7, -25)),
            (7, 25),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{LineKind, standard};

    // Each standard table is a complete prefix code, and its lines cover the
    // values without gaps or overlaps: a value wrong in a table, as typed
    // from the standard, would break one or the other.
    #[test]
    fn the_standard_tables_are_complete_and_contiguous() {
        for number in 1..=15 {
            let table = standard(number);
            let kraft: f64 = table
                .codes
                .keys()
                .map(|&(length, _)| 0.5f64.powi(length as i32))
                .sum();
            assert_eq!(kraft, 1.0, "table B.{number}");

            let mut normal: Vec<_> = table
                .codes
                .values()
                .filter(|line| line.kind == LineKind::Normal)
                .collect();
            normal.sort_by_key(|line| line.low);
            for pair in normal.windows(2) {
                assert_eq!(
                    pair[0].low + (1i64 << pair[0].range),
                    pair[1].low,
                    "table B.{number}"
                );
            }
            let last = normal.last().unwrap();
            let end = last.low + (1i64 << last.range);
            for line in table.codes.values() {
                match line.kind {
                    LineKind::Lower => assert_eq!(line.low, normal[0].low - 1, "B.{number}"),
                    LineKind::Upper => assert_eq!(line.low, end, "B.{number}"),
                    _ => {}
                }
            }
        }
    }
}
