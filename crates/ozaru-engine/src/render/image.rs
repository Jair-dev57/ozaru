use super::blend::BlendMode;
use super::canvas::{Clip, Pixmap};
use super::path::Contour;
use super::raster::{FillRule, Mask, fill_contours, rect_mask};
use crate::base::Limits;
use crate::content::Matrix;
use crate::filters::{ImageFilter, decode_jpeg_scaled};
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
    /// Whether it is a stencil mask, painted with the fill color.
    pub stencil: bool,
    /// Whether it is smoothed when drawn larger than it is (`/Interpolate`).
    pub interpolate: bool,
}

/// How many eighths of its size a JPEG image is decoded at to be drawn
/// `target` pixels wide and high: the smallest of 1, 2, 4 and 8 that leaves
/// it no smaller than that.
pub fn jpeg_scale(width: usize, height: usize, target: (f64, f64)) -> usize {
    [1usize, 2, 4]
        .into_iter()
        .find(|&n| {
            (width * n).div_ceil(8) as f64 >= target.0
                && (height * n).div_ceil(8) as f64 >= target.1
        })
        .unwrap_or(8)
}

/// The pixels of `image` ready to draw, `None` if they can't be read:
/// JPEG 2000 and color spaces ozaru can't read. A JPEG image without a mask
/// is decoded at `scale` eighths of its size (see [`jpeg_scale`]), which is
/// much faster for one drawn small; any other image comes whole.
pub fn prepare(image: &ImageData, limits: Limits, scale: usize) -> Option<Bitmap> {
    let description = &image.description;
    let (mut width, mut height) = (description.width as usize, description.height as usize);
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
        let scale = if image.opacity.is_some() || description.image_mask {
            8
        } else {
            scale
        };
        let jpeg = decode_jpeg_scaled(&image.data, transform, 0, limits, scale).ok()?;
        if (jpeg.width, jpeg.height) != ((width * scale).div_ceil(8), (height * scale).div_ceil(8))
        {
            return None;
        }
        samples = jpeg.data;
        description.bits_per_component = 8;
        description.width = jpeg.width as u32;
        description.height = jpeg.height as u32;
        width = jpeg.width;
        height = jpeg.height;
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
        stencil: description.image_mask,
        interpolate: description.interpolate,
    })
}

/// Makes a bitmap `fx` times narrower and `fy` times shorter, each pixel the
/// average of the ones it covers, weighted by their opacity so that the colors
/// of transparent pixels don't leak.
fn reduce(bitmap: &Bitmap, fx: usize, fy: usize) -> Bitmap {
    let (width, height) = (bitmap.width.div_ceil(fx), bitmap.height.div_ceil(fy));
    let mut rgb = vec![0u8; width * height * 3];
    let mut alpha = bitmap.alpha.as_ref().map(|_| vec![0u8; width * height]);
    // The sums of one output row, filled from `fy` source rows.
    let mut sums = vec![0u32; width * 3];
    let mut weights = vec![0u32; width];
    let mut counts = vec![0u32; width];
    for y in 0..height {
        sums.fill(0);
        weights.fill(0);
        counts.fill(0);
        for sy in y * fy..((y + 1) * fy).min(bitmap.height) {
            let row = sy * bitmap.width;
            match &bitmap.alpha {
                None => {
                    let pixels = &bitmap.rgb[row * 3..(row + bitmap.width) * 3];
                    for (sx, pixel) in pixels.as_chunks::<3>().0.iter().enumerate() {
                        let x = sx / fx;
                        sums[x * 3] += u32::from(pixel[0]);
                        sums[x * 3 + 1] += u32::from(pixel[1]);
                        sums[x * 3 + 2] += u32::from(pixel[2]);
                        counts[x] += 1;
                    }
                }
                Some(opacity) => {
                    for sx in 0..bitmap.width {
                        let (x, i) = (sx / fx, row + sx);
                        let weight = u32::from(opacity[i]);
                        sums[x * 3] += u32::from(bitmap.rgb[i * 3]) * weight;
                        sums[x * 3 + 1] += u32::from(bitmap.rgb[i * 3 + 1]) * weight;
                        sums[x * 3 + 2] += u32::from(bitmap.rgb[i * 3 + 2]) * weight;
                        weights[x] += weight;
                        counts[x] += 1;
                    }
                }
            }
        }
        for x in 0..width {
            let at = y * width + x;
            let divisor = if bitmap.alpha.is_some() {
                weights[x]
            } else {
                counts[x]
            };
            for c in 0..3 {
                rgb[at * 3 + c] = sums[x * 3 + c].checked_div(divisor).unwrap_or(0) as u8;
            }
            if let Some(alpha) = &mut alpha {
                alpha[at] = (weights[x] / counts[x].max(1)) as u8;
            }
        }
    }
    Bitmap {
        width,
        height,
        rgb,
        alpha,
        stencil: bitmap.stencil,
        interpolate: bitmap.interpolate,
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
    mode: BlendMode,
) {
    let det = matrix.a * matrix.d - matrix.b * matrix.c;
    if det.abs() < 1e-9 || !matrix.is_finite() {
        return;
    }
    let limit = clip.rect.intersect(&pixmap.rect());
    // Without turning or skewing, the image is a rectangle along the axes,
    // and its coverage and source positions follow from the column and the row.
    let straight = matrix.b.abs() < 1e-9 && matrix.c.abs() < 1e-9;
    let shape = if straight {
        let (x0, x1) = (
            matrix.e.min(matrix.e + matrix.a),
            matrix.e.max(matrix.e + matrix.a),
        );
        let (y0, y1) = (
            matrix.f.min(matrix.f + matrix.d),
            matrix.f.max(matrix.f + matrix.d),
        );
        rect_mask(x0, y0, x1, y1, &limit)
    } else {
        let quad = Contour {
            points: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                .iter()
                .map(|&(x, y)| matrix.apply(x, y))
                .collect(),
            closed: true,
        };
        fill_contours(&[quad], FillRule::NonZero, &limit)
    };
    let Some(shape) = shape else {
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
    let smooth = bitmap.interpolate
        || dest_w < source.width as f64 * 1.0
        || dest_h < source.height as f64 * 1.0;
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
    // Where each column and row falls in the image, when it is straight.
    let columns: Vec<f64> = (rect.x0..rect.x1)
        .map(|x| ((f64::from(x) + 0.5 - matrix.e) / matrix.a).clamp(0.0, 1.0) * w)
        .collect();
    let rows: Vec<f64> = (rect.y0..rect.y1)
        .map(|y| (1.0 - ((f64::from(y) + 0.5 - matrix.f) / matrix.d).clamp(0.0, 1.0)) * h)
        .collect();
    if straight {
        draw_straight(
            pixmap,
            clip,
            source,
            &shape,
            (&columns, &rows),
            smooth,
            (opacity, stencil, mode),
        );
        return;
    }
    for y in rect.y0..rect.y1 {
        let shape_row = &shape.data[(y - rect.y0) as usize * rect.width()..][..rect.width()];
        for x in rect.x0..rect.x1 {
            let coverage = u32::from(shape_row[(x - rect.x0) as usize]);
            if coverage == 0 {
                continue;
            }
            let (sx, sy) = if straight {
                (
                    columns[(x - rect.x0) as usize],
                    rows[(y - rect.y0) as usize],
                )
            } else {
                let (u, v) = inverse.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
                (u.clamp(0.0, 1.0) * w, (1.0 - v.clamp(0.0, 1.0)) * h)
            };
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
            pixmap.blend_with(x, y, rgb, total as u8, clip, mode);
        }
    }
}

// Draws an image that is a rectangle along the axes: `columns` and `rows`
// are where each column and row of `shape` falls in the bitmap. Mixing the
// four pixels around a point is done in whole numbers.
fn draw_straight(
    pixmap: &mut Pixmap,
    clip: &Clip,
    bitmap: &Bitmap,
    shape: &Mask,
    (columns, rows): (&[f64], &[f64]),
    smooth: bool,
    (opacity, stencil, mode): (u8, Option<[u8; 3]>, BlendMode),
) {
    let rect = shape.rect;
    let (w, h) = (bitmap.width, bitmap.height);
    // Each column's two source columns and the weight of the second, in 256ths.
    let axis = |positions: &[f64], size: usize| -> Vec<(usize, usize, u32)> {
        positions
            .iter()
            .map(|&p| {
                if smooth {
                    let p = (p - 0.5).max(0.0);
                    let i = (p.floor() as usize).min(size - 1);
                    ((i), (i + 1).min(size - 1), ((p - i as f64) * 256.0) as u32)
                } else {
                    let i = (p as usize).min(size - 1);
                    (i, i, 0)
                }
            })
            .collect()
    };
    let (xs, ys) = (axis(columns, w), axis(rows, h));
    let mix = |a: u8, b: u8, t: u32| ((u32::from(a) * (256 - t) + u32::from(b) * t) >> 8) as u8;
    for y in rect.y0..rect.y1 {
        let shape_row = &shape.data[(y - rect.y0) as usize * rect.width()..][..rect.width()];
        let (y0, y1, wy) = ys[(y - rect.y0) as usize];
        for x in rect.x0..rect.x1 {
            let coverage = u32::from(shape_row[(x - rect.x0) as usize]);
            if coverage == 0 {
                continue;
            }
            let (x0, x1, wx) = xs[(x - rect.x0) as usize];
            let (i00, i10, i01, i11) = (y0 * w + x0, y0 * w + x1, y1 * w + x0, y1 * w + x1);
            let mut color = [0u8; 3];
            for (c, out) in color.iter_mut().enumerate() {
                let top = mix(bitmap.rgb[i00 * 3 + c], bitmap.rgb[i10 * 3 + c], wx);
                let bottom = mix(bitmap.rgb[i01 * 3 + c], bitmap.rgb[i11 * 3 + c], wx);
                *out = mix(top, bottom, wy);
            }
            let a = match &bitmap.alpha {
                None => 255,
                Some(alpha) => {
                    let top = mix(alpha[i00], alpha[i10], wx);
                    let bottom = mix(alpha[i01], alpha[i11], wx);
                    mix(top, bottom, wy)
                }
            };
            let color = stencil.unwrap_or(color);
            let total = coverage * u32::from(a) / 255 * u32::from(opacity) / 255;
            pixmap.blend_with(x, y, color, total as u8, clip, mode);
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
