//! PDF syntax: the objects a PDF file is made of (ISO 32000-2, section 7.3).

mod chars;
mod lexer;
mod object;
mod token;

pub use lexer::Lexer;
pub use object::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
pub use token::{Token, TokenKind};
