//! Writing PDF files: objects as PDF syntax, whole documents and incremental
//! updates (ISO 32000-2, sections 7.3 and 7.5).

mod editor;
mod file;
mod serialize;

pub use editor::Editor;
pub use file::{FileWriter, Layout};
pub use serialize::{write_dictionary, write_indirect, write_object};
