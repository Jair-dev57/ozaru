//! The document structure: the catalog, the page tree and document-wide data
//! (ISO 32000-2, section 7.7).

mod catalog;
mod date;
mod info;
mod open;
mod pages;
mod resources;
mod text;
mod xmp;

pub use catalog::{Catalog, read_catalog};
pub use date::{Date, parse_date};
pub use info::{Info, Trapped, read_info};
pub use open::{Document, OpenOptions};
pub use pages::{Page, Rectangle, read_pages, rebuild_pages};
pub use resources::Resources;
pub use text::decode_text_string;
pub use xmp::{Xmp, parse_xmp_date, read_xmp};
