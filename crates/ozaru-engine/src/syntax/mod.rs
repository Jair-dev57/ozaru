//! PDF syntax: the objects a PDF file is made of (ISO 32000-2, section 7.3).

mod chars;
mod lexer;
mod object;
mod parser;
mod token;

pub use lexer::Lexer;
pub use object::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
pub use parser::{ParsedObject, Parser, parse_object};
pub use token::{Token, TokenKind};
