//! Compression: LZ77 matches found with hash chains, then Huffman codes made for
//! each block, as RFC 1951 describes. Level 0 stores, 1 is fastest and 9 the
//! smallest.

use super::adler32;
use super::inflate::{CODE_LENGTH_ORDER, DISTANCE_BASE, DISTANCE_EXTRA, LENGTH_BASE, LENGTH_EXTRA};

const WINDOW: usize = 32_768;
const WINDOW_MASK: usize = WINDOW - 1;
const HASH_BITS: u32 = 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
// A match this short and this far away costs more than the literals.
const TOO_FAR: usize = 4096;
const NONE: u32 = u32::MAX;
// The most symbols in a block.
const BLOCK_SYMBOLS: usize = 1 << 15;
const MAX_STORED: usize = 65_535;

// How hard each level looks, as zlib's levels do: matches this long are good
// enough to look less (`good`), long enough to not try lazily (`lazy`), long
// enough to stop (`nice`), and how many candidates to see (`chain`).
struct Effort {
    good: usize,
    lazy: usize,
    nice: usize,
    chain: usize,
    lazy_matching: bool,
}

fn effort(level: u8) -> Effort {
    let (good, lazy, nice, chain, lazy_matching) = match level {
        1 => (4, 4, 6, 1, false),
        2 => (4, 5, 16, 8, false),
        3 => (4, 6, 32, 32, false),
        4 => (4, 4, 16, 16, true),
        5 => (8, 16, 32, 32, true),
        6 => (8, 16, 128, 128, true),
        7 => (8, 32, 128, 256, true),
        8 => (32, 128, 258, 1024, true),
        _ => (32, 258, 258, 4096, true),
    };
    Effort {
        good,
        lazy,
        nice,
        chain,
        lazy_matching,
    }
}

/// Compresses `data` as raw Deflate, at `level` from 0 (stored) to 9; higher
/// levels are the same as 9.
pub fn deflate(data: &[u8], level: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 64);
    compress(data, level, &mut out);
    out
}

/// Compresses `data` with the zlib wrapper, as PDF's Flate filter wants.
pub fn deflate_zlib(data: &[u8], level: u8) -> Vec<u8> {
    let flags = match level {
        0 | 1 => 0x01,
        2..=5 => 0x5E,
        6 => 0x9C,
        _ => 0xDA,
    };
    let mut out = vec![0x78, flags];
    compress(data, level, &mut out);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    buffer: u64,
    count: u32,
}

impl BitWriter<'_> {
    #[inline]
    fn write(&mut self, value: u32, bits: u32) {
        self.buffer |= u64::from(value) << self.count;
        self.count += bits;
        if self.count >= 32 {
            self.out
                .extend_from_slice(&(self.buffer as u32).to_le_bytes());
            self.buffer >>= 32;
            self.count -= 32;
        }
    }

    fn align(&mut self) {
        while self.count > 0 {
            self.out.push(self.buffer as u8);
            self.buffer >>= 8;
            self.count = self.count.saturating_sub(8);
        }
        self.buffer = 0;
    }
}

// A literal, or a match: `distance << 16 | length`, with distance 0 for a literal
// whose byte is the length.
type Token = u32;

fn literal(byte: u8) -> Token {
    u32::from(byte)
}

fn matched(length: usize, distance: usize) -> Token {
    (distance as u32) << 16 | length as u32
}

fn compress(data: &[u8], level: u8, out: &mut Vec<u8>) {
    let mut writer = BitWriter {
        out,
        buffer: 0,
        count: 0,
    };
    if level == 0 || data.len() >= NONE as usize {
        stored_blocks(&mut writer, data, true);
        writer.align();
        return;
    }
    let effort = effort(level);
    let mut matcher = Matcher::new(data, effort.chain > 1);
    let mut tokens: Vec<Token> = Vec::with_capacity(BLOCK_SYMBOLS);
    // Where the bytes of `tokens` start.
    let mut block_start = 0;
    let end = data.len();
    let mut position = 0;

    if effort.lazy_matching {
        // As zlib's `deflate_slow`: a match is only taken once the next position
        // has been seen not to have a longer one.
        let mut length = MIN_MATCH - 1;
        let mut distance = 0;
        let mut pending = false;
        while position < end {
            let previous = (length, distance);
            length = MIN_MATCH - 1;
            let candidate = matcher.insert(position);
            if candidate != NONE && previous.0 < effort.lazy {
                let chain = if previous.0 >= effort.good {
                    effort.chain / 4
                } else {
                    effort.chain
                };
                let found = matcher.longest(position, candidate, chain, effort.nice, previous.0);
                if found.0 > previous.0 && !(found.0 == MIN_MATCH && found.1 > TOO_FAR) {
                    (length, distance) = found;
                }
            }
            if previous.0 >= MIN_MATCH && length <= previous.0 {
                tokens.push(matched(previous.0, previous.1));
                // The bytes of the match, but the two positions already inserted.
                for skip in position + 1..position + previous.0 - 1 {
                    matcher.insert(skip);
                }
                position += previous.0 - 1;
                pending = false;
                length = MIN_MATCH - 1;
            } else if pending {
                tokens.push(literal(data[position - 1]));
                position += 1;
            } else {
                pending = true;
                position += 1;
            }
            if tokens.len() >= BLOCK_SYMBOLS {
                flush(&mut writer, data, &tokens, block_start, false);
                block_start += covered(&tokens);
                tokens.clear();
            }
        }
        if pending {
            tokens.push(literal(data[end - 1]));
        }
    } else {
        // Where nothing matches (noise, photographs), look less and less often.
        let min_match = if effort.chain > 1 {
            MIN_MATCH
        } else {
            MIN_MATCH + 1
        };
        let mut misses = 0usize;
        while position < end {
            let candidate = matcher.insert(position);
            let mut found = (0, 0);
            if candidate != NONE {
                found = matcher.longest(
                    position,
                    candidate,
                    effort.chain,
                    effort.nice,
                    MIN_MATCH - 1,
                );
                if found.0 < min_match || found.0 == MIN_MATCH && found.1 > TOO_FAR {
                    found = (0, 0);
                }
            }
            if found.0 >= min_match {
                // Short matches are chance in noise: they only help a little.
                misses = if found.0 >= 8 { 0 } else { misses / 2 };
                tokens.push(matched(found.0, found.1));
                // Long matches don't insert their positions, which saves time.
                if found.0 <= effort.lazy {
                    for skip in position + 1..position + found.0 {
                        matcher.insert(skip);
                    }
                }
                position += found.0;
            } else {
                misses += 1;
                // The bytes skipped are literals without being looked at.
                let skipped = (misses >> 3).min(31).min(end - position - 1);
                tokens.extend(
                    data[position..=position + skipped]
                        .iter()
                        .map(|&byte| literal(byte)),
                );
                position += 1 + skipped;
            }
            if tokens.len() >= BLOCK_SYMBOLS {
                flush(&mut writer, data, &tokens, block_start, false);
                block_start += covered(&tokens);
                tokens.clear();
            }
        }
    }
    flush(&mut writer, data, &tokens, block_start, true);
    writer.align();
}

// How many bytes of the input `tokens` stand for.
fn covered(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .map(|&token| {
            if token >> 16 == 0 {
                1
            } else {
                (token & 0xFFFF) as usize
            }
        })
        .sum()
}

fn stored_blocks(writer: &mut BitWriter<'_>, data: &[u8], last: bool) {
    if data.is_empty() {
        write_stored(writer, data, last);
        return;
    }
    let mut chunks = data.chunks(MAX_STORED).peekable();
    while let Some(chunk) = chunks.next() {
        write_stored(writer, chunk, last && chunks.peek().is_none());
    }
}

fn write_stored(writer: &mut BitWriter<'_>, chunk: &[u8], last: bool) {
    writer.write(u32::from(last), 1);
    writer.write(0, 2);
    writer.align();
    let length = chunk.len() as u32;
    writer.write(length, 16);
    writer.write(!length & 0xFFFF, 16);
    writer.out.extend_from_slice(chunk);
}

// Finds earlier occurrences of the bytes at a position, with hash chains of the
// positions of each three bytes.
struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
    // Whether older candidates are kept: the fastest level only looks at one.
    chains: bool,
}

impl<'a> Matcher<'a> {
    fn new(data: &'a [u8], chains: bool) -> Self {
        Self {
            data,
            head: vec![NONE; 1 << HASH_BITS],
            prev: vec![NONE; if chains { WINDOW } else { 0 }],
            chains,
        }
    }

    fn hash(&self, position: usize) -> usize {
        let d = self.data;
        let mut value = u32::from(d[position])
            | u32::from(d[position + 1]) << 8
            | u32::from(d[position + 2]) << 16;
        if !self.chains {
            // The fastest level matches four bytes at least, which finds fewer
            // matches by chance in noise.
            value |= u32::from(d[position + 3]) << 24;
        }
        (value.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    }

    // Adds `position` to its chain, and returns the one before it, or `NONE`.
    fn insert(&mut self, position: usize) -> u32 {
        if position
            + if self.chains {
                MIN_MATCH
            } else {
                MIN_MATCH + 1
            }
            > self.data.len()
        {
            return NONE;
        }
        let slot = self.hash(position);
        let candidate = self.head[slot];
        if self.chains {
            self.prev[position & WINDOW_MASK] = candidate;
        }
        self.head[slot] = position as u32;
        candidate
    }

    // The longest match at `position` among the chain from `candidate`, as
    // `(length, distance)`; `(0, 0)` unless longer than `beaten`.
    fn longest(
        &self,
        position: usize,
        mut candidate: u32,
        mut chain: usize,
        nice: usize,
        beaten: usize,
    ) -> (usize, usize) {
        let data = self.data;
        let limit = MAX_MATCH.min(data.len() - position);
        let mut best = (0, 0);
        let mut best_length = beaten.max(MIN_MATCH - 1);
        while candidate != NONE && chain > 0 {
            let at = candidate as usize;
            let distance = position - at;
            if distance > WINDOW {
                break;
            }
            // A candidate has to beat the best so far at the byte after it.
            if best_length < limit && data[at + best_length] == data[position + best_length] {
                let length = common_prefix(&data[at..], &data[position..], limit);
                if length > best_length {
                    best_length = length;
                    best = (length, distance);
                    if length >= nice || length == limit {
                        break;
                    }
                }
            }
            if !self.chains {
                break;
            }
            let next = self.prev[at & WINDOW_MASK];
            if next != NONE && next as usize >= at {
                break;
            }
            candidate = next;
            chain -= 1;
        }
        best
    }
}

// How many bytes the two slices have in common, up to `limit`.
#[inline]
fn common_prefix(a: &[u8], b: &[u8], limit: usize) -> usize {
    let mut length = 0;
    while length + 8 <= limit {
        let x = u64::from_le_bytes(a[length..length + 8].try_into().expect("8 bytes"));
        let y = u64::from_le_bytes(b[length..length + 8].try_into().expect("8 bytes"));
        let difference = x ^ y;
        if difference != 0 {
            return length + (difference.trailing_zeros() / 8) as usize;
        }
        length += 8;
    }
    while length < limit && a[length] == b[length] {
        length += 1;
    }
    length
}

// The code of each length (3 to 258): an index into `LENGTH_BASE`.
fn length_code(length: usize) -> usize {
    static TABLE: std::sync::OnceLock<[u8; 259]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u8; 259];
        for (length, slot) in table.iter_mut().enumerate().skip(3) {
            let mut code = 27;
            while usize::from(LENGTH_BASE[code]) > length {
                code -= 1;
            }
            // 258 has its own code, the last.
            *slot = if length == 258 { 28 } else { code as u8 };
        }
        table
    });
    usize::from(table[length])
}

fn distance_code(distance: usize) -> usize {
    if distance <= 4 {
        return distance - 1;
    }
    let d = distance - 1;
    let bits = usize::BITS - 1 - d.leading_zeros();
    (2 * bits as usize) + ((d >> (bits - 1)) & 1)
}

// Symbols: 0 to 255 literals, 256 the end of the block, 257 to 285 lengths.
struct Frequencies {
    literals: [u32; 286],
    distances: [u32; 30],
    // Bits of the extra values of lengths and distances.
    extra_bits: u64,
}

fn count(tokens: &[Token]) -> Frequencies {
    let mut f = Frequencies {
        literals: [0; 286],
        distances: [0; 30],
        extra_bits: 0,
    };
    for &token in tokens {
        let distance = (token >> 16) as usize;
        let length = (token & 0xFFFF) as usize;
        if distance == 0 {
            f.literals[length] += 1;
        } else {
            let lc = length_code(length);
            let dc = distance_code(distance);
            f.literals[257 + lc] += 1;
            f.distances[dc] += 1;
            f.extra_bits += u64::from(LENGTH_EXTRA[lc]) + u64::from(DISTANCE_EXTRA[dc]);
        }
    }
    f.literals[256] = 1;
    f
}

// The lengths of the codes for `frequencies`, none over `max`.
#[allow(clippy::needless_range_loop)]
fn code_lengths(frequencies: &[u32], max: usize) -> Vec<u8> {
    let mut lengths = vec![0u8; frequencies.len()];
    let mut symbols: Vec<usize> = (0..frequencies.len())
        .filter(|&s| frequencies[s] > 0)
        .collect();
    match symbols.len() {
        0 => return lengths,
        1 => {
            lengths[symbols[0]] = 1;
            return lengths;
        }
        _ => {}
    }
    symbols.sort_by_key(|&s| (frequencies[s], s));

    // The Huffman tree, with two queues: the leaves in order, and the nodes made,
    // which come out in order too. `depth_parent` links each node to its parent.
    let n = symbols.len();
    let mut weight: Vec<u64> = symbols.iter().map(|&s| u64::from(frequencies[s])).collect();
    let mut parent: Vec<usize> = vec![0; 2 * n - 1];
    weight.resize(2 * n - 1, 0);
    let (mut leaf, mut node) = (0, n);
    for made in n..2 * n - 1 {
        let mut pick = || {
            if leaf < n && (node >= made || weight[leaf] <= weight[node]) {
                leaf += 1;
                leaf - 1
            } else {
                node += 1;
                node - 1
            }
        };
        let (a, b) = (pick(), pick());
        weight[made] = weight[a] + weight[b];
        parent[a] = made;
        parent[b] = made;
    }
    // Depths, from the root (the last node) down.
    let mut depth = vec![0usize; 2 * n - 1];
    for index in (0..2 * n - 2).rev() {
        depth[index] = depth[parent[index]] + 1;
    }

    // How many codes there are of each length, none longer than `max`.
    let mut counts = vec![0u32; max + 1];
    for &d in &depth[..n] {
        counts[d.min(max)] += 1;
    }
    // Shortening the long codes may over-subscribe the code space; lengthen
    // others until it fits.
    let mut total: u64 = (1..=max).map(|l| u64::from(counts[l]) << (max - l)).sum();
    while total > 1 << max {
        counts[max] -= 1;
        for length in (1..max).rev() {
            if counts[length] != 0 {
                counts[length] -= 1;
                counts[length + 1] += 2;
                break;
            }
        }
        total -= 1;
    }
    // The most frequent symbols get the shortest codes: the symbols are ordered
    // by frequency, rarest first.
    let mut at = n;
    for length in 1..=max {
        for _ in 0..counts[length] {
            at -= 1;
            lengths[symbols[at]] = length as u8;
        }
    }
    lengths
}

// The codes for `lengths`, already reversed for writing least significant bit
// first.
fn codes(lengths: &[u8]) -> Vec<u16> {
    let mut count = [0u32; 16];
    for &l in lengths {
        count[usize::from(l)] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let value = next[usize::from(l)];
            next[usize::from(l)] += 1;
            (value.reverse_bits() >> (32 - u32::from(l))) as u16
        })
        .collect()
}

// The code lengths of a dynamic block, as run-length coded symbols: each is a
// symbol of the code length code and its extra bits.
fn run_lengths(lengths: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lengths.len() {
        let value = lengths[i];
        let mut run = 1;
        while i + run < lengths.len() && lengths[i + run] == value {
            run += 1;
        }
        i += run;
        if value == 0 {
            while run >= 11 {
                let take = run.min(138);
                out.push((18, (take - 11) as u8));
                run -= take;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u8));
                run = 0;
            }
        } else {
            out.push((value, 0));
            run -= 1;
            while run >= 3 {
                let take = run.min(6);
                out.push((16, (take - 3) as u8));
                run -= take;
            }
        }
        for _ in 0..run {
            out.push((value, 0));
        }
    }
    out
}

fn flush(writer: &mut BitWriter<'_>, data: &[u8], tokens: &[Token], start: usize, last: bool) {
    let end = start + covered(tokens);
    let f = count(tokens);
    let literal_lengths = code_lengths(&f.literals, 15);
    let mut distance_lengths = code_lengths(&f.distances, 15);
    if distance_lengths.iter().all(|&l| l == 0) {
        // No matches: one distance code that is never used.
        distance_lengths[0] = 1;
    }
    let literal_count = literal_lengths
        .iter()
        .rposition(|&l| l != 0)
        .map_or(257, |p| p + 1)
        .max(257);
    let distance_count = distance_lengths
        .iter()
        .rposition(|&l| l != 0)
        .map_or(1, |p| p + 1);
    let mut all = literal_lengths[..literal_count].to_vec();
    all.extend_from_slice(&distance_lengths[..distance_count]);
    let runs = run_lengths(&all);
    let mut run_freq = [0u32; 19];
    for &(symbol, _) in &runs {
        run_freq[usize::from(symbol)] += 1;
    }
    let run_lengths_code = code_lengths(&run_freq, 7);
    let header_count = CODE_LENGTH_ORDER
        .iter()
        .rposition(|&s| run_lengths_code[s] != 0)
        .map_or(4, |p| p + 1)
        .max(4);

    // Sizes in bits of the three ways to write the block.
    let symbol_bits = |lengths: &[u8], freq: &[u32]| -> u64 {
        freq.iter()
            .zip(lengths)
            .map(|(&f, &l)| u64::from(f) * u64::from(l))
            .sum()
    };
    let dynamic = 3
        + 14
        + 3 * header_count as u64
        + runs
            .iter()
            .map(|&(symbol, _)| {
                u64::from(run_lengths_code[usize::from(symbol)])
                    + match symbol {
                        16 => 2,
                        17 => 3,
                        18 => 7,
                        _ => 0,
                    }
            })
            .sum::<u64>()
        + symbol_bits(&literal_lengths, &f.literals)
        + symbol_bits(&distance_lengths, &f.distances)
        + f.extra_bits;
    let mut fixed_literal = [8u8; 288];
    fixed_literal[144..256].fill(9);
    fixed_literal[256..280].fill(7);
    let fixed = 3
        + f.literals
            .iter()
            .enumerate()
            .map(|(s, &n)| u64::from(n) * u64::from(fixed_literal[s]))
            .sum::<u64>()
        + 5 * f.distances.iter().map(|&n| u64::from(n)).sum::<u64>()
        + f.extra_bits;
    let stored = 8 * ((end - start) as u64 + 5 * ((end - start) / MAX_STORED + 1) as u64);

    if stored < dynamic.min(fixed) {
        stored_blocks(writer, &data[start..end], last);
        return;
    }
    if fixed <= dynamic {
        writer.write(u32::from(last), 1);
        writer.write(1, 2);
        let literal_codes = codes(&fixed_literal);
        let distance_lengths = [5u8; 30];
        let distance_codes = codes(&distance_lengths);
        write_tokens(
            writer,
            tokens,
            (&fixed_literal, &literal_codes),
            (&distance_lengths, &distance_codes),
        );
        return;
    }

    writer.write(u32::from(last), 1);
    writer.write(2, 2);
    writer.write((literal_count - 257) as u32, 5);
    writer.write((distance_count - 1) as u32, 5);
    writer.write((header_count - 4) as u32, 4);
    for &position in CODE_LENGTH_ORDER.iter().take(header_count) {
        writer.write(u32::from(run_lengths_code[position]), 3);
    }
    let run_codes = codes(&run_lengths_code);
    for &(symbol, extra) in &runs {
        let s = usize::from(symbol);
        writer.write(u32::from(run_codes[s]), u32::from(run_lengths_code[s]));
        match symbol {
            16 => writer.write(u32::from(extra), 2),
            17 => writer.write(u32::from(extra), 3),
            18 => writer.write(u32::from(extra), 7),
            _ => {}
        }
    }
    let literal_codes = codes(&literal_lengths);
    let distance_codes = codes(&distance_lengths);
    write_tokens(
        writer,
        tokens,
        (&literal_lengths, &literal_codes),
        (&distance_lengths, &distance_codes),
    );
}

fn write_tokens(
    writer: &mut BitWriter<'_>,
    tokens: &[Token],
    (literal_lengths, literal_codes): (&[u8], &[u16]),
    (distance_lengths, distance_codes): (&[u8], &[u16]),
) {
    for &token in tokens {
        let distance = (token >> 16) as usize;
        let length = (token & 0xFFFF) as usize;
        if distance == 0 {
            writer.write(
                u32::from(literal_codes[length]),
                u32::from(literal_lengths[length]),
            );
            continue;
        }
        let lc = length_code(length);
        let symbol = 257 + lc;
        writer.write(
            u32::from(literal_codes[symbol]),
            u32::from(literal_lengths[symbol]),
        );
        writer.write(
            (length - usize::from(LENGTH_BASE[lc])) as u32,
            u32::from(LENGTH_EXTRA[lc]),
        );
        let dc = distance_code(distance);
        writer.write(
            u32::from(distance_codes[dc]),
            u32::from(distance_lengths[dc]),
        );
        writer.write(
            (distance - usize::from(DISTANCE_BASE[dc])) as u32,
            u32::from(DISTANCE_EXTRA[dc]),
        );
    }
    writer.write(
        u32::from(literal_codes[256]),
        u32::from(literal_lengths[256]),
    );
}
