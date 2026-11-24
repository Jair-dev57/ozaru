//! The password algorithms of revisions 5 and 6, which encrypt with AES-256
//! (ISO 32000-2, sections 7.6.4.3.3 and 7.6.4.3.4).

use crate::crypto::{Aes, Sha256, Sha384, Sha512, cbc_decrypt, cbc_encrypt};

/// Passwords are UTF-8, and only their first 127 bytes count.
pub(crate) const MAX_PASSWORD: usize = 127;

/// Algorithm 2.B: the hash of `password` with an 8-byte `salt` and, for owner
/// passwords, the 48 bytes of /U as `user_data`. Revision 5 uses a single SHA-256;
/// revision 6 repeats SHA-256, SHA-384 and SHA-512 with AES-128 in between, at
/// least 64 times, to make guessing passwords slow. It stops once the round number
/// passes the last byte of the round plus 32, so after 287 rounds at most.
pub(crate) fn hash(revision: u8, password: &[u8], salt: &[u8], user_data: &[u8]) -> [u8; 32] {
    let mut first = Sha256::new();
    first.update(password);
    first.update(salt);
    first.update(user_data);
    let mut digest: Vec<u8> = first.finalize().to_vec();
    if revision == 5 {
        return first_32(&digest);
    }

    let mut round = 0u32;
    loop {
        let block: Vec<u8> = [password, &digest, user_data].concat();
        let mut repeated = block.repeat(64);
        let aes = Aes::new(&digest[..16]).expect("the key is 16 bytes");
        let iv: [u8; 16] = digest[16..32].try_into().expect("16 bytes");
        cbc_encrypt(&aes, &iv, &mut repeated);
        let encrypted = repeated;

        // The first 16 bytes, as a big number, modulo 3: since 256 is 1 modulo 3,
        // that's the sum of the bytes modulo 3.
        let choice = encrypted[..16].iter().map(|&b| u32::from(b)).sum::<u32>() % 3;
        digest = match choice {
            0 => Sha256::digest(&encrypted).to_vec(),
            1 => Sha384::digest(&encrypted).to_vec(),
            _ => Sha512::digest(&encrypted).to_vec(),
        };
        round += 1;
        let last = u32::from(*encrypted.last().expect("the data is never empty"));
        if round >= 64 && last + 32 <= round {
            return first_32(&digest);
        }
    }
}

/// The file key, stored in /UE or /OE encrypted with AES-256 in CBC mode, with a
/// zero initialization vector and no padding, under the `intermediate` key.
pub(super) fn unwrap_key(intermediate: &[u8; 32], wrapped: &[u8]) -> Vec<u8> {
    let mut key = wrapped[..32].to_vec();
    let aes = Aes::new(intermediate).expect("the key is 32 bytes");
    cbc_decrypt(&aes, &[0; 16], &mut key);
    key
}

fn first_32(digest: &[u8]) -> [u8; 32] {
    digest[..32]
        .try_into()
        .expect("SHA-2 digests are at least 32 bytes")
}

/// The file key encrypted for /UE or /OE, the opposite of `unwrap_key`.
pub(crate) fn wrap_key(intermediate: &[u8; 32], key: &[u8; 32]) -> Vec<u8> {
    let mut wrapped = key.to_vec();
    let aes = Aes::new(intermediate).expect("the key is 32 bytes");
    cbc_encrypt(&aes, &[0; 16], &mut wrapped);
    wrapped
}

/// `data` encrypted in CBC mode with PKCS#7 padding, after the 16 bytes of
/// `iv`, as strings and streams are written (ISO 32000-2, section 7.6.3).
pub(crate) fn encrypt_cbc(key: &[u8; 32], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let padding = 16 - data.len() % 16;
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat_n(padding as u8, padding));
    let aes = Aes::new(key).expect("the key is 32 bytes");
    cbc_encrypt(&aes, iv, &mut padded);
    let mut out = iv.to_vec();
    out.extend_from_slice(&padded);
    out
}

/// One block of AES-256 in ECB mode, for /Perms.
pub(crate) fn encrypt_block(key: &[u8; 32], block: &[u8; 16]) -> Vec<u8> {
    let aes = Aes::new(key).expect("the key is 32 bytes");
    let mut data = *block;
    aes.encrypt_block(&mut data);
    data.to_vec()
}
