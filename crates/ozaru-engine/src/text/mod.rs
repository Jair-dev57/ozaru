//! Text: the characters of a page, put together into words and lines
//! (ISO 32000-2, section 9.10).

mod glyph;
mod line;

pub use glyph::{TextChar, characters};
pub use line::{TextLine, lines};
