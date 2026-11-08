use crate::syntax::Object;

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

/// The parts of the graphics state that matter for reading a page: the
/// current transformation matrix (ISO 32000-2, section 8.4). Colours, lines
/// and clipping come with rendering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphicsState {
    /// The current transformation matrix, from user space to the default user
    /// space of the page.
    pub ctm: Matrix,
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
