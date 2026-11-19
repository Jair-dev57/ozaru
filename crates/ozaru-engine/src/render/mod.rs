//! Rendering: drawing paths as pixels. The rasterizer is the engine's own: it
//! fills and strokes paths with anti-aliasing, clips, and blends colors into
//! a pixmap.

mod blend;
mod canvas;
mod glyphs;
mod image;
mod page;
mod path;
mod raster;
mod shading;
mod stroke;

pub use blend::BlendMode;
pub use canvas::{Clip, Pixmap};
pub use glyphs::{GlyphCache, GlyphShape};
pub use image::{Bitmap, draw_bitmap, jpeg_scale, prepare};
pub use page::{RenderOptions, page_matrix, render_page};
pub use path::{Contour, Path, Segment, flatten};
pub use raster::{FillRule, IRect, Mask, fill_contours, rect_mask};
pub use shading::{Shading, paint_shading, read_shading};
pub use stroke::{LineCap, LineJoin, StrokeStyle, stroke_contours};
