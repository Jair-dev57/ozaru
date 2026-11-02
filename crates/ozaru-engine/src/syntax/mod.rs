//! PDF syntax: the objects a PDF file is made of (ISO 32000-2, section 7.3).

mod object;

pub use object::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
