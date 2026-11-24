//! Deflate (RFC 1951) and the zlib wrapper around it (RFC 1950): the compression
//! of Flate streams, written for the engine.

mod compress;
mod inflate;

pub use compress::{deflate, deflate_zlib};
pub use inflate::{InflateStatus, inflate};

/// The Adler-32 checksum of `data` (RFC 1950, section 8.2).
pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    // The most bytes that can be added before the sums have to be reduced.
    const CHUNK: usize = 5_552;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(CHUNK) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}
