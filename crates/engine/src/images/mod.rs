//! Images: the image XObjects and inline images of a page, what they are and
//! their data (ISO 32000-2, section 8.9), and the files they make.

mod color;
mod description;
mod document;
mod export;
mod png;
mod png_read;

pub use color::{ColorSpace, color_key_opacity, pixels, read_color_space, resampled};
pub use description::{ImageData, ImageDescription, read_image};
pub use document::{PageImage, PageImages, document_images};
pub use export::{ImageFile, ImageFormat, image_file};
pub use png::{PngColor, write_png, write_png_level};
pub use png_read::{DecodedPng, read_png};
