//! Writing PDF files: objects as PDF syntax, whole documents and incremental
//! updates (ISO 32000-2, sections 7.3 and 7.5).

mod editor;
mod encrypt;
mod file;
mod forms;
mod serialize;

pub use editor::{Editor, NewAttachment, PageLabel};
pub use encrypt::{Encrypter, Encryption, RANDOM_BYTES};
pub use file::{FileWriter, Layout};
pub use forms::fill_field;
pub use serialize::{write_dictionary, write_indirect, write_object};
