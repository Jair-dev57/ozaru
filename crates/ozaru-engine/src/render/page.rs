use std::collections::HashMap;
use std::sync::Arc;

use super::canvas::{Clip, Pixmap};
use super::image::{draw_bitmap, prepare};
use super::path::{Path, flatten};
use super::raster::{FillRule, fill_contours};
use super::stroke::{StrokeStyle, stroke_contours};
use crate::base::{Error, ErrorKind, Limit, Limits, Result};
use crate::content::{ClipNode, Interpreter, Matrix, PageContent, PaintItem, PaintState};
use crate::document::Page;
use crate::images::{ImageData, read_image};
use crate::syntax::ObjectId;
use crate::xref::Resolver;

/// How a page is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    /// Pixels for each point of the page: 1 is 72 dots per inch.
    pub scale: f64,
    /// The color of the paper, as red, green and blue.
    pub background: [u8; 3],
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            background: [255, 255, 255],
        }
    }
}

/// The matrix from the default user space of `page` to pixels at `scale`: the
/// crop box starts at the top left corner, and the page is turned by its
/// `/Rotate`. Also the size of the pixmap, in pixels.
pub fn page_matrix(page: &Page, scale: f64) -> (Matrix, usize, usize) {
    let crop = page.crop_box();
    let (width, height) = (crop.width() * scale, crop.height() * scale);
    let s = scale;
    let (matrix, w, h) = match page.rotation() % 360 {
        90 => (
            Matrix::new(0.0, s, s, 0.0, -crop.bottom * s, -crop.left * s),
            height,
            width,
        ),
        180 => (
            Matrix::new(-s, 0.0, 0.0, s, crop.right * s, -crop.bottom * s),
            width,
            height,
        ),
        270 => (
            Matrix::new(0.0, -s, -s, 0.0, crop.top * s, crop.right * s),
            height,
            width,
        ),
        _ => (
            Matrix::new(s, 0.0, 0.0, -s, -crop.left * s, crop.top * s),
            width,
            height,
        ),
    };
    (
        matrix,
        w.ceil().max(1.0) as usize,
        h.ceil().max(1.0) as usize,
    )
}

/// Draws `page`. It is an error if the page would take more than
/// `limits.max_decoded_length` bytes as pixels. What can't be drawn, such as
/// JPEG 2000 images, is left out.
pub fn render_page(
    resolver: &mut Resolver<'_>,
    page: &Page,
    options: &RenderOptions,
    limits: Limits,
) -> Result<Pixmap> {
    if !options.scale.is_finite() || options.scale <= 0.0 {
        return Err(Error::new(ErrorKind::InvalidScale, 0));
    }
    let (page_to_device, width, height) = page_matrix(page, options.scale);
    let mut pixmap = Pixmap::new(width, height, limits.max_decoded_length)
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded(Limit::DecodedLength), 0))?;
    pixmap.clear(options.background);
    if page.is_damaged() {
        return Ok(pixmap);
    }
    let mut interpreter = Interpreter::painting(limits);
    let content = interpreter.run_page(resolver, page);
    let mut painter = Painter {
        page_to_device,
        base: Clip::new(pixmap.rect()),
        clips: HashMap::new(),
        images: HashMap::new(),
        limits,
    };
    painter.paint(&mut pixmap, resolver, &content);
    Ok(pixmap)
}

struct Painter {
    page_to_device: Matrix,
    base: Clip,
    // The clips already made, by the address of the node that makes them.
    clips: HashMap<usize, Clip>,
    // The images already prepared, once for each object.
    images: HashMap<ObjectId, Option<Arc<super::image::Bitmap>>>,
    limits: Limits,
}

impl Painter {
    fn paint(&mut self, pixmap: &mut Pixmap, resolver: &mut Resolver<'_>, content: &PageContent) {
        for item in &content.order {
            match *item {
                PaintItem::Path(index) => {
                    let painted = &content.paths[index];
                    let clip = self.clip(&painted.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let matrix = painted.matrix.then(&self.page_to_device);
                    if let Some(rule) = painted.fill {
                        let color = painted.paint.fill_color;
                        let alpha = to_byte(painted.paint.fill_alpha);
                        fill_path(pixmap, &clip, &painted.path, &matrix, rule, color, alpha);
                    }
                    if painted.stroke {
                        let color = painted.paint.stroke_color;
                        let alpha = to_byte(painted.paint.stroke_alpha);
                        stroke_path(
                            pixmap,
                            &clip,
                            &painted.path,
                            &matrix,
                            &painted.paint.line,
                            color,
                            alpha,
                        );
                    }
                }
                PaintItem::Image(index) => {
                    let placed = &content.images[index];
                    let clip = self.clip(&placed.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let id = match &placed.source {
                        crate::content::ImageSource::XObject { id, .. } => Some(*id),
                        crate::content::ImageSource::Inline { .. } => None,
                    };
                    let bitmap = match id.and_then(|id| self.images.get(&id).cloned()) {
                        Some(cached) => cached,
                        None => {
                            let image: Option<ImageData> =
                                read_image(resolver, &placed.source, self.limits).ok();
                            let bitmap = image
                                .as_ref()
                                .and_then(|image| prepare(image, self.limits))
                                .map(Arc::new);
                            if let Some(id) = id {
                                self.images.insert(id, bitmap.clone());
                            }
                            bitmap
                        }
                    };
                    let Some(bitmap) = bitmap else { continue };
                    let matrix = placed.matrix.then(&self.page_to_device);
                    let stencil = is_stencil(resolver, &placed.source, self.limits)
                        .then_some(placed.paint.fill_color);
                    draw_bitmap(
                        pixmap,
                        &clip,
                        &bitmap,
                        &matrix,
                        to_byte(placed.paint.fill_alpha),
                        stencil,
                    );
                }
                PaintItem::Glyph(_) => {}
            }
        }
    }

    // The clip of a painting state in device pixels, made once for each node.
    fn clip(&mut self, paint: &PaintState) -> Clip {
        match &paint.clip {
            None => self.base.clone(),
            Some(node) => self.node_clip(node),
        }
    }

    fn node_clip(&mut self, node: &Arc<ClipNode>) -> Clip {
        let key = Arc::as_ptr(node) as usize;
        if let Some(clip) = self.clips.get(&key) {
            return clip.clone();
        }
        let parent = match &node.parent {
            None => self.base.clone(),
            Some(parent) => self.node_clip(parent),
        };
        let matrix = node.matrix.then(&self.page_to_device);
        let mask = flatten(&node.path, &matrix, 0.1)
            .and_then(|contours| fill_contours(&contours, node.rule, &parent.rect));
        let clip = parent.intersect(mask.as_ref());
        self.clips.insert(key, clip.clone());
        clip
    }
}

fn is_stencil(
    resolver: &mut Resolver<'_>,
    source: &crate::content::ImageSource,
    limits: Limits,
) -> bool {
    read_image(resolver, source, limits)
        .map(|image| image.description.image_mask)
        .unwrap_or(false)
}

fn to_byte(alpha: f64) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn fill_path(
    pixmap: &mut Pixmap,
    clip: &Clip,
    path: &Path,
    matrix: &Matrix,
    rule: FillRule,
    color: [u8; 3],
    alpha: u8,
) {
    let Some(contours) = flatten(path, matrix, 0.1) else {
        return;
    };
    if let Some(mask) = fill_contours(&contours, rule, &clip.rect.intersect(&pixmap.rect())) {
        pixmap.fill(&mask, clip, color, alpha);
    }
}

fn stroke_path(
    pixmap: &mut Pixmap,
    clip: &Clip,
    path: &Path,
    matrix: &Matrix,
    style: &StrokeStyle,
    color: [u8; 3],
    alpha: u8,
) {
    // A line thinner than a pixel is a pixel wide, as the thinnest line a
    // device draws (section 10.6.2).
    let scale = matrix.a.hypot(matrix.b).max(matrix.c.hypot(matrix.d));
    if scale <= 0.0 || !scale.is_finite() {
        return;
    }
    let mut style = style.clone();
    if style.width * scale < 1.0 {
        style.width = 1.0 / scale;
    }
    let Some(contours) = flatten(path, &Matrix::IDENTITY, 0.1 / scale) else {
        return;
    };
    let Some(outline) = stroke_contours(&contours, &style, matrix, 0.1) else {
        return;
    };
    if let Some(mask) = fill_contours(
        &outline,
        FillRule::NonZero,
        &clip.rect.intersect(&pixmap.rect()),
    ) {
        pixmap.fill(&mask, clip, color, alpha);
    }
}
