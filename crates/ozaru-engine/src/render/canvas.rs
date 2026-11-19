use std::sync::Arc;

use super::blend::BlendMode;
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
        self.fill_with(mask, clip, color, opacity, BlendMode::Normal);
    }

    /// Like [`fill`](Self::fill), mixing the color with what is there as
    /// `mode` says.
    pub fn fill_with(
        &mut self,
        mask: &Mask,
        clip: &Clip,
        color: [u8; 3],
        opacity: u8,
        mode: BlendMode,
    ) {
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
                self.mix(x, y, color, alpha, mode);
            }
        }
    }

    /// Paints `color` at one pixel with `alpha` (0 to 255), inside `clip`.
    pub fn blend(&mut self, x: i32, y: i32, color: [u8; 3], alpha: u8, clip: &Clip) {
        self.blend_with(x, y, color, alpha, clip, BlendMode::Normal);
    }

    pub fn blend_with(
        &mut self,
        x: i32,
        y: i32,
        color: [u8; 3],
        alpha: u8,
        clip: &Clip,
        mode: BlendMode,
    ) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let alpha = u32::from(alpha) * u32::from(clip.at(x, y)) / 255;
        self.mix(x, y, color, alpha, mode);
    }

    // The pixel at `x` and `y` mixed with `color` of `alpha` (0 to 255).
    fn mix(&mut self, x: i32, y: i32, color: [u8; 3], alpha: u32, mode: BlendMode) {
        if alpha == 0 {
            return;
        }
        let at = (y as usize * self.width + x as usize) * 4;
        let pixel = &mut self.data[at..at + 4];
        let inverse = 255 - alpha;
        if mode == BlendMode::Normal || pixel[3] == 0 {
            for c in 0..3 {
                pixel[c] = ((u32::from(color[c]) * alpha + u32::from(pixel[c]) * inverse + 127)
                    / 255) as u8;
            }
            pixel[3] = (alpha + (u32::from(pixel[3]) * inverse + 127) / 255) as u8;
            return;
        }
        // The backdrop's color without its alpha, then the formula of
        // section 11.3.6: the source where there is no backdrop, the blend
        // where there is, and the backdrop where there is no source.
        let (ab, a_s) = (f32::from(pixel[3]) / 255.0, alpha as f32 / 255.0);
        let backdrop = [0, 1, 2].map(|c| (f32::from(pixel[c]) / f32::from(pixel[3])).min(1.0));
        let source = color.map(|c| f32::from(c) / 255.0);
        let blended = mode.blend(backdrop, source);
        for c in 0..3 {
            let premultiplied = a_s * (1.0 - ab) * source[c]
                + a_s * ab * blended[c]
                + (1.0 - a_s) * f32::from(pixel[c]) / 255.0;
            pixel[c] = (premultiplied * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
        pixel[3] = ((a_s + ab * (1.0 - a_s)) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }

    /// Puts a group, drawn on a transparent pixmap of the same size, over
    /// this one, with `opacity` (0 to 255) inside `clip`, mixed as `mode`
    /// says.
    pub fn composite(&mut self, group: &Pixmap, clip: &Clip, opacity: u8, mode: BlendMode) {
        if (group.width, group.height) != (self.width, self.height) {
            return;
        }
        let rect = clip.rect.intersect(&self.rect());
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let at = (y as usize * self.width + x as usize) * 4;
                let source = &group.data[at..at + 4];
                if source[3] == 0 {
                    continue;
                }
                let k = u32::from(clip.at(x, y)) * u32::from(opacity) / 255;
                if k == 0 {
                    continue;
                }
                // The group's color without its alpha, and its alpha scaled.
                let sa = u32::from(source[3]);
                let color = [0, 1, 2].map(|c| (u32::from(source[c]) * 255 / sa).min(255) as u8);
                self.mix(x, y, color, sa * k / 255, mode);
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
