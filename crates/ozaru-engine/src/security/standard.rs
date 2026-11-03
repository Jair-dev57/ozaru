use md5::{Digest, Md5};
use rc4::{KeyInit, Rc4, StreamCipher};

use crate::base::{Error, ErrorKind, Result};
use crate::syntax::{Dictionary, Object};

/// The padding string of the standard (ISO 32000-2, section 7.6.4.3.2). A
/// password shorter than 32 bytes is completed with it, so the empty password is
/// this whole string.
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// The settings of the standard security handler, from a file's `/Encrypt`
/// dictionary and the first part of its `/ID` (ISO 32000-2, section 7.6.4).
///
/// Revisions 2 to 4 are supported so far: keys of 40 to 128 bits, derived with
/// MD5, for RC4 or, from version 4 on, AES-128.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardSecurity {
    version: u8,
    revision: u8,
    key_length: usize,
    stream_method: CryptMethod,
    string_method: CryptMethod,
    owner: Vec<u8>,
    user: Vec<u8>,
    permissions: u32,
    encrypt_metadata: bool,
    file_id: Vec<u8>,
}

/// How streams or strings are encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptMethod {
    /// Not encrypted: the `/Identity` crypt filter.
    Identity,
    /// RC4, the only method of versions 1 and 2 (`/V2` in a crypt filter).
    Rc4,
    /// AES-128 in CBC mode (`/AESV2` in a crypt filter).
    Aes128,
}

/// The key that decrypts a file, derived from a correct password.
#[derive(Clone, PartialEq, Eq)]
pub struct FileKey(Vec<u8>);

impl FileKey {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

// The key decrypts the whole file, so it isn't printed.
impl std::fmt::Debug for FileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileKey({} bytes)", self.0.len())
    }
}

impl StandardSecurity {
    /// Reads `encrypt`, a file's `/Encrypt` dictionary. `file_id` is the first
    /// string of the trailer's `/ID`, empty if it has none. `offset` is used for
    /// errors.
    ///
    /// A handler other than `/Standard`, or a version or revision not supported
    /// yet, is an [`ErrorKind::UnsupportedEncryption`] error; a missing or wrong
    /// entry is an [`ErrorKind::InvalidEncryption`] error.
    pub fn new(encrypt: &Dictionary, file_id: &[u8], offset: usize) -> Result<Self> {
        let unsupported = || Error::new(ErrorKind::UnsupportedEncryption, offset);
        let invalid = || Error::new(ErrorKind::InvalidEncryption, offset);
        let integer = |key: &str| encrypt.get(key).and_then(Object::as_integer);
        let string = |key: &str| {
            encrypt
                .get(key)
                .and_then(Object::as_string)
                .map(|string| string.as_bytes().to_vec())
        };

        let handler = encrypt.get("Filter").and_then(Object::as_name);
        if handler.is_none_or(|handler| handler.as_bytes() != b"Standard") {
            return Err(unsupported());
        }
        let version = match integer("V") {
            None | Some(0) => return Err(invalid()),
            Some(version @ (1 | 2 | 4)) => version as u8,
            Some(_) => return Err(unsupported()),
        };
        let revision = match integer("R") {
            Some(revision @ 2..=4) => revision as u8,
            Some(5 | 6) => return Err(unsupported()),
            _ => return Err(invalid()),
        };
        // Version 1 is always 40 bits and version 4 always 128. Version 2 says it in
        // /Length, a multiple of 8 from 40 to 128, which is 40 by default.
        let key_length = match (version, integer("Length")) {
            (1, _) => 5,
            (4, _) => 16,
            (_, None) => 5,
            (_, Some(bits @ 40..=128)) if bits % 8 == 0 => bits as usize / 8,
            _ => return Err(invalid()),
        };
        if (revision == 2 && key_length != 5) || (version == 4) != (revision == 4) {
            return Err(invalid());
        }
        let (stream_method, string_method) = if version == 4 {
            (
                crypt_filter(encrypt, "StmF", offset)?,
                crypt_filter(encrypt, "StrF", offset)?,
            )
        } else {
            (CryptMethod::Rc4, CryptMethod::Rc4)
        };

        let owner = string("O")
            .filter(|owner| owner.len() >= 32)
            .ok_or_else(invalid)?;
        let user = string("U")
            .filter(|user| user.len() >= 32)
            .ok_or_else(invalid)?;
        // /P is a 32-bit signed integer, but some writers store it unsigned.
        let permissions = integer("P")
            .filter(|&p| i64::from(i32::MIN) <= p && p <= i64::from(u32::MAX))
            .ok_or_else(invalid)? as u32;
        let encrypt_metadata = encrypt
            .get("EncryptMetadata")
            .and_then(Object::as_bool)
            .unwrap_or(true);

        Ok(Self {
            version,
            revision,
            key_length,
            stream_method,
            string_method,
            owner: owner[..32].to_vec(),
            user: user[..32].to_vec(),
            permissions,
            encrypt_metadata,
            file_id: file_id.to_vec(),
        })
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn revision(&self) -> u8 {
        self.revision
    }

    /// Length of the file key, in bytes: 5 for 40 bits, up to 16 for 128 bits.
    pub fn key_length(&self) -> usize {
        self.key_length
    }

    /// How the file's streams are encrypted.
    pub fn stream_method(&self) -> CryptMethod {
        self.stream_method
    }

    /// How the file's strings are encrypted.
    pub fn string_method(&self) -> CryptMethod {
        self.string_method
    }

    /// The `/P` flags of what the document allows, such as printing (bit 3) or
    /// copying text (bit 5). Only a reader can enforce them: they don't change
    /// what can be decrypted.
    pub fn permissions(&self) -> u32 {
        self.permissions
    }

    /// The file key if `password` is the user password, or `None` if it isn't.
    ///
    /// Most encrypted files have an empty user password, so `authenticate(b"")` is
    /// usually the first thing to try.
    pub fn authenticate(&self, password: &[u8]) -> Option<FileKey> {
        let key = self.file_key(password);
        let expected = self.user_entry(&key);
        // Revision 2 compares the whole /U; 3 and 4 only its first 16 bytes, since
        // the rest is arbitrary padding.
        let compared = if self.revision == 2 { 32 } else { 16 };
        (expected[..compared] == self.user[..compared]).then_some(FileKey(key))
    }

    // Algorithm 2: the file key for `password`.
    fn file_key(&self, password: &[u8]) -> Vec<u8> {
        let mut hash = Md5::new();
        hash.update(padded(password));
        hash.update(&self.owner);
        hash.update(self.permissions.to_le_bytes());
        hash.update(&self.file_id);
        if self.revision >= 4 && !self.encrypt_metadata {
            hash.update([0xFF; 4]);
        }
        let mut digest = hash.finalize().to_vec();
        if self.revision >= 3 {
            for _ in 0..50 {
                digest = Md5::digest(&digest[..self.key_length]).to_vec();
            }
        }
        digest.truncate(self.key_length);
        digest
    }

    // Algorithms 4 and 5: the /U entry the file would have if `key` were right.
    fn user_entry(&self, key: &[u8]) -> Vec<u8> {
        if self.revision == 2 {
            return rc4(key, &PADDING);
        }
        let mut hash = Md5::new();
        hash.update(PADDING);
        hash.update(&self.file_id);
        let mut value = rc4(key, &hash.finalize());
        for round in 1..=19u8 {
            let round_key: Vec<u8> = key.iter().map(|byte| byte ^ round).collect();
            value = rc4(&round_key, &value);
        }
        value
    }
}

// The method of the crypt filter named by `key` (/StmF or /StrF) in version 4:
// /Identity by default, or an entry of /CF with its /CFM.
fn crypt_filter(encrypt: &Dictionary, key: &str, offset: usize) -> Result<CryptMethod> {
    let invalid = || Error::new(ErrorKind::InvalidEncryption, offset);
    let name = match encrypt.get(key) {
        None => return Ok(CryptMethod::Identity),
        Some(value) => value.as_name().ok_or_else(invalid)?,
    };
    if name.as_bytes() == b"Identity" {
        return Ok(CryptMethod::Identity);
    }
    let filter = encrypt
        .get("CF")
        .and_then(Object::as_dictionary)
        .and_then(|filters| filters.get(name.as_bytes()))
        .and_then(Object::as_dictionary)
        .ok_or_else(invalid)?;
    match filter
        .get("CFM")
        .and_then(Object::as_name)
        .map(|n| n.as_bytes())
    {
        None | Some(b"None") => Ok(CryptMethod::Identity),
        Some(b"V2") => Ok(CryptMethod::Rc4),
        Some(b"AESV2") => Ok(CryptMethod::Aes128),
        Some(_) => Err(Error::new(ErrorKind::UnsupportedEncryption, offset)),
    }
}

// The first 32 bytes of the password, completed with the padding string.
fn padded(password: &[u8]) -> [u8; 32] {
    let mut padded = PADDING;
    let length = password.len().min(32);
    padded[..length].copy_from_slice(&password[..length]);
    padded[length..].copy_from_slice(&PADDING[..32 - length]);
    padded
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut output = data.to_vec();
    Rc4::new_from_slice(key)
        .expect("PDF keys are 5 to 16 bytes, which RC4 accepts")
        .apply_keystream(&mut output);
    output
}
