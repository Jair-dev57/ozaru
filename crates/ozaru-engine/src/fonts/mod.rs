//! Fonts: how the character codes of a content stream map to Unicode and to
//! glyph widths (ISO 32000-2, section 9).

mod cmap;
mod encoding;
mod glyphlist;
mod tounicode;

pub use cmap::{CMapData, Code, Destination, RangeDestination, parse_cmap};
pub use encoding::{BaseEncoding, Encoding, read_encoding};
pub use glyphlist::{GlyphList, glyph_to_unicode};
pub use tounicode::{ToUnicode, read_to_unicode};
