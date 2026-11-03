//! PDF syntax: the objects a PDF file is made of (ISO 32000-2, section 7.3).

pub(crate) mod chars;
mod indirect;
mod lexer;
mod object;
mod parser;
mod stream;
mod token;

pub use indirect::{
    IndirectObject, ParsedIndirectObject, parse_indirect_object, parse_indirect_object_with_lengths,
};
pub use lexer::Lexer;
pub use object::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};
pub use parser::{ParsedObject, Parser, parse_object};
pub use stream::Stream;
pub use token::{Token, TokenKind};
