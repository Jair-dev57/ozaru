//! Text: the characters of a page, put together into words and lines
//! (ISO 32000-2, section 9.10).

mod extract;
mod glyph;
mod line;
mod word;

pub use extract::{PageProblem, PageText, document_text, page_text};
pub use glyph::{TextChar, characters};
pub use line::{TextLine, lines};
pub use word::{line_text, needs_space};
