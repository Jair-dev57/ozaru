//! Encrypting a file when it is saved: the standard security handler, revision
//! 6, with AES-256 (ISO 32000-2, section 7.6.4.4).
//!
//! The engine has no source of randomness of its own: whoever calls it gives
//! the random bytes the file key, the salts and the initialization vectors are
//! made from, which should come from the operating system.

use sha2::{Digest, Sha256};

use crate::base::{Error, ErrorKind, Result};
use crate::security::aes256;
use crate::syntax::{Dictionary, Name, Object, PdfString, StringFormat};

/// How a file is encrypted when saved.
#[derive(Debug, Clone)]
pub struct Encryption<'a> {
    /// What opens the file for reading. It may be empty: anyone can open the
    /// file, and only the permissions restrict what they do.
    pub user_password: &'a [u8],
    /// What opens the file with every permission.
    pub owner_password: &'a [u8],
    /// The permission bits that are allowed (section 7.6.4.2, table 22), such
    /// as 4 for printing; the bits the standard reserves are set for you.
    pub permissions: u32,
    /// Whether the XMP metadata is encrypted too.
    pub encrypt_metadata: bool,
}

/// The random bytes `Encrypter::new` takes at least.
pub const RANDOM_BYTES: usize = 128;

/// Encrypts the strings and streams of the objects it is given, and has made
/// the `/Encrypt` dictionary that tells readers how to decrypt them.
pub struct Encrypter {
    key: [u8; 32],
    seed: [u8; 32],
    counter: u64,
    encrypt_metadata: bool,
}

impl Encrypter {
    /// The encrypter, and the `/Encrypt` dictionary, which isn't itself
    /// encrypted. It is an `InvalidFieldValue`-free error: `NotEnoughRandom`
    /// if there are fewer than [`RANDOM_BYTES`] of `random`.
    pub fn new(encryption: &Encryption<'_>, random: &[u8]) -> Result<(Self, Dictionary)> {
        if random.len() < RANDOM_BYTES {
            return Err(Error::new(ErrorKind::NotEnoughRandom, 0));
        }
        let take = |from: usize, length: usize| &random[from..from + length];
        let key: [u8; 32] = take(0, 32).try_into().expect("32 bytes");
        let seed: [u8; 32] = take(96, 32).try_into().expect("32 bytes");
        let (user_validation, user_key_salt) = (take(32, 8), take(40, 8));
        let (owner_validation, owner_key_salt) = (take(48, 8), take(56, 8));
        let cut = |password: &[u8]| password[..password.len().min(aes256::MAX_PASSWORD)].to_vec();
        let (user_password, owner_password) = (
            cut(encryption.user_password),
            cut(encryption.owner_password),
        );

        // /U: the hash of the user password with a salt, and both salts.
        let mut user = aes256::hash(6, &user_password, user_validation, &[]).to_vec();
        user.extend_from_slice(user_validation);
        user.extend_from_slice(user_key_salt);
        let user_key = aes256::hash(6, &user_password, user_key_salt, &[]);
        let user_wrapped = aes256::wrap_key(&user_key, &key);

        // /O: the same with the owner password, and the 48 bytes of /U.
        let mut owner = aes256::hash(6, &owner_password, owner_validation, &user).to_vec();
        owner.extend_from_slice(owner_validation);
        owner.extend_from_slice(owner_key_salt);
        let owner_key = aes256::hash(6, &owner_password, owner_key_salt, &user);
        let owner_wrapped = aes256::wrap_key(&owner_key, &key);

        // The bits the standard fixes: 7 and 8 set, and 13 to 32 set.
        let permissions = encryption.permissions | 0xFFFF_F0C0;
        let mut block = [0u8; 16];
        block[..4].copy_from_slice(&permissions.to_le_bytes());
        block[4..8].copy_from_slice(&[0xFF; 4]);
        block[8] = if encryption.encrypt_metadata {
            b'T'
        } else {
            b'F'
        };
        block[9..12].copy_from_slice(b"adb");
        block[12..].copy_from_slice(take(88, 4));
        let perms = aes256::encrypt_block(&key, &block);

        let string =
            |bytes: Vec<u8>| Object::String(PdfString::new(bytes, StringFormat::Hexadecimal));
        let mut filter = Dictionary::new();
        filter.insert(Name::from("AuthEvent"), Object::Name(Name::from("DocOpen")));
        filter.insert(Name::from("CFM"), Object::Name(Name::from("AESV3")));
        filter.insert(Name::from("Length"), Object::Integer(32));
        let mut filters = Dictionary::new();
        filters.insert(Name::from("StdCF"), Object::Dictionary(filter));

        let mut dictionary = Dictionary::new();
        dictionary.insert(Name::from("Filter"), Object::Name(Name::from("Standard")));
        dictionary.insert(Name::from("V"), Object::Integer(5));
        dictionary.insert(Name::from("R"), Object::Integer(6));
        dictionary.insert(Name::from("Length"), Object::Integer(256));
        dictionary.insert(Name::from("CF"), Object::Dictionary(filters));
        dictionary.insert(Name::from("StmF"), Object::Name(Name::from("StdCF")));
        dictionary.insert(Name::from("StrF"), Object::Name(Name::from("StdCF")));
        dictionary.insert(Name::from("O"), string(owner));
        dictionary.insert(Name::from("U"), string(user));
        dictionary.insert(Name::from("OE"), string(owner_wrapped));
        dictionary.insert(Name::from("UE"), string(user_wrapped));
        dictionary.insert(
            Name::from("P"),
            Object::Integer(i64::from(permissions as i32)),
        );
        dictionary.insert(Name::from("Perms"), string(perms));
        if !encryption.encrypt_metadata {
            dictionary.insert(Name::from("EncryptMetadata"), Object::Boolean(false));
        }
        Ok((
            Self {
                key,
                seed,
                counter: 0,
                encrypt_metadata: encryption.encrypt_metadata,
            },
            dictionary,
        ))
    }

    // A new initialization vector, unpredictable without the seed.
    fn iv(&mut self) -> [u8; 16] {
        self.counter += 1;
        let digest = Sha256::new()
            .chain_update(self.seed)
            .chain_update(self.counter.to_be_bytes())
            .finalize();
        digest[..16].try_into().expect("16 bytes")
    }

    fn encrypt(&mut self, data: &[u8]) -> Vec<u8> {
        let iv = self.iv();
        aes256::encrypt_cbc(&self.key, &iv, data)
    }

    /// Encrypts every string of `object` and, if there is one, the data of the
    /// stream it is the dictionary of.
    pub fn encrypt_object(
        &mut self,
        object: &mut Object,
        data: Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        let metadata = matches!(object, Object::Dictionary(d)
            if d.get("Type").and_then(Object::as_name).is_some_and(|t| t.as_bytes() == b"Metadata"));
        self.strings(object);
        if metadata && !self.encrypt_metadata {
            return data;
        }
        data.map(|data| self.encrypt(&data))
    }

    fn strings(&mut self, object: &mut Object) {
        match object {
            Object::String(string) => {
                let encrypted = self.encrypt(string.as_bytes());
                string.set_bytes(encrypted);
            }
            Object::Array(items) => items.iter_mut().for_each(|item| self.strings(item)),
            Object::Dictionary(dictionary) => {
                let mut values: Vec<&mut Object> = dictionary.values_mut().collect();
                for value in values.iter_mut() {
                    self.strings(value);
                }
            }
            Object::Stream(stream) => {
                for value in stream.dictionary_mut().values_mut() {
                    self.strings(value);
                }
            }
            _ => {}
        }
    }
}
