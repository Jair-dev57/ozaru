//! Cross-reference data: where each object of a file is (ISO 32000-2, section
//! 7.5).

mod chain;
mod objstm;
mod startxref;
mod stream;
mod table;

pub use chain::{LoadedXref, load_xref, parse_xref_section};
pub use objstm::{CompressedObject, ObjectStream, ParsedObjectStream, parse_object_stream};
pub use startxref::{Startxref, find_startxref};
pub use stream::parse_xref_stream;
pub use table::{ParsedXrefTable, XrefEntry, XrefTable, parse_xref_table};

use crate::syntax::chars::is_whitespace;

fn skip_whitespace(input: &[u8], mut position: usize) -> usize {
    while input.get(position).is_some_and(|&byte| is_whitespace(byte)) {
        position += 1;
    }
    position
}
