//! Text: the characters of a page, put together into words and lines
//! (ISO 32000-2, section 9.10).

mod extract;
mod glyph;
mod line;
mod word;

pub use extract::{
    PageBlocks, PageProblem, PageText, PageWords, TextBlock, document_blocks, document_text,
    document_words, page_blocks, page_text, page_words,
};
pub use glyph::{TextChar, characters};
pub use line::{TextLine, lines};
pub use word::{TextWord, line_text, line_words, needs_space};
