use crate::content::Matrix;

/// One step of a path (ISO 32000-2, section 8.5.2), in user space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    /// Starts a new subpath.
    Move(f64, f64),
    Line(f64, f64),
    /// A cubic Bézier curve to the last point, with its two control points
    /// first.
    Curve(f64, f64, f64, f64, f64, f64),
    /// Closes the subpath with a line to its start.
    Close,
}

/// A path: the subpaths painting operators fill, stroke or clip with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub segments: Vec<Segment>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.segments.push(Segment::Move(x, y));
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.segments.push(Segment::Line(x, y));
    }

    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.segments.push(Segment::Curve(x1, y1, x2, y2, x3, y3));
    }

    pub fn close(&mut self) {
        self.segments.push(Segment::Close);
    }

    /// A closed rectangle, as the `re` operator makes it.
    pub fn rectangle(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.move_to(x, y);
        self.line_to(x + width, y);
        self.line_to(x + width, y + height);
        self.line_to(x, y + height);
        self.close();
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The path with every point moved by `matrix`.
    pub fn transformed(&self, matrix: &Matrix) -> Self {
        let segments = self
            .segments
            .iter()
            .map(|segment| match *segment {
                Segment::Move(x, y) => {
                    let (x, y) = matrix.apply(x, y);
                    Segment::Move(x, y)
                }
                Segment::Line(x, y) => {
                    let (x, y) = matrix.apply(x, y);
                    Segment::Line(x, y)
                }
                Segment::Curve(x1, y1, x2, y2, x3, y3) => {
                    let (x1, y1) = matrix.apply(x1, y1);
                    let (x2, y2) = matrix.apply(x2, y2);
                    let (x3, y3) = matrix.apply(x3, y3);
                    Segment::Curve(x1, y1, x2, y2, x3, y3)
                }
                Segment::Close => Segment::Close,
            })
            .collect();
        Self { segments }
    }
}

/// A subpath as straight lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub points: Vec<(f64, f64)>,
    pub closed: bool,
}

/// The most points curves are cut into: a few of them make a shape of
/// thousands of points, and no more is needed.
const MAX_CURVE_STEPS: usize = 512;

/// The most points that all the contours of a path may have, so that a path
/// can't make the work or the memory grow without a limit.
pub(super) const MAX_POINTS: usize = 4_000_000;

/// Cuts the curves of `path` into lines no farther than `tolerance` from
/// them, after moving every point with `matrix`. A subpath that is only a
/// move is left out, and a subpath with a single point stays as it is (a
/// stroke may draw a dot there). `None` if the path has more points than
/// `MAX_POINTS`, or any that isn't finite.
pub fn flatten(path: &Path, matrix: &Matrix, tolerance: f64) -> Option<Vec<Contour>> {
    let mut contours: Vec<Contour> = Vec::new();
    let mut current: Option<Contour> = None;
    let mut start = (0.0, 0.0);
    let mut total = 0usize;
    let finish = |contour: Option<Contour>, contours: &mut Vec<Contour>| {
        if let Some(contour) = contour {
            // A bare move paints nothing.
            if contour.points.len() > 1 || contour.closed {
                contours.push(contour);
            }
        }
    };
    for segment in &path.segments {
        match *segment {
            Segment::Move(x, y) => {
                finish(current.take(), &mut contours);
                let point = matrix.apply(x, y);
                start = point;
                total += 1;
                current = Some(Contour {
                    points: vec![point],
                    closed: false,
                });
            }
            Segment::Line(x, y) => {
                let point = matrix.apply(x, y);
                let contour = current.get_or_insert_with(|| Contour {
                    points: vec![start],
                    closed: false,
                });
                contour.points.push(point);
                total += 1;
            }
            Segment::Curve(x1, y1, x2, y2, x3, y3) => {
                let contour = current.get_or_insert_with(|| Contour {
                    points: vec![start],
                    closed: false,
                });
                let p0 = *contour.points.last().unwrap_or(&start);
                let p1 = matrix.apply(x1, y1);
                let p2 = matrix.apply(x2, y2);
                let p3 = matrix.apply(x3, y3);
                // The second difference bounds how far the curve bends.
                let dd = (p0.0 - 2.0 * p1.0 + p2.0)
                    .hypot(p0.1 - 2.0 * p1.1 + p2.1)
                    .max((p1.0 - 2.0 * p2.0 + p3.0).hypot(p1.1 - 2.0 * p2.1 + p3.1));
                let steps = if dd.is_finite() {
                    ((0.75 * dd / tolerance).sqrt().ceil() as usize).clamp(1, MAX_CURVE_STEPS)
                } else {
                    1
                };
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let u = 1.0 - t;
                    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    contour.points.push((
                        b0 * p0.0 + b1 * p1.0 + b2 * p2.0 + b3 * p3.0,
                        b0 * p0.1 + b1 * p1.1 + b2 * p2.1 + b3 * p3.1,
                    ));
                }
                total += steps;
            }
            Segment::Close => {
                if let Some(mut contour) = current.take() {
                    contour.closed = true;
                    contours.push(contour);
                    // The next segment starts a subpath at the same point.
                    current = None;
                }
            }
        }
        if total > MAX_POINTS {
            return None;
        }
    }
    finish(current.take(), &mut contours);
    let finite = contours
        .iter()
        .all(|c| c.points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    finite.then_some(contours)
}
