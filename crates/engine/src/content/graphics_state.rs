use std::sync::Arc;

use super::text_state::TextState;
use crate::images::ColorSpace;
use crate::render::{BlendMode, FillRule, Path, StrokeStyle};
use crate::syntax::{Name, Object, ObjectId};

/// A transformation matrix `[a b c d e f]`, which maps a point `(x, y)` to
/// `(a·x + c·y + e, b·x + d·y + f)` (ISO 32000-2, section 8.3.4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// A translation by `(x, y)`.
    pub const fn translation(x: f64, y: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// The matrix of six numbers, like the operands of `cm` or a `/Matrix`
    /// entry. `None` if they aren't six finite numbers.
    pub fn from_objects(objects: &[Object]) -> Option<Self> {
        let [a, b, c, d, e, f] = objects else {
            return None;
        };
        let matrix = Self::new(
            a.as_number()?,
            b.as_number()?,
            c.as_number()?,
            d.as_number()?,
            e.as_number()?,
            f.as_number()?,
        );
        matrix.is_finite().then_some(matrix)
    }

    /// `self × other`: the transformation of `self` followed by that of
    /// `other`.
    pub fn then(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    /// Where the matrix maps the point `(x, y)`.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Whether every number is finite, neither infinite nor NaN.
    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .iter()
            .all(|value| value.is_finite())
    }
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A clipping path that was set, and the one it cut (ISO 32000-2, section
/// 8.5.4). Paths and states share them, so the clip a graphics state saves is
/// one pointer.
#[derive(Debug, PartialEq)]
pub struct ClipNode {
    /// The clip that this path cut, if any.
    pub parent: Option<Arc<ClipNode>>,
    pub path: Path,
    /// The matrix the path was made with: from its user space to the default
    /// user space of the page.
    pub matrix: Matrix,
    pub rule: FillRule,
    /// For the clip text makes (rendering modes 4 to 7), the glyphs, with the
    /// matrix of each, instead of a path.
    pub glyphs: Vec<ClipGlyph>,
}

/// A glyph that is part of a clip made by text.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipGlyph {
    pub font: Arc<crate::fonts::Font>,
    pub code: crate::fonts::Code,
    /// Its text rendering matrix, as in [`Glyph`](super::Glyph).
    pub matrix: Matrix,
}

/// A soft mask set with an `ExtGState`'s `/SMask` (ISO 32000-2, section
/// 11.6.5.2): a form whose painting gives the opacity of what comes after.
#[derive(Debug, PartialEq)]
pub struct SoftMask {
    /// The transparency group that makes the mask: a form XObject.
    pub form: ObjectId,
    /// The current matrix when it was set, which the form is painted with.
    pub matrix: Matrix,
    /// `true` for a luminosity mask, `false` for one of alpha.
    pub luminosity: bool,
    /// The color the group is painted over, for a luminosity mask.
    pub backdrop: [u8; 3],
}

/// A pattern that areas are filled or stroked with, set with `scn` (ISO
/// 32000-2, section 8.7).
#[derive(Debug, PartialEq)]
pub struct PatternRef {
    /// The pattern: a reference to its object (always a stream for a tiling
    /// pattern), or the dictionary of a shading pattern written in place.
    pub object: Object,
    /// The default space of the content stream that uses it, which the
    /// pattern's `/Matrix` maps to (section 8.7.3.1).
    pub base: Matrix,
    /// For an uncolored tiling pattern, the color it is painted in.
    pub color: [u8; 3],
}

/// What painting uses of the graphics state: colors, opacity, the line and
/// the clip.
#[derive(Debug, Clone, PartialEq)]
pub struct PaintState {
    /// The color areas are filled with, as RGB, and its space.
    pub fill_color: [u8; 3],
    pub fill_space: Option<Arc<ColorSpace>>,
    pub stroke_color: [u8; 3],
    pub stroke_space: Option<Arc<ColorSpace>>,
    /// The constant opacity of fills and strokes, `/ca` and `/CA`, from 0 to
    /// 1.
    pub fill_alpha: f64,
    pub stroke_alpha: f64,
    pub line: Arc<StrokeStyle>,
    pub clip: Option<Arc<ClipNode>>,
    /// How what is painted mixes with what is under it, `/BM`.
    pub blend: BlendMode,
    pub soft_mask: Option<Arc<SoftMask>>,
    /// The patterns fills and strokes use instead of their colors.
    pub fill_pattern: Option<Arc<PatternRef>>,
    pub stroke_pattern: Option<Arc<PatternRef>>,
}

impl Default for PaintState {
    fn default() -> Self {
        Self {
            fill_color: [0; 3],
            fill_space: None,
            stroke_color: [0; 3],
            stroke_space: None,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            line: Arc::new(StrokeStyle::default()),
            clip: None,
            blend: BlendMode::Normal,
            soft_mask: None,
            fill_pattern: None,
            stroke_pattern: None,
        }
    }
}

/// The parts of the graphics state that matter for reading a page: the
/// current transformation matrix, the text state, the pattern areas are
/// filled with, and what painting uses (ISO 32000-2, section 8.4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphicsState {
    pub paint: PaintState,
    /// The current transformation matrix, from user space to the default user
    /// space of the page.
    pub ctm: Matrix,
    pub text: TextState,
    /// The name, in the resources' `/Pattern`, of the pattern that fills
    /// areas, set with `scn` in the `/Pattern` color space. `None` when they
    /// are filled with a color.
    pub fill_pattern: Option<Name>,
}

/// The graphics state and the states saved with `q`, restored with `Q`
/// (ISO 32000-2, section 8.4.2).
#[derive(Debug, Clone)]
pub struct GraphicsStack {
    current: GraphicsState,
    saved: Vec<GraphicsState>,
    max_depth: usize,
    // Saves beyond `max_depth`, ignored, so that their restores are too.
    ignored: usize,
}

impl GraphicsStack {
    /// A stack starting at `state`, which keeps at most `max_depth` saved
    /// states.
    pub fn new(state: GraphicsState, max_depth: usize) -> Self {
        Self {
            current: state,
            saved: Vec::new(),
            max_depth,
            ignored: 0,
        }
    }

    pub fn current(&self) -> &GraphicsState {
        &self.current
    }

    pub fn current_mut(&mut self) -> &mut GraphicsState {
        &mut self.current
    }

    /// How many states are saved.
    pub fn depth(&self) -> usize {
        self.saved.len() + self.ignored
    }

    /// `q`: saves a copy of the current state. `false` if `max_depth` states
    /// are saved already: the save is ignored, and so is the restore that
    /// matches it.
    pub fn save(&mut self) -> bool {
        if self.saved.len() >= self.max_depth {
            self.ignored += 1;
            return false;
        }
        self.saved.push(self.current.clone());
        true
    }

    /// `Q`: goes back to the state saved last. `false` if none is saved: the
    /// current state is kept.
    pub fn restore(&mut self) -> bool {
        if self.ignored > 0 {
            self.ignored -= 1;
            return true;
        }
        match self.saved.pop() {
            Some(state) => {
                self.current = state;
                true
            }
            None => false,
        }
    }

    /// `cm`: the current transformation matrix becomes `matrix × ctm`. `false`
    /// if that isn't finite: the matrix is kept as it was.
    pub fn transform(&mut self, matrix: &Matrix) -> bool {
        let ctm = matrix.then(&self.current.ctm);
        if !ctm.is_finite() {
            return false;
        }
        self.current.ctm = ctm;
        true
    }
}
