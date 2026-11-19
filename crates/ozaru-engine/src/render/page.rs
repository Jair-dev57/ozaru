use std::collections::HashMap;
use std::sync::Arc;

use super::blend::BlendMode;
use super::canvas::{Clip, Pixmap};
use super::glyphs::GlyphCache;
use super::image::{draw_bitmap, prepare};
use super::path::{Path, flatten};
use super::raster::{FillRule, Mask, fill_contours};
use super::stroke::{StrokeStyle, stroke_contours};
use crate::base::{Error, ErrorKind, Limit, Limits, Result};
use crate::content::{
    ClipNode, Interpreter, Matrix, PageContent, PaintItem, PaintState, SoftMask, TextRenderingMode,
};
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
        glyphs: GlyphCache::new(),
        masks: HashMap::new(),
        size: (width, height),
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
    glyphs: GlyphCache,
    // The soft masks already drawn, by the address of the mask.
    masks: HashMap<usize, Option<Mask>>,
    // The size of the pixmap, which masks and groups share.
    size: (usize, usize),
    limits: Limits,
}

// What a shape is painted with.
#[derive(Clone, Copy)]
struct Ink {
    color: [u8; 3],
    alpha: u8,
    mode: BlendMode,
}

impl Painter {
    fn paint(&mut self, pixmap: &mut Pixmap, resolver: &mut Resolver<'_>, content: &PageContent) {
        self.paint_range(pixmap, resolver, content, 0, content.order.len());
    }

    // Paints the items from `start` to `end` of the order.
    fn paint_range(
        &mut self,
        pixmap: &mut Pixmap,
        resolver: &mut Resolver<'_>,
        content: &PageContent,
        start: usize,
        end: usize,
    ) {
        let mut next = start;
        while next < end {
            let item = content.order[next];
            next += 1;
            match item {
                PaintItem::EndGroup => {}
                PaintItem::BeginGroup(index) => {
                    // The group ends at the `EndGroup` that matches.
                    let (mut depth, mut close) = (1, next);
                    while close < end {
                        match content.order[close] {
                            PaintItem::BeginGroup(_) => depth += 1,
                            PaintItem::EndGroup => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        close += 1;
                    }
                    self.paint_group(pixmap, resolver, content, index, next, close);
                    next = close + 1;
                }
                PaintItem::Path(index) => {
                    let painted = &content.paths[index];
                    let clip = self.clip(resolver, &painted.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let matrix = painted.matrix.then(&self.page_to_device);
                    if let Some(rule) = painted.fill {
                        let ink = Ink {
                            color: painted.paint.fill_color,
                            alpha: to_byte(painted.paint.fill_alpha),
                            mode: painted.paint.blend,
                        };
                        fill_path(pixmap, &clip, &painted.path, &matrix, rule, ink);
                    }
                    if painted.stroke {
                        let ink = Ink {
                            color: painted.paint.stroke_color,
                            alpha: to_byte(painted.paint.stroke_alpha),
                            mode: painted.paint.blend,
                        };
                        stroke_path(
                            pixmap,
                            &clip,
                            &painted.path,
                            &matrix,
                            &painted.paint.line,
                            ink,
                        );
                    }
                }
                PaintItem::Image(index) => {
                    let placed = &content.images[index];
                    let clip = self.clip(resolver, &placed.paint);
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
                        placed.paint.blend,
                    );
                }
                PaintItem::Glyph(index) => {
                    let glyph = &content.glyphs[index];
                    let mode = glyph.rendering_mode;
                    let fill = matches!(
                        mode,
                        TextRenderingMode::Fill
                            | TextRenderingMode::FillStroke
                            | TextRenderingMode::FillClip
                            | TextRenderingMode::FillStrokeClip
                    );
                    let stroke = matches!(
                        mode,
                        TextRenderingMode::Stroke
                            | TextRenderingMode::FillStroke
                            | TextRenderingMode::StrokeClip
                            | TextRenderingMode::FillStrokeClip
                    );
                    if !fill && !stroke {
                        continue;
                    }
                    let clip = self.clip(resolver, &glyph.paint);
                    if clip.rect.is_empty() {
                        continue;
                    }
                    let Some(shape) = self.glyphs.shape(&glyph.font, glyph.code) else {
                        continue;
                    };
                    let matrix = shape.matrix.then(&glyph.matrix).then(&self.page_to_device);
                    if fill {
                        let ink = Ink {
                            color: glyph.paint.fill_color,
                            alpha: to_byte(glyph.paint.fill_alpha),
                            mode: glyph.paint.blend,
                        };
                        fill_path(pixmap, &clip, &shape.path, &matrix, FillRule::NonZero, ink);
                    }
                    if stroke {
                        // The line width is in the space of the page, as the
                        // glyph's own matrix has the font size in it.
                        let in_page = shape.path.transformed(&shape.matrix.then(&glyph.matrix));
                        let ink = Ink {
                            color: glyph.paint.stroke_color,
                            alpha: to_byte(glyph.paint.stroke_alpha),
                            mode: glyph.paint.blend,
                        };
                        stroke_path(
                            pixmap,
                            &clip,
                            &in_page,
                            &self.page_to_device,
                            &glyph.paint.line,
                            ink,
                        );
                    }
                }
            }
        }
    }

    // The clip of a painting state in device pixels, made once for each node.
    // The clip of a painting state in device pixels, with its soft mask.
    fn clip(&mut self, resolver: &mut Resolver<'_>, paint: &PaintState) -> Clip {
        let clip = match &paint.clip {
            None => self.base.clone(),
            Some(node) => self.node_clip(node),
        };
        match &paint.soft_mask {
            Some(soft) if !clip.rect.is_empty() => {
                let mask = self.soft_mask(resolver, soft);
                // A mask that can't be drawn doesn't hide anything.
                match mask {
                    Some(mask) => clip.intersect(Some(&mask)),
                    None => clip,
                }
            }
            _ => clip,
        }
    }

    // The opacity a soft mask gives each pixel, drawn once for each mask.
    fn soft_mask(&mut self, resolver: &mut Resolver<'_>, soft: &Arc<SoftMask>) -> Option<Mask> {
        let key = Arc::as_ptr(soft) as usize;
        if let Some(mask) = self.masks.get(&key) {
            return mask.clone();
        }
        // Guards against a mask that uses itself.
        self.masks.insert(key, None);
        let (width, height) = self.size;
        let mut pixmap = Pixmap::new(width, height, self.limits.max_decoded_length)?;
        if soft.luminosity {
            pixmap.clear(soft.backdrop);
        }
        let mut interpreter = Interpreter::painting(self.limits);
        // The form is painted with the matrix it had when the mask was set.
        let content = interpreter.run_form(resolver, soft.form, &soft.matrix);
        let mut inner = Painter {
            page_to_device: self.page_to_device,
            base: self.base.clone(),
            clips: HashMap::new(),
            images: HashMap::new(),
            glyphs: GlyphCache::new(),
            masks: HashMap::new(),
            size: self.size,
            limits: self.limits,
        };
        inner.paint(&mut pixmap, resolver, &content);
        let rect = pixmap.rect();
        let data: Vec<u8> = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                if soft.luminosity {
                    // Opaque over the backdrop: the luminosity of the color.
                    let l = 0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2]);
                    (l + 0.5).min(255.0) as u8
                } else {
                    p[3]
                }
            })
            .collect();
        let mask = Mask { rect, data };
        self.masks.insert(key, Some(mask.clone()));
        Some(mask)
    }

    // Paints the group of the items `start` to `end` on its own pixmap and puts
    // it over `pixmap` with the opacity, blend mode and clip it was painted
    // with.
    fn paint_group(
        &mut self,
        pixmap: &mut Pixmap,
        resolver: &mut Resolver<'_>,
        content: &PageContent,
        index: usize,
        start: usize,
        end: usize,
    ) {
        let group = &content.groups[index];
        let clip = self.clip(resolver, &group.paint);
        if clip.rect.is_empty() {
            return;
        }
        let Some(mut own) = Pixmap::new(
            pixmap.width(),
            pixmap.height(),
            self.limits.max_decoded_length,
        ) else {
            return;
        };
        self.paint_range(&mut own, resolver, content, start, end);
        pixmap.composite(
            &own,
            &clip,
            to_byte(group.paint.fill_alpha),
            group.paint.blend,
        );
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
    ink: Ink,
) {
    let Some(contours) = flatten(path, matrix, 0.1) else {
        return;
    };
    if let Some(mask) = fill_contours(&contours, rule, &clip.rect.intersect(&pixmap.rect())) {
        pixmap.fill_with(&mask, clip, ink.color, ink.alpha, ink.mode);
    }
}

fn stroke_path(
    pixmap: &mut Pixmap,
    clip: &Clip,
    path: &Path,
    matrix: &Matrix,
    style: &StrokeStyle,
    ink: Ink,
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
        pixmap.fill_with(&mask, clip, ink.color, ink.alpha, ink.mode);
    }
}
