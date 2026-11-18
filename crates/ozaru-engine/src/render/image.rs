use super::canvas::{Clip, Pixmap};
use super::path::Contour;
use super::raster::{FillRule, fill_contours};
use crate::base::Limits;
use crate::content::Matrix;
use crate::filters::{ImageFilter, decode_jpeg};
use crate::images::{ImageData, PngColor, pixels};

/// An image ready to draw: its pixels as RGB and its opacity, each pixel of
/// the rows from the top.
#[derive(Debug, Clone)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    /// Red, green and blue of each pixel, or for a stencil mask the color it
    /// paints, which is left to the caller.
    pub rgb: Vec<u8>,
    /// How opaque each pixel is. `None` if all are.
    pub alpha: Option<Vec<u8>>,
}

/// The pixels of `image` ready to draw, `None` if they can't be read:
/// JPEG 2000 and color spaces ozaru can't read.
pub fn prepare(image: &ImageData, limits: Limits) -> Option<Bitmap> {
    let description = &image.description;
    let (width, height) = (description.width as usize, description.height as usize);
    let mut samples = image.data.clone();
    let mut description = description.clone();
    if let Some(encoding) = &image.encoding {
        if encoding.filter != ImageFilter::Dct {
            return None;
        }
        let transform = encoding
            .parameters
            .as_ref()
            .and_then(|p| p.get("ColorTransform"))
            .and_then(|t| t.as_integer())
            .map(|t| t != 0);
        let jpeg = decode_jpeg(&image.data, transform, 0, limits).ok()?;
        if (jpeg.width, jpeg.height) != (width, height) {
            return None;
        }
        samples = jpeg.data;
        description.bits_per_component = 8;
    }
    let (color, data) = pixels(&description, image.colors.as_ref(), &samples)?;
    let count = width.checked_mul(height)?;
    let mut alpha = image.opacity.clone().filter(|a| a.len() == count);
    let rgb = match color {
        PngColor::Gray if description.image_mask => {
            // Black where it paints: the painted pixels are opaque.
            let painted: Vec<u8> = data.iter().map(|&g| 255 - g).collect();
            alpha = Some(painted);
            vec![0; count * 3]
        }
        PngColor::Gray => data.iter().flat_map(|&g| [g, g, g]).collect(),
        PngColor::Rgb => data,
        _ => return None,
    };
    if rgb.len() != count * 3 {
        return None;
    }
    Some(Bitmap {
        width,
        height,
        rgb,
        alpha,
    })
}

/// Halves a bitmap `factor` times in each direction by averaging the pixels
/// it covers, until the pixels aren't much smaller than the ones they are
/// drawn with.
fn reduce(bitmap: &Bitmap, fx: usize, fy: usize) -> Bitmap {
    let (width, height) = (bitmap.width.div_ceil(fx), bitmap.height.div_ceil(fy));
    let mut rgb = vec![0u8; width * height * 3];
    let mut alpha = bitmap.alpha.as_ref().map(|_| vec![0u8; width * height]);
    for y in 0..height {
        for x in 0..width {
            let (mut sum, mut a_sum, mut n) = ([0u32; 3], 0u32, 0u32);
            for sy in y * fy..((y + 1) * fy).min(bitmap.height) {
                for sx in x * fx..((x + 1) * fx).min(bitmap.width) {
                    let i = sy * bitmap.width + sx;
                    // Weighted by opacity, so that transparent pixels' colors don't leak.
                    let weight = bitmap.alpha.as_ref().map_or(255, |a| u32::from(a[i]));
                    for (c, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(bitmap.rgb[i * 3 + c]) * weight;
                    }
                    a_sum += weight;
                    n += 1;
                }
            }
            let at = y * width + x;
            for (c, total) in sum.iter().enumerate() {
                rgb[at * 3 + c] = total.checked_div(a_sum).unwrap_or(0) as u8;
            }
            if let Some(alpha) = &mut alpha {
                alpha[at] = (a_sum / n.max(1)) as u8;
            }
        }
    }
    Bitmap {
        width,
        height,
        rgb,
        alpha,
    }
}

/// Draws `bitmap` into the unit square that `matrix` maps to the pixmap, with
/// `opacity` (0 to 255). A stencil mask paints `color` instead of its own.
pub fn draw_bitmap(
    pixmap: &mut Pixmap,
    clip: &Clip,
    bitmap: &Bitmap,
    matrix: &Matrix,
    opacity: u8,
    stencil: Option<[u8; 3]>,
) {
    let det = matrix.a * matrix.d - matrix.b * matrix.c;
    if det.abs() < 1e-9 || !matrix.is_finite() {
        return;
    }
    let quad = Contour {
        points: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .iter()
            .map(|&(x, y)| matrix.apply(x, y))
            .collect(),
        closed: true,
    };
    let Some(shape) = fill_contours(
        &[quad],
        FillRule::NonZero,
        &clip.rect.intersect(&pixmap.rect()),
    ) else {
        return;
    };
    // The size of the image on the page, in pixels, tells how much to reduce.
    let (dest_w, dest_h) = (matrix.a.hypot(matrix.b), matrix.c.hypot(matrix.d));
    let fx = ((bitmap.width as f64 / dest_w.max(1.0)).floor() as usize).max(1);
    let fy = ((bitmap.height as f64 / dest_h.max(1.0)).floor() as usize).max(1);
    let reduced;
    let source = if fx > 1 || fy > 1 {
        reduced = reduce(bitmap, fx, fy);
        &reduced
    } else {
        bitmap
    };
    // Bigger on the page than in the image: pixels are repeated, as readers
    // do; smaller: they are mixed.
    let smooth = dest_w < source.width as f64 * 1.0 || dest_h < source.height as f64 * 1.0;
    // From device to the unit square.
    let inverse = Matrix::new(
        matrix.d / det,
        -matrix.b / det,
        -matrix.c / det,
        matrix.a / det,
        (matrix.c * matrix.f - matrix.d * matrix.e) / det,
        (matrix.b * matrix.e - matrix.a * matrix.f) / det,
    );
    let (w, h) = (source.width as f64, source.height as f64);
    let rect = shape.rect;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let coverage = u32::from(shape.at(x, y));
            if coverage == 0 {
                continue;
            }
            let (u, v) = inverse.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let (sx, sy) = (u.clamp(0.0, 1.0) * w, (1.0 - v.clamp(0.0, 1.0)) * h);
            let (rgb, a) = if smooth {
                sample_bilinear(source, sx - 0.5, sy - 0.5)
            } else {
                let (ix, iy) = (
                    (sx as usize).min(source.width - 1),
                    (sy as usize).min(source.height - 1),
                );
                sample(source, ix, iy)
            };
            let rgb = stencil.unwrap_or(rgb);
            let total = coverage * u32::from(a) / 255 * u32::from(opacity) / 255;
            pixmap.blend(x, y, rgb, total as u8, clip);
        }
    }
}

fn sample(bitmap: &Bitmap, x: usize, y: usize) -> ([u8; 3], u8) {
    let i = y * bitmap.width + x;
    (
        [
            bitmap.rgb[i * 3],
            bitmap.rgb[i * 3 + 1],
            bitmap.rgb[i * 3 + 2],
        ],
        bitmap.alpha.as_ref().map_or(255, |a| a[i]),
    )
}

fn sample_bilinear(bitmap: &Bitmap, x: f64, y: f64) -> ([u8; 3], u8) {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |dx: f64, dy: f64| {
        let ix = ((x0 + dx).max(0.0) as usize).min(bitmap.width - 1);
        let iy = ((y0 + dy).max(0.0) as usize).min(bitmap.height - 1);
        sample(bitmap, ix, iy)
    };
    let corners = [
        (at(0.0, 0.0), (1.0 - fx) * (1.0 - fy)),
        (at(1.0, 0.0), fx * (1.0 - fy)),
        (at(0.0, 1.0), (1.0 - fx) * fy),
        (at(1.0, 1.0), fx * fy),
    ];
    let mut rgb = [0.0; 3];
    let mut weight_total = 0.0;
    let mut alpha = 0.0;
    for ((color, a), weight) in corners {
        let w = weight * f64::from(a);
        for c in 0..3 {
            rgb[c] += f64::from(color[c]) * w;
        }
        weight_total += w;
        alpha += weight * f64::from(a);
    }
    let rgb = if weight_total > 0.0 {
        [0, 1, 2].map(|c| (rgb[c] / weight_total).round() as u8)
    } else {
        [0, 0, 0]
    };
    (rgb, alpha.round() as u8)
}
