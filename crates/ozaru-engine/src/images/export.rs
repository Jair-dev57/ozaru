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
/// without decoding them. Images whose data are pixels become PNG files when
/// they are in `/DeviceGray` or `/DeviceRGB`, with 8 bits for each component
/// and the usual `/Decode`, and their data isn't much shorter than their
/// size says. Other images are `None` for now.
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
    if description.image_mask || description.bits_per_component != 8 {
        return None;
    }
    let color = match description.color_space.as_ref()?.as_name()?.as_bytes() {
        b"DeviceGray" => PngColor::Gray,
        b"DeviceRGB" => PngColor::Rgb,
        _ => return None,
    };
    let usual_decode = description
        .decode
        .as_ref()
        .is_none_or(|decode| decode.chunks(2).all(|pair| pair == [0.0, 1.0]));
    if !usual_decode {
        return None;
    }
    // A damaged image may say it is far larger than its data: its missing
    // pixels are made black, but only up to twice the data and a megabyte
    // more, so that a few bytes can't make a file of gigabytes.
    let samples = match color {
        PngColor::Gray | PngColor::GrayAlpha => 1,
        PngColor::Rgb | PngColor::Rgba => 3,
    };
    let expected = description.width as usize * description.height as usize * samples;
    if expected > 2 * image.data.len() + (1 << 20) {
        return None;
    }
    let data = write_png(description.width, description.height, color, &image.data);
    Some(ImageFile {
        format: ImageFormat::Png,
        data,
    })
}
