//! The password algorithms of revisions 5 and 6, which encrypt with AES-256
//! (ISO 32000-2, sections 7.6.4.3.3 and 7.6.4.3.4).

use aes::{Aes128, Aes256};
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use sha2::{Digest, Sha256, Sha384, Sha512};

/// Passwords are UTF-8, and only their first 127 bytes count.
pub(super) const MAX_PASSWORD: usize = 127;

/// Algorithm 2.B: the hash of `password` with an 8-byte `salt` and, for owner
/// passwords, the 48 bytes of /U as `user_data`. Revision 5 uses a single SHA-256;
/// revision 6 repeats SHA-256, SHA-384 and SHA-512 with AES-128 in between, at
/// least 64 times, to make guessing passwords slow. It stops once the round number
/// passes the last byte of the round plus 32, so after 287 rounds at most.
pub(super) fn hash(revision: u8, password: &[u8], salt: &[u8], user_data: &[u8]) -> [u8; 32] {
    let mut digest: Vec<u8> = Sha256::new()
        .chain_update(password)
        .chain_update(salt)
        .chain_update(user_data)
        .finalize()
        .to_vec();
    if revision == 5 {
        return first_32(&digest);
    }

    let mut round = 0u32;
    loop {
        let block: Vec<u8> = [password, &digest, user_data].concat();
        let mut repeated = block.repeat(64);
        cbc::Encryptor::<Aes128>::new_from_slices(&digest[..16], &digest[16..32])
            .expect("the key and initialization vector are 16 bytes")
            .encrypt_padded::<NoPadding>(&mut repeated, block.len() * 64)
            .expect("64 copies of the block are a whole number of AES blocks");
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
    cbc::Decryptor::<Aes256>::new_from_slices(intermediate, &[0; 16])
        .expect("the key is 32 bytes and the initialization vector 16")
        .decrypt_padded::<NoPadding>(&mut key)
        .expect("32 bytes are two AES blocks");
    key
}

fn first_32(digest: &[u8]) -> [u8; 32] {
    digest[..32]
        .try_into()
        .expect("SHA-2 digests are at least 32 bytes")
}
