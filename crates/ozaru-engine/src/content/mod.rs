//! Content streams: the operations that paint a page or a form, and what they
//! leave on it (ISO 32000-2, sections 7.8, 8 and 9).

mod operators;

pub use operators::{ContentParser, InlineImage, Operation, Operator};
