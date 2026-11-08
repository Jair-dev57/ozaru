//! Fonts: how the character codes of a content stream map to Unicode and to
//! glyph widths (ISO 32000-2, section 9).

mod encoding;

pub use encoding::{BaseEncoding, Encoding, read_encoding};
