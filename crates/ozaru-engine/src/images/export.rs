use super::color;
use super::description::ImageData;
use super::png::{PngColor, write_png};
use crate::filters::ImageFilter;

/// The format of an image file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// JPEG, as the PDF has it, from `/DCTDecode`.
    Jpeg,
    /// JPEG 2000 as a codestream or a JP2 file, as the PDF has it, from
    /// `/JPXDecode`.
    Jpeg2000,
    /// PNG, written from the pixels of the image.
    Png,
}

impl ImageFormat {
    /// The usual extension of its files, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Jpeg2000 => "jp2",
            Self::Png => "png",
        }
    }
}

/// An image as a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    pub format: ImageFormat,
    pub data: Vec<u8>,
}

/// The image `image` as a file, if its format and colors are supported.
///
/// JPEG and JPEG 2000 images are given as the PDF has them, byte for byte,
/// without decoding them, and without their mask. Images whose data are
/// pixels become PNG files in gray or RGB, as [`color::pixels`] makes them,
/// with an alpha channel if they have a mask, and if their data isn't much
/// shorter than their size says. Images in color spaces whose colors need
/// functions, such as `/Separation`, and CCITT and JBIG2 images, are `None`
/// for now.
pub fn image_file(image: &ImageData) -> Option<ImageFile> {
    if let Some(encoding) = &image.encoding {
        let format = match encoding.filter {
            ImageFilter::Dct => ImageFormat::Jpeg,
            ImageFilter::Jpx => ImageFormat::Jpeg2000,
            ImageFilter::CcittFax | ImageFilter::Jbig2 => return None,
        };
        return Some(ImageFile {
            format,
            data: image.data.clone(),
        });
    }
    let description = &image.description;
    let (color, samples) = color::pixels(description, image.colors.as_ref(), &image.data)?;
    let (color, samples) = match &image.opacity {
        Some(opacity) => {
            let channels = if color == PngColor::Gray { 1 } else { 3 };
            let mut with_alpha = Vec::with_capacity(samples.len() / channels * (channels + 1));
            for (pixel, alpha) in samples.chunks(channels).zip(opacity) {
                with_alpha.extend_from_slice(pixel);
                with_alpha.push(*alpha);
            }
            let color = if color == PngColor::Gray {
                PngColor::GrayAlpha
            } else {
                PngColor::Rgba
            };
            (color, with_alpha)
        }
        None => (color, samples),
    };
    let data = write_png(description.width, description.height, color, &samples);
    Some(ImageFile {
        format: ImageFormat::Png,
        data,
    })
}
