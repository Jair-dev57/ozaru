use aes::Aes128;
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockModeDecrypt, KeyIvInit};
use md5::{Digest, Md5};

use super::standard::rc4;
use super::{CryptMethod, FileKey, StandardSecurity};
use crate::base::{DiagnosticKind, Diagnostics, Result};
use crate::syntax::{Dictionary, Object, ObjectId};

const AES_BLOCK: usize = 16;

impl StandardSecurity {
    /// Decrypts a string of the object `id`, with the method for strings.
    ///
    /// `offset` is used for diagnostics. AES data that isn't a whole number of
    /// blocks, or whose padding is wrong, is decrypted as far as it goes, with a
    /// `DamagedEncryptedData` diagnostic.
    pub fn decrypt_string(
        &self,
        key: &FileKey,
        id: ObjectId,
        data: &[u8],
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Result<Vec<u8>> {
        Ok(self.decrypt(self.string_method(), key, id, data, offset, diagnostics))
    }

    /// Decrypts the raw data of a stream of the object `id`, before its filters
    /// are applied, with the method for streams. Check
    /// [`decrypts_stream`](Self::decrypts_stream) first.
    pub fn decrypt_stream_data(
        &self,
        key: &FileKey,
        id: ObjectId,
        data: &[u8],
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Result<Vec<u8>> {
        Ok(self.decrypt(self.stream_method(), key, id, data, offset, diagnostics))
    }

    /// Whether the data of a stream with this dictionary is encrypted. The
    /// standard leaves cross-reference streams unencrypted, and the metadata
    /// stream too when `/EncryptMetadata` is false.
    pub fn decrypts_stream(&self, dictionary: &Dictionary) -> bool {
        let kind = dictionary.get("Type").and_then(Object::as_name);
        match kind.map(|kind| kind.as_bytes()) {
            Some(b"XRef") => false,
            Some(b"Metadata") => self.encrypts_metadata(),
            _ => true,
        }
    }

    /// Decrypts every string inside `object`, which is object `id`: in arrays,
    /// dictionaries and stream dictionaries, at any depth. The data of a stream is
    /// left for [`decrypt_stream_data`](Self::decrypt_stream_data).
    ///
    /// Objects inside an object stream must not be decrypted: the object stream
    /// was decrypted as a whole. Neither must the `/Encrypt` dictionary.
    pub fn decrypt_object(
        &self,
        key: &FileKey,
        id: ObjectId,
        object: &mut Object,
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Result<()> {
        // An explicit stack, so a deeply nested object can't overflow the call
        // stack.
        let mut pending: Vec<&mut Object> = vec![object];
        while let Some(object) = pending.pop() {
            match object {
                Object::String(string) => {
                    let plain =
                        self.decrypt_string(key, id, string.as_bytes(), offset, diagnostics)?;
                    string.set_bytes(plain);
                }
                Object::Array(items) => pending.extend(items.iter_mut()),
                Object::Dictionary(dictionary) => pending.extend(dictionary.values_mut()),
                Object::Stream(stream) => pending.extend(stream.dictionary_mut().values_mut()),
                _ => {}
            }
        }
        Ok(())
    }

    fn decrypt(
        &self,
        method: CryptMethod,
        key: &FileKey,
        id: ObjectId,
        data: &[u8],
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Vec<u8> {
        match method {
            CryptMethod::Identity => data.to_vec(),
            CryptMethod::Rc4 => rc4(&object_key(key, id, false), data),
            CryptMethod::Aes128 => aes_cbc(&object_key(key, id, true), data, offset, diagnostics),
        }
    }
}

// AES in CBC mode (ISO 32000-2, section 7.6.3): the data starts with a random
// 16-byte initialization vector, and the plain text is padded to whole blocks with
// PKCS#7, n bytes of value n.
fn aes_cbc(key: &[u8], data: &[u8], offset: usize, diagnostics: &mut Diagnostics) -> Vec<u8> {
    // Some writers leave empty strings empty instead of encrypting them.
    if data.is_empty() {
        return Vec::new();
    }
    let damaged = |diagnostics: &mut Diagnostics| {
        diagnostics.push(DiagnosticKind::DamagedEncryptedData, offset);
    };
    let Some((iv, rest)) = data.split_first_chunk::<AES_BLOCK>() else {
        damaged(diagnostics);
        return Vec::new();
    };
    let whole = rest.len() / AES_BLOCK * AES_BLOCK;
    // Data cut short is reported once, not again for the padding it lost.
    if whole != rest.len() || whole == 0 {
        damaged(diagnostics);
        return decrypt_blocks(key, iv, &rest[..whole]);
    }

    let mut plain = decrypt_blocks(key, iv, rest);

    let padding = plain.last().map_or(0, |&n| usize::from(n));
    let valid = (1..=AES_BLOCK).contains(&padding)
        && padding <= plain.len()
        && plain[plain.len() - padding..]
            .iter()
            .all(|&byte| usize::from(byte) == padding);
    if valid {
        plain.truncate(plain.len() - padding);
    } else {
        damaged(diagnostics);
    }
    plain
}

fn decrypt_blocks(key: &[u8], iv: &[u8; AES_BLOCK], blocks: &[u8]) -> Vec<u8> {
    let mut plain = blocks.to_vec();
    cbc::Decryptor::<Aes128>::new_from_slices(key, iv)
        .expect("AES-128 keys and initialization vectors are 16 bytes")
        .decrypt_padded::<NoPadding>(&mut plain)
        .expect("the data is a whole number of blocks");
    plain
}

// Algorithm 1: each object has its own key, the MD5 of the file key, the object
// number (3 bytes) and its generation (2 bytes), and "sAlT" for AES. It's at most
// 16 bytes long.
fn object_key(key: &FileKey, id: ObjectId, aes: bool) -> Vec<u8> {
    let key = key.as_bytes();
    let mut hash = Md5::new();
    hash.update(key);
    hash.update(&id.number.to_le_bytes()[..3]);
    hash.update(id.generation.to_le_bytes());
    if aes {
        hash.update(b"sAlT");
    }
    let mut digest = hash.finalize().to_vec();
    digest.truncate((key.len() + 5).min(16));
    digest
}
