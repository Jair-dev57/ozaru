//! AES (FIPS 197) with keys of 128, 192 or 256 bits, and CBC mode (NIST SP 800-38A).
//!
//! The tables are built at compile time from the definition of the cipher: the
//! multiplicative inverse in GF(2^8) and an affine map make the S-box, and the
//! rounds are done with four lookup tables, as the standard's authors suggest.

// Multiplication in GF(2^8), modulo x^8 + x^4 + x^3 + x + 1.
const fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut product = 0;
    while b != 0 {
        if b & 1 != 0 {
            product ^= a;
        }
        a = (a << 1) ^ if a & 0x80 != 0 { 0x1B } else { 0 };
        b >>= 1;
    }
    product
}

const fn build_sboxes() -> ([u8; 256], [u8; 256]) {
    let mut sbox = [0u8; 256];
    let mut inverse = [0u8; 256];
    let mut x = 0;
    while x < 256 {
        // The inverse of x, 0 for 0.
        let mut b = 0u8;
        let mut y = 1;
        while x != 0 && y < 256 {
            if gmul(x as u8, y as u8) == 1 {
                b = y as u8;
                break;
            }
            y += 1;
        }
        let s =
            b ^ b.rotate_left(1) ^ b.rotate_left(2) ^ b.rotate_left(3) ^ b.rotate_left(4) ^ 0x63;
        sbox[x] = s;
        inverse[s as usize] = x as u8;
        x += 1;
    }
    (sbox, inverse)
}

const SBOXES: ([u8; 256], [u8; 256]) = build_sboxes();
const SBOX: [u8; 256] = SBOXES.0;
const INVERSE_SBOX: [u8; 256] = SBOXES.1;

// Tables for a round of encryption, SubBytes, ShiftRows and MixColumns at once.
const fn encryption_table(rotation: u32) -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut x = 0;
    while x < 256 {
        let s = SBOX[x];
        let word =
            (gmul(s, 2) as u32) << 24 | (s as u32) << 16 | (s as u32) << 8 | gmul(s, 3) as u32;
        table[x] = word.rotate_right(rotation);
        x += 1;
    }
    table
}

// The same for decryption, with the inverse operations.
const fn decryption_table(rotation: u32) -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut x = 0;
    while x < 256 {
        let s = INVERSE_SBOX[x];
        let word = (gmul(s, 14) as u32) << 24
            | (gmul(s, 9) as u32) << 16
            | (gmul(s, 13) as u32) << 8
            | gmul(s, 11) as u32;
        table[x] = word.rotate_right(rotation);
        x += 1;
    }
    table
}

const TE0: [u32; 256] = encryption_table(0);
const TE1: [u32; 256] = encryption_table(8);
const TE2: [u32; 256] = encryption_table(16);
const TE3: [u32; 256] = encryption_table(24);
const TD0: [u32; 256] = decryption_table(0);
const TD1: [u32; 256] = decryption_table(8);
const TD2: [u32; 256] = decryption_table(16);
const TD3: [u32; 256] = decryption_table(24);

const ROUND_CONSTANTS: [u32; 10] = [
    0x0100_0000,
    0x0200_0000,
    0x0400_0000,
    0x0800_0000,
    0x1000_0000,
    0x2000_0000,
    0x4000_0000,
    0x8000_0000,
    0x1B00_0000,
    0x3600_0000,
];

fn sub_word(word: u32) -> u32 {
    let b = word.to_be_bytes();
    u32::from_be_bytes([
        SBOX[usize::from(b[0])],
        SBOX[usize::from(b[1])],
        SBOX[usize::from(b[2])],
        SBOX[usize::from(b[3])],
    ])
}

/// An AES key, expanded to encrypt and decrypt blocks of 16 bytes.
#[derive(Clone)]
pub struct Aes {
    encrypt_keys: [u32; 60],
    decrypt_keys: [u32; 60],
    rounds: usize,
}

impl Aes {
    /// The cipher for a key of 16, 24 or 32 bytes; `None` for another length.
    pub fn new(key: &[u8]) -> Option<Self> {
        let words = match key.len() {
            16 | 24 | 32 => key.len() / 4,
            _ => return None,
        };
        let rounds = words + 6;
        let total = 4 * (rounds + 1);
        let mut encrypt_keys = [0u32; 60];
        for (word, bytes) in encrypt_keys.iter_mut().zip(key.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in words..total {
            let mut temp = encrypt_keys[i - 1];
            if i % words == 0 {
                temp = sub_word(temp.rotate_left(8)) ^ ROUND_CONSTANTS[i / words - 1];
            } else if words > 6 && i % words == 4 {
                temp = sub_word(temp);
            }
            encrypt_keys[i] = encrypt_keys[i - words] ^ temp;
        }
        // The keys for decryption are the same in reverse order, with the middle
        // ones through InvMixColumns, so decrypting is shaped like encrypting.
        let mut decrypt_keys = [0u32; 60];
        for round in 0..=rounds {
            for column in 0..4 {
                let word = encrypt_keys[4 * (rounds - round) + column];
                decrypt_keys[4 * round + column] = if round == 0 || round == rounds {
                    word
                } else {
                    let b = word.to_be_bytes();
                    TD0[usize::from(SBOX[usize::from(b[0])])]
                        ^ TD1[usize::from(SBOX[usize::from(b[1])])]
                        ^ TD2[usize::from(SBOX[usize::from(b[2])])]
                        ^ TD3[usize::from(SBOX[usize::from(b[3])])]
                };
            }
        }
        Some(Self {
            encrypt_keys,
            decrypt_keys,
            rounds,
        })
    }

    fn load(block: &[u8; 16]) -> [u32; 4] {
        let mut state = [0u32; 4];
        for (word, bytes) in state.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        state
    }

    fn store(state: [u32; 4], block: &mut [u8; 16]) {
        for (bytes, word) in block.as_chunks_mut::<4>().0.iter_mut().zip(state) {
            bytes.copy_from_slice(&word.to_be_bytes());
        }
    }

    /// Encrypts one block in place.
    pub fn encrypt_block(&self, block: &mut [u8; 16]) {
        let k = &self.encrypt_keys;
        let [mut s0, mut s1, mut s2, mut s3] = Self::load(block);
        s0 ^= k[0];
        s1 ^= k[1];
        s2 ^= k[2];
        s3 ^= k[3];
        for round in 1..self.rounds {
            let t0 = TE0[(s0 >> 24) as usize]
                ^ TE1[(s1 >> 16 & 255) as usize]
                ^ TE2[(s2 >> 8 & 255) as usize]
                ^ TE3[(s3 & 255) as usize]
                ^ k[4 * round];
            let t1 = TE0[(s1 >> 24) as usize]
                ^ TE1[(s2 >> 16 & 255) as usize]
                ^ TE2[(s3 >> 8 & 255) as usize]
                ^ TE3[(s0 & 255) as usize]
                ^ k[4 * round + 1];
            let t2 = TE0[(s2 >> 24) as usize]
                ^ TE1[(s3 >> 16 & 255) as usize]
                ^ TE2[(s0 >> 8 & 255) as usize]
                ^ TE3[(s1 & 255) as usize]
                ^ k[4 * round + 2];
            let t3 = TE0[(s3 >> 24) as usize]
                ^ TE1[(s0 >> 16 & 255) as usize]
                ^ TE2[(s1 >> 8 & 255) as usize]
                ^ TE3[(s2 & 255) as usize]
                ^ k[4 * round + 3];
            (s0, s1, s2, s3) = (t0, t1, t2, t3);
        }
        // The last round has no MixColumns.
        let last = 4 * self.rounds;
        let out = |a: u32, b: u32, c: u32, d: u32, key: u32| -> u32 {
            (u32::from(SBOX[(a >> 24) as usize]) << 24
                | u32::from(SBOX[(b >> 16 & 255) as usize]) << 16
                | u32::from(SBOX[(c >> 8 & 255) as usize]) << 8
                | u32::from(SBOX[(d & 255) as usize]))
                ^ key
        };
        Self::store(
            [
                out(s0, s1, s2, s3, k[last]),
                out(s1, s2, s3, s0, k[last + 1]),
                out(s2, s3, s0, s1, k[last + 2]),
                out(s3, s0, s1, s2, k[last + 3]),
            ],
            block,
        );
    }

    /// Decrypts one block in place.
    pub fn decrypt_block(&self, block: &mut [u8; 16]) {
        let k = &self.decrypt_keys;
        let [mut s0, mut s1, mut s2, mut s3] = Self::load(block);
        s0 ^= k[0];
        s1 ^= k[1];
        s2 ^= k[2];
        s3 ^= k[3];
        for round in 1..self.rounds {
            let t0 = TD0[(s0 >> 24) as usize]
                ^ TD1[(s3 >> 16 & 255) as usize]
                ^ TD2[(s2 >> 8 & 255) as usize]
                ^ TD3[(s1 & 255) as usize]
                ^ k[4 * round];
            let t1 = TD0[(s1 >> 24) as usize]
                ^ TD1[(s0 >> 16 & 255) as usize]
                ^ TD2[(s3 >> 8 & 255) as usize]
                ^ TD3[(s2 & 255) as usize]
                ^ k[4 * round + 1];
            let t2 = TD0[(s2 >> 24) as usize]
                ^ TD1[(s1 >> 16 & 255) as usize]
                ^ TD2[(s0 >> 8 & 255) as usize]
                ^ TD3[(s3 & 255) as usize]
                ^ k[4 * round + 2];
            let t3 = TD0[(s3 >> 24) as usize]
                ^ TD1[(s2 >> 16 & 255) as usize]
                ^ TD2[(s1 >> 8 & 255) as usize]
                ^ TD3[(s0 & 255) as usize]
                ^ k[4 * round + 3];
            (s0, s1, s2, s3) = (t0, t1, t2, t3);
        }
        let last = 4 * self.rounds;
        let out = |a: u32, b: u32, c: u32, d: u32, key: u32| -> u32 {
            (u32::from(INVERSE_SBOX[(a >> 24) as usize]) << 24
                | u32::from(INVERSE_SBOX[(b >> 16 & 255) as usize]) << 16
                | u32::from(INVERSE_SBOX[(c >> 8 & 255) as usize]) << 8
                | u32::from(INVERSE_SBOX[(d & 255) as usize]))
                ^ key
        };
        Self::store(
            [
                out(s0, s3, s2, s1, k[last]),
                out(s1, s0, s3, s2, k[last + 1]),
                out(s2, s1, s0, s3, k[last + 2]),
                out(s3, s2, s1, s0, k[last + 3]),
            ],
            block,
        );
    }
}

/// Encrypts `data` in place in CBC mode, from the initialization vector `iv`.
/// Its length must be a multiple of 16; padding is the caller's.
pub fn cbc_encrypt(aes: &Aes, iv: &[u8; 16], data: &mut [u8]) {
    let mut previous = *iv;
    for chunk in data.as_chunks_mut::<16>().0 {
        let block = chunk;
        for (byte, mask) in block.iter_mut().zip(previous) {
            *byte ^= mask;
        }
        aes.encrypt_block(block);
        previous = *block;
    }
}

/// Decrypts `data` in place in CBC mode, from the initialization vector `iv`.
/// Its length must be a multiple of 16.
pub fn cbc_decrypt(aes: &Aes, iv: &[u8; 16], data: &mut [u8]) {
    let mut previous = *iv;
    for chunk in data.as_chunks_mut::<16>().0 {
        let block = chunk;
        let cipher = *block;
        aes.decrypt_block(block);
        for (byte, mask) in block.iter_mut().zip(previous) {
            *byte ^= mask;
        }
        previous = cipher;
    }
}
