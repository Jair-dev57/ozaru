use md5::{Digest, Md5};

use super::standard::rc4;
use super::{CryptMethod, FileKey, StandardSecurity};
use crate::base::{Error, ErrorKind, Result};
use crate::syntax::{Dictionary, Object, ObjectId};

impl StandardSecurity {
    /// Decrypts a string of the object `id`, with the method for strings.
    ///
    /// `offset` is used for errors. AES isn't supported yet, so it is an
    /// [`ErrorKind::UnsupportedEncryption`] error.
    pub fn decrypt_string(
        &self,
        key: &FileKey,
        id: ObjectId,
        data: &[u8],
        offset: usize,
    ) -> Result<Vec<u8>> {
        self.decrypt(self.string_method(), key, id, data, offset)
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
    ) -> Result<Vec<u8>> {
        self.decrypt(self.stream_method(), key, id, data, offset)
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
    ) -> Result<()> {
        // An explicit stack, so a deeply nested object can't overflow the call
        // stack.
        let mut pending: Vec<&mut Object> = vec![object];
        while let Some(object) = pending.pop() {
            match object {
                Object::String(string) => {
                    let plain = self.decrypt_string(key, id, string.as_bytes(), offset)?;
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
    ) -> Result<Vec<u8>> {
        match method {
            CryptMethod::Identity => Ok(data.to_vec()),
            CryptMethod::Rc4 => Ok(rc4(&object_key(key, id, false), data)),
            // AES-128 comes in the next step.
            CryptMethod::Aes128 => Err(Error::new(ErrorKind::UnsupportedEncryption, offset)),
        }
    }
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
