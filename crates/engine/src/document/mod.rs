//! The document structure: the catalog, the page tree and document-wide data
//! (ISO 32000-2, section 7.7).

mod annotations;
mod catalog;
mod date;
mod forms;
mod info;
mod navigation;
mod open;
mod pages;
mod resources;
mod text;
mod xmp;

pub use annotations::{Annotation, Link, read_annotations};
pub use catalog::{Catalog, read_catalog};
pub use date::{Date, parse_date};
pub use forms::{FieldKind, FieldValue, FormField, Widget, read_form_fields};
pub use info::{Info, Trapped, read_info};
pub use navigation::{
    Attachment, Bookmark, NamedDestination, read_attachments, read_named_destinations,
    read_outlines, read_page_labels,
};
pub use open::{Document, OpenOptions};
pub use pages::{Page, Rectangle, read_pages, rebuild_pages};
pub use resources::Resources;
pub use text::decode_text_string;
pub use xmp::{Xmp, parse_xmp_date, read_xmp};
