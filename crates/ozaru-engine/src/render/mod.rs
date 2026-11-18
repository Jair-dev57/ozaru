//! Rendering: drawing paths as pixels. The rasterizer is the engine's own: it
//! fills and strokes paths with anti-aliasing, clips, and blends colors into
//! a pixmap.

mod canvas;
mod path;
mod raster;
mod stroke;

pub use canvas::{Clip, Pixmap};
pub use path::{Contour, Path, Segment, flatten};
pub use raster::{FillRule, IRect, Mask, fill_contours};
pub use stroke::{LineCap, LineJoin, StrokeStyle, stroke_contours};
