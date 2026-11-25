//! Text: the characters of a page, put together into words and lines
//! (ISO 32000-2, section 9.10).

mod analysis;
mod bidi;
mod columns;
mod extract;
mod glyph;
mod line;
mod markdown;
mod tables;
mod word;

pub use analysis::{TextMatch, TextSpan, is_bold_name, is_italic_name, page_search, page_spans};
pub use bidi::{BidiClass, bidi_class, bidi_levels, paragraph_level};
pub use columns::read_columns;
pub use extract::{
    PageBlocks, PageMarkdown, PageProblem, PageTables, PageText, PageWords, TextBlock,
    document_blocks, document_blocks_colored, document_markdown, document_tables, document_text,
    document_words, page_blocks, page_text, page_words,
};
pub use glyph::{TextChar, characters};
pub use line::{TextLine, lines};
pub use markdown::{MarkdownStyle, page_markdown, table_markdown};
pub use tables::{Table, TableCell, TableStrategy, page_tables};
pub use word::{TextWord, line_text, line_words, needs_space};
