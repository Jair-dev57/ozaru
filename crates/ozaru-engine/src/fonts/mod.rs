//! Fonts: how the character codes of a content stream map to Unicode and to
//! glyph widths (ISO 32000-2, section 9).

mod cmap;
mod composite;
mod embedded;
mod encoding;
mod font;
mod glyphlist;
mod metrics;
mod simple;
mod tounicode;

pub use cmap::{CMapData, Code, Destination, RangeDestination, parse_cmap};
pub use composite::{CidCMap, CidFontKind, CompositeFont, VerticalMetrics, read_composite_font};
pub use encoding::{BaseEncoding, Encoding, read_encoding};
pub use font::Font;
pub use glyphlist::{GlyphList, glyph_to_unicode};
pub use metrics::StandardFont;
pub use simple::{SimpleFont, SimpleFontKind, read_simple_font};
pub use tounicode::{ToUnicode, read_to_unicode};

use std::sync::Arc;

use crate::syntax::Object;
use crate::xref::Resolver;

// `object`, resolved if it is a reference. Null if it can't be.
fn resolved(resolver: &mut Resolver<'_>, object: &Object) -> Arc<Object> {
    match object {
        Object::Reference(id) => resolver
            .resolve(*id)
            .unwrap_or_else(|_| Arc::new(Object::Null)),
        object => Arc::new(object.clone()),
    }
}
