//! Content streams: the operations that paint a page or a form, and what they
//! leave on it (ISO 32000-2, sections 7.8, 8 and 9).

mod graphics_state;
mod interpreter;
mod operators;
mod text_state;

pub use graphics_state::{GraphicsStack, GraphicsState, Matrix};
pub use interpreter::{Glyph, Interpreter, PageContent};
pub use operators::{ContentParser, InlineImage, Operation, Operator};
pub use text_state::{TextObject, TextRenderingMode, TextState};
