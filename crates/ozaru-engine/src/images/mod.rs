//! Images: the image XObjects and inline images of a page, what they are and
//! their data (ISO 32000-2, section 8.9).

mod description;

pub use description::{ImageData, ImageDescription, read_image};
