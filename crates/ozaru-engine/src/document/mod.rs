//! The document structure: the catalog, the page tree and document-wide data
//! (ISO 32000-2, section 7.7).

mod catalog;
mod pages;

pub use catalog::{Catalog, read_catalog};
pub use pages::{Page, read_pages};
