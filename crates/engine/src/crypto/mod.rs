//! The hashes and ciphers that PDF encryption uses (ISO 32000-2, section 7.6),
//! written for the engine: MD5, RC4, SHA-256, SHA-384, SHA-512 and AES in CBC
//! mode. They are only for PDF's own algorithms: MD5 and RC4 are weak, and are
//! here because old files are protected with them.

mod aes;
mod md5;
mod rc4;
mod sha2;

pub use aes::{Aes, cbc_decrypt, cbc_encrypt};
pub use md5::Md5;
pub use rc4::{Rc4, rc4};
pub use sha2::{Sha256, Sha384, Sha512};
