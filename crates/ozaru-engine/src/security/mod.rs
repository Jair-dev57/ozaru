//! Decryption of encrypted PDF files (ISO 32000-2, section 7.6).
//!
//! Many PDFs are encrypted even though they open without a password: the author
//! sets an owner password to restrict printing or copying, and the content is
//! encrypted with a key that anyone can derive from the empty user password.
//!
//! The cryptographic primitives come from RustCrypto (`md-5`, `rc4`); only the PDF
//! algorithms that combine them are written here.

mod standard;

pub use standard::{CryptMethod, FileKey, StandardSecurity};
