use std::sync::Arc;

use super::graphics_state::Matrix;
use crate::fonts::Font;

/// How glyphs are painted, set with `Tr` (ISO 32000-2, section 9.3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextRenderingMode {
    #[default]
    Fill,
    Stroke,
    FillStroke,
    /// Neither filled nor stroked: the text can't be seen, like the text of a
    /// scanned page under its image.
    Invisible,
    FillClip,
    StrokeClip,
    FillStrokeClip,
    Clip,
}

impl TextRenderingMode {
    /// The mode numbered `value`, from 0 to 7.
    pub fn from_number(value: i64) -> Option<Self> {
        Some(match value {
            0 => Self::Fill,
            1 => Self::Stroke,
            2 => Self::FillStroke,
            3 => Self::Invisible,
            4 => Self::FillClip,
            5 => Self::StrokeClip,
            6 => Self::FillStrokeClip,
            7 => Self::Clip,
            _ => return None,
        })
    }
}

/// The text state parameters, part of the graphics state, so `q` and `Q` save
/// and restore them (ISO 32000-2, section 9.3).
#[derive(Debug, Clone, PartialEq)]
pub struct TextState {
    /// `Tc`, in unscaled text space units.
    pub character_spacing: f64,
    /// `Tw`, added to each single-byte code 32, in unscaled text space units.
    pub word_spacing: f64,
    /// `Tz` divided by 100: 1 for glyphs as wide as the font draws them.
    pub horizontal_scaling: f64,
    /// `TL`, used by `T*`, `'` and `"`.
    pub leading: f64,
    /// The font set with `Tf`, or with `/Font` in an `ExtGState`.
    pub font: Option<Arc<Font>>,
    /// The font size set with `Tf`.
    pub font_size: f64,
    pub rendering_mode: TextRenderingMode,
    /// `Ts`, in unscaled text space units.
    pub rise: f64,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            character_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            leading: 0.0,
            font: None,
            font_size: 0.0,
            rendering_mode: TextRenderingMode::Fill,
            rise: 0.0,
        }
    }
}

/// The text matrix and the text line matrix, which exist between `BT` and
/// `ET` (ISO 32000-2, section 9.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextObject {
    /// The text matrix, `Tm`: where the next glyph goes.
    pub matrix: Matrix,
    /// The text line matrix, `Tlm`: the start of the current line.
    pub line_matrix: Matrix,
}

impl TextObject {
    /// `Td`: starts a new line, offset by `(x, y)` from the start of the
    /// current one.
    pub fn move_text(&mut self, x: f64, y: f64) {
        self.line_matrix = Matrix::translation(x, y).then(&self.line_matrix);
        self.matrix = self.line_matrix;
    }

    /// `Tm`: both matrices become `matrix`.
    pub fn set_matrix(&mut self, matrix: Matrix) {
        self.matrix = matrix;
        self.line_matrix = matrix;
    }

    /// `T*`: starts the next line, `leading` below the current one.
    pub fn next_line(&mut self, leading: f64) {
        self.move_text(0.0, -leading);
    }

    /// Moves the text matrix by `(x, y)` in text space, after a glyph or a
    /// `TJ` adjustment.
    pub fn advance(&mut self, x: f64, y: f64) {
        self.matrix = Matrix::translation(x, y).then(&self.matrix);
    }

    /// The text rendering matrix, from text space to the default user space
    /// of the page: the font size, horizontal scaling and rise of `state`,
    /// then the text matrix, then `ctm` (section 9.4.4).
    pub fn rendering_matrix(&self, state: &TextState, ctm: &Matrix) -> Matrix {
        let size = Matrix::new(
            state.font_size * state.horizontal_scaling,
            0.0,
            0.0,
            state.font_size,
            0.0,
            state.rise,
        );
        size.then(&self.matrix).then(ctm)
    }
}
