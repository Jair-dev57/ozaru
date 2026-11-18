use std::sync::Arc;

use super::raster::{IRect, Mask};
use crate::images::{PngColor, write_png};

/// An image being drawn: 8 bits for red, green, blue and alpha, with the
/// colors multiplied by the alpha, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixmap {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Pixmap {
    /// A pixmap of `width` by `height` pixels, all transparent. `None` if it
    /// would take more than `max_bytes` bytes.
    pub fn new(width: usize, height: usize, max_bytes: usize) -> Option<Self> {
        let size = width.checked_mul(height)?.checked_mul(4)?;
        if size > max_bytes {
            return None;
        }
        Some(Self {
            width,
            height,
            data: vec![0; size],
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The whole pixmap as a rectangle.
    pub fn rect(&self) -> IRect {
        IRect::new(0, 0, self.width as i32, self.height as i32)
    }

    /// The bytes of the pixels: premultiplied RGBA, rows from the top.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The pixel at `x` and `y` as red, green, blue and alpha (premultiplied),
    /// or `None` outside the pixmap.
    pub fn pixel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y * self.width + x) * 4;
        self.data[at..at + 4].try_into().ok()
    }

    /// Paints every pixel with `color`, and alpha 255.
    pub fn clear(&mut self, color: [u8; 3]) {
        for pixel in self.data.as_chunks_mut::<4>().0 {
            *pixel = [color[0], color[1], color[2], 255];
        }
    }

    /// Paints `color` with `opacity` (0 to 255) where `mask` covers, inside
    /// `clip`, over what is there.
    pub fn fill(&mut self, mask: &Mask, clip: &Clip, color: [u8; 3], opacity: u8) {
        let rect = mask.rect.intersect(&clip.rect).intersect(&self.rect());
        if rect.is_empty() || opacity == 0 {
            return;
        }
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let coverage = u32::from(mask.at(x, y));
                if coverage == 0 {
                    continue;
                }
                let coverage = coverage * u32::from(clip.at(x, y)) / 255;
                let alpha = (coverage * u32::from(opacity) + 127) / 255;
                if alpha == 0 {
                    continue;
                }
                let at = (y as usize * self.width + x as usize) * 4;
                let pixel = &mut self.data[at..at + 4];
                let inverse = 255 - alpha;
                for c in 0..3 {
                    pixel[c] = ((u32::from(color[c]) * alpha + u32::from(pixel[c]) * inverse + 127)
                        / 255) as u8;
                }
                pixel[3] = (alpha + (u32::from(pixel[3]) * inverse + 127) / 255) as u8;
            }
        }
    }

    /// The pixmap as a PNG file, over `background` where it is transparent.
    pub fn to_png(&self, background: [u8; 3]) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.width * self.height * 3);
        for pixel in self.data.as_chunks::<4>().0 {
            let inverse = 255 - u32::from(pixel[3]);
            for c in 0..3 {
                let value = u32::from(pixel[c]) + (u32::from(background[c]) * inverse + 127) / 255;
                rgb.push(value.min(255) as u8);
            }
        }
        write_png(self.width as u32, self.height as u32, PngColor::Rgb, &rgb)
    }
}

/// The area drawing is limited to (ISO 32000-2, section 8.5.4): a rectangle
/// and, once a clipping path has cut it, the coverage of each pixel in it.
/// It is cheap to copy.
#[derive(Debug, Clone)]
pub struct Clip {
    pub rect: IRect,
    mask: Option<Arc<Mask>>,
}

impl Clip {
    /// No limit but `rect`.
    pub fn new(rect: IRect) -> Self {
        Self { rect, mask: None }
    }

    /// How much of the pixel at `x` and `y` is inside the clip, from 0 to 255.
    pub fn at(&self, x: i32, y: i32) -> u8 {
        if x < self.rect.x0 || x >= self.rect.x1 || y < self.rect.y0 || y >= self.rect.y1 {
            return 0;
        }
        match &self.mask {
            Some(mask) => mask.at(x, y),
            None => 255,
        }
    }

    /// The clip cut again by a shape: where both are inside. `None` for a
    /// shape that covers nothing, which clips everything.
    pub fn intersect(&self, shape: Option<&Mask>) -> Self {
        let Some(shape) = shape else {
            return Self::new(IRect::new(
                self.rect.x0,
                self.rect.y0,
                self.rect.x0,
                self.rect.y0,
            ));
        };
        let rect = self.rect.intersect(&shape.rect);
        if rect.is_empty() {
            return Self::new(IRect::new(rect.x0, rect.y0, rect.x0, rect.y0));
        }
        let mut data = Vec::with_capacity(rect.width() * rect.height());
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let both = u32::from(shape.at(x, y)) * u32::from(self.at(x, y));
                data.push(((both + 127) / 255) as u8);
            }
        }
        Self {
            rect,
            mask: Some(Arc::new(Mask { rect, data })),
        }
    }
}
