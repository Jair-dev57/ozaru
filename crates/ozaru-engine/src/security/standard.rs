use md5::{Digest, Md5};
use rc4::{KeyInit, Rc4, StreamCipher};

use super::aes256;

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
/// Revisions 2 to 6 are supported:
///
/// - **2 to 4:** keys of 40 to 128 bits, derived with MD5, for RC4 or, from
///   version 4 on, AES-128.
/// - **5 and 6:** a random 256-bit key for AES-256, stored in /UE encrypted under
///   a SHA-2 hash of the password. Revision 5 is an older Adobe extension, still
///   found in files.
///
/// Both passwords open a file: the user password, often empty, and the owner
/// password, which the standard allows in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardSecurity {
    version: u8,
    revision: u8,
    key_length: usize,
    stream_method: CryptMethod,
    string_method: CryptMethod,
    owner: Vec<u8>,
    user: Vec<u8>,
    // The encrypted file key of revisions 5 and 6, empty before them.
    user_key: Vec<u8>,
    // The same key encrypted for the owner password, /OE. Empty before revision 5,
    // or if the file doesn't have it, and then the owner password can't be used.
    owner_key: Vec<u8>,
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
    /// AES-256 in CBC mode (`/AESV3` in a crypt filter), from revision 5 on.
    Aes256,
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
            Some(version @ (1 | 2 | 4 | 5)) => version as u8,
            Some(_) => return Err(unsupported()),
        };
        let revision = match integer("R") {
            Some(revision @ 2..=6) => revision as u8,
            _ => return Err(invalid()),
        };
        // Version 1 is always 40 bits, version 4 always 128 and version 5 always
        // 256. Version 2 says it in /Length, a multiple of 8 from 40 to 128, which
        // is 40 by default.
        let key_length = match (version, integer("Length")) {
            (1, _) => 5,
            (4, _) => 16,
            (5, _) => 32,
            (_, None) => 5,
            (_, Some(bits @ 40..=128)) if bits % 8 == 0 => bits as usize / 8,
            _ => return Err(invalid()),
        };
        let matching = match version {
            1 | 2 => matches!(revision, 2 | 3),
            4 => revision == 4,
            _ => matches!(revision, 5 | 6),
        };
        if !matching || (revision == 2 && key_length != 5) {
            return Err(invalid());
        }
        let (stream_method, string_method) = if version >= 4 {
            (
                crypt_filter(encrypt, "StmF", offset)?,
                crypt_filter(encrypt, "StrF", offset)?,
            )
        } else {
            (CryptMethod::Rc4, CryptMethod::Rc4)
        };

        // /O and /U are 32 bytes, and 48 from revision 5 on: a hash and two salts.
        let entry_length = if revision >= 5 { 48 } else { 32 };
        let owner = string("O")
            .filter(|owner| owner.len() >= entry_length)
            .ok_or_else(invalid)?;
        let user = string("U")
            .filter(|user| user.len() >= entry_length)
            .ok_or_else(invalid)?;
        let user_key = if revision >= 5 {
            string("UE")
                .filter(|key| key.len() >= 32)
                .ok_or_else(invalid)?[..32]
                .to_vec()
        } else {
            Vec::new()
        };
        let owner_key = match string("OE") {
            Some(key) if revision >= 5 && key.len() >= 32 => key[..32].to_vec(),
            _ => Vec::new(),
        };
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
            owner: owner[..entry_length].to_vec(),
            user: user[..entry_length].to_vec(),
            user_key,
            owner_key,
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

    /// Length of the file key, in bytes: 5 for 40 bits, up to 16 for 128 bits, and
    /// 32 for AES-256.
    pub fn key_length(&self) -> usize {
        self.key_length
    }

    /// Whether the metadata stream is encrypted: false only when `/EncryptMetadata`
    /// says so, from version 4 on.
    pub fn encrypts_metadata(&self) -> bool {
        self.encrypt_metadata
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

    /// The file key if `password` is the user password or the owner password, or
    /// `None` if it is neither. From revision 5 on, the password is UTF-8.
    ///
    /// Most encrypted files have an empty user password, so `authenticate(b"")` is
    /// usually the first thing to try. Both passwords give the same key.
    pub fn authenticate(&self, password: &[u8]) -> Option<FileKey> {
        self.authenticate_user(password)
            .or_else(|| self.authenticate_owner(password))
    }

    fn authenticate_user(&self, password: &[u8]) -> Option<FileKey> {
        if self.revision >= 5 {
            return self.authenticate_aes256(password);
        }
        let key = self.file_key(password);
        let expected = self.user_entry(&key);
        // Revision 2 compares the whole /U; 3 and 4 only its first 16 bytes, since
        // the rest is arbitrary padding.
        let compared = if self.revision == 2 { 32 } else { 16 };
        (expected[..compared] == self.user[..compared]).then_some(FileKey(key))
    }

    // Algorithms 7 and 12: the owner password, which opens the file as well.
    fn authenticate_owner(&self, password: &[u8]) -> Option<FileKey> {
        if self.revision >= 5 {
            return self.authenticate_owner_aes256(password);
        }
        // /O is the padded user password encrypted with RC4, under a key derived
        // from the owner password, so decrypting it gives the user password.
        let mut digest = Md5::digest(padded(password)).to_vec();
        if self.revision >= 3 {
            for _ in 0..50 {
                digest = Md5::digest(&digest).to_vec();
            }
        }
        let key = &digest[..self.key_length];
        let user_password = if self.revision == 2 {
            rc4(key, &self.owner)
        } else {
            (0..=19u8).rev().fold(self.owner.clone(), |value, round| {
                let round_key: Vec<u8> = key.iter().map(|byte| byte ^ round).collect();
                rc4(&round_key, &value)
            })
        };
        self.authenticate_user(&user_password)
    }

    // Algorithm 12: /O is the hash of the owner password with its first salt and
    // /U; the hash with the second salt decrypts the file key from /OE.
    fn authenticate_owner_aes256(&self, password: &[u8]) -> Option<FileKey> {
        if self.owner_key.is_empty() {
            return None;
        }
        let password = &password[..password.len().min(aes256::MAX_PASSWORD)];
        let (hash, rest) = self.owner.split_at(32);
        let (validation_salt, key_salt) = rest.split_at(8);
        if aes256::hash(self.revision, password, validation_salt, &self.user) != hash {
            return None;
        }
        let intermediate = aes256::hash(self.revision, password, key_salt, &self.user);
        Some(FileKey(aes256::unwrap_key(&intermediate, &self.owner_key)))
    }

    // Algorithms 2.A and 11: /U is the hash of the password with its first salt;
    // the hash with the second salt decrypts the file key from /UE.
    fn authenticate_aes256(&self, password: &[u8]) -> Option<FileKey> {
        let password = &password[..password.len().min(aes256::MAX_PASSWORD)];
        let (hash, rest) = self.user.split_at(32);
        let (validation_salt, key_salt) = rest.split_at(8);
        if aes256::hash(self.revision, password, validation_salt, &[]) != hash {
            return None;
        }
        let intermediate = aes256::hash(self.revision, password, key_salt, &[]);
        Some(FileKey(aes256::unwrap_key(&intermediate, &self.user_key)))
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

// The method of the crypt filter named by `key` (/StmF or /StrF) from version 4 on:
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
        Some(b"AESV3") => Ok(CryptMethod::Aes256),
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

pub(super) fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut output = data.to_vec();
    Rc4::new_from_slice(key)
        .expect("PDF keys are 5 to 16 bytes, which RC4 accepts")
        .apply_keystream(&mut output);
    output
}
