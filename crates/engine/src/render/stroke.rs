use super::path::{Contour, MAX_POINTS};
use crate::content::Matrix;

/// How the ends of an open subpath, and of dashes, are drawn (ISO 32000-2,
/// section 8.4.3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    /// Squared off at the end point.
    Butt,
    /// A semicircle around the end point.
    Round,
    /// Squared off past the end point by half the width.
    Square,
}

/// How two lines of a subpath meet (section 8.4.3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    /// The outer edges go on until they meet, unless that is too far.
    Miter,
    Round,
    /// The outer corner is cut off.
    Bevel,
}

/// How a path is stroked, in user space units.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    /// The longest a miter may be, as a multiple of the line width.
    pub miter_limit: f64,
    /// The lengths of the dashes and gaps in turn, or empty for a solid line.
    pub dash: Vec<f64>,
    /// How far into the dash pattern the line starts.
    pub dash_phase: f64,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 10.0,
            dash: Vec::new(),
            dash_phase: 0.0,
        }
    }
}

/// The most dashes one stroke is cut into: a pattern of tiny dashes along a
/// long path would make the work grow without a limit, and it is a solid
/// line to the eye.
const MAX_DASHES: usize = 50_000;

type Point = (f64, f64);

/// The area that stroking `contours`, in user space, paints, as polygons in
/// device space to fill with the non-zero rule. `tolerance` is how far, in
/// device pixels, round parts may be from true circles. `None` if the stroke
/// would be made of too many points.
pub fn stroke_contours(
    contours: &[Contour],
    style: &StrokeStyle,
    matrix: &Matrix,
    tolerance: f64,
) -> Option<Vec<Contour>> {
    let half = style.width / 2.0;
    if !half.is_finite() || half <= 0.0 {
        return Some(Vec::new());
    }
    let scale = (matrix.a.hypot(matrix.b)).max(matrix.c.hypot(matrix.d));
    let radius = half * scale;
    // The angle each piece of an arc spans.
    let step = if radius > tolerance {
        (2.0 * (1.0 - tolerance / radius).acos()).clamp(0.05, std::f64::consts::FRAC_PI_2)
    } else {
        std::f64::consts::FRAC_PI_2
    };
    let stroker = Stroker { half, style, step };
    let dashes = dash_pattern(style);
    let mut polygons: Vec<Vec<Point>> = Vec::new();
    let mut count = 0usize;
    let mut pieces = 0usize;
    for contour in contours {
        let lines = match &dashes {
            Some(pattern) => dash(contour, pattern, style.dash_phase, &mut pieces)?,
            None => vec![contour.clone()],
        };
        for line in &lines {
            stroker.stroke(line, &mut polygons);
            count += polygons.last().map_or(0, Vec::len);
            if count > MAX_POINTS {
                return None;
            }
        }
    }
    Some(
        polygons
            .into_iter()
            .map(|polygon| Contour {
                points: polygon
                    .into_iter()
                    .map(|(x, y)| matrix.apply(x, y))
                    .collect(),
                closed: true,
            })
            .collect(),
    )
}

// The dash pattern if it can be used: no negative length, and some length.
fn dash_pattern(style: &StrokeStyle) -> Option<Vec<f64>> {
    let valid = !style.dash.is_empty()
        && style.dash.iter().all(|d| d.is_finite() && *d >= 0.0)
        && style.dash.iter().sum::<f64>() > 0.0
        && style.dash_phase.is_finite();
    // An odd number of lengths repeats itself with gaps and dashes swapped.
    valid.then(|| {
        if style.dash.len() % 2 == 1 {
            style.dash.iter().chain(&style.dash).copied().collect()
        } else {
            style.dash.clone()
        }
    })
}

/// Cuts a contour into the open pieces that are on in the dash pattern.
fn dash(
    contour: &Contour,
    pattern: &[f64],
    phase: f64,
    pieces: &mut usize,
) -> Option<Vec<Contour>> {
    let mut points = contour.points.clone();
    if contour.closed && points.len() > 1 {
        points.push(points[0]);
    }
    let total: f64 = pattern.iter().sum();
    // Where in the pattern the line starts.
    let mut position = phase.rem_euclid(total);
    let mut index = 0;
    // A dash of no length at the start is kept, to be a dot.
    while position > pattern[index] || (position == pattern[index] && pattern[index] > 0.0) {
        position -= pattern[index];
        index = (index + 1) % pattern.len();
    }
    let mut left = pattern[index] - position;
    let mut on = index % 2 == 0;
    let mut out: Vec<Contour> = Vec::new();
    let mut current: Vec<Point> = if on {
        points.first().map(|p| vec![*p]).unwrap_or_default()
    } else {
        Vec::new()
    };
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        let mut done = 0.0;
        while length - done > left {
            done += left;
            let t = done / length;
            let point = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if on {
                current.push(point);
                out.push(Contour {
                    points: std::mem::take(&mut current),
                    closed: false,
                });
                *pieces += 1;
                if *pieces > MAX_DASHES {
                    return None;
                }
            } else {
                current = vec![point];
            }
            on = !on;
            index = (index + 1) % pattern.len();
            left = pattern[index];
        }
        left -= length - done;
        if on {
            current.push(b);
        }
    }
    if on && !current.is_empty() {
        out.push(Contour {
            points: current,
            closed: false,
        });
    }
    Some(out)
}

struct Stroker<'s> {
    half: f64,
    style: &'s StrokeStyle,
    step: f64,
}

fn same(a: Point, b: Point) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

fn signed_area(polygon: &[Point]) -> f64 {
    let mut sum = 0.0;
    let mut previous = polygon[polygon.len() - 1];
    for &point in polygon {
        sum += previous.0 * point.1 - point.0 * previous.1;
        previous = point;
    }
    sum / 2.0
}

impl Stroker<'_> {
    fn stroke(&self, contour: &Contour, out: &mut Vec<Vec<Point>>) {
        let mut points: Vec<Point> = Vec::with_capacity(contour.points.len());
        for &point in &contour.points {
            if points.last().is_none_or(|&last| !same(last, point)) {
                points.push(point);
            }
        }
        let closed = contour.closed && points.len() > 2;
        if closed && same(points[0], points[points.len() - 1]) {
            points.pop();
        }
        if points.len() < 2 {
            if let Some(&point) = points.first() {
                self.dot(point, out);
            }
            return;
        }
        // The unit direction of each segment; the last goes back to the
        // start of a closed subpath.
        let count = if closed {
            points.len()
        } else {
            points.len() - 1
        };
        let directions: Vec<Point> = (0..count)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                let length = (b.0 - a.0).hypot(b.1 - a.1);
                ((b.0 - a.0) / length, (b.1 - a.1) / length)
            })
            .collect();
        if closed {
            let mut left = Vec::new();
            let mut right = Vec::new();
            for i in 0..points.len() {
                let before = directions[(i + directions.len() - 1) % directions.len()];
                self.join(points[i], before, directions[i], 1.0, &mut left);
                self.join(points[i], before, directions[i], -1.0, &mut right);
            }
            right.reverse();
            // The bigger loop is the outside of the ring, the other its hole.
            let (outer, inner) = if signed_area(&left).abs() >= signed_area(&right).abs() {
                (left, right)
            } else {
                (right, left)
            };
            let mut outer = outer;
            let mut inner = inner;
            if signed_area(&outer) < 0.0 {
                outer.reverse();
            }
            if signed_area(&inner) > 0.0 {
                inner.reverse();
            }
            out.push(outer);
            out.push(inner);
            return;
        }
        let mut outline = Vec::new();
        let last = points.len() - 1;
        self.offset(points[0], directions[0], 1.0, &mut outline);
        for i in 1..last {
            self.join(
                points[i],
                directions[i - 1],
                directions[i],
                1.0,
                &mut outline,
            );
        }
        self.offset(points[last], directions[last - 1], 1.0, &mut outline);
        self.cap(points[last], directions[last - 1], &mut outline);
        self.offset(points[last], directions[last - 1], -1.0, &mut outline);
        for i in (1..last).rev() {
            // Going back, the directions are reversed and so are the sides.
            let back = (-directions[i].0, -directions[i].1);
            let before = (-directions[i - 1].0, -directions[i - 1].1);
            self.join(points[i], back, before, 1.0, &mut outline);
        }
        self.offset(points[0], directions[0], -1.0, &mut outline);
        self.cap(
            points[0],
            (-directions[0].0, -directions[0].1),
            &mut outline,
        );
        if signed_area(&outline) < 0.0 {
            outline.reverse();
        }
        out.push(outline);
    }

    // The point `side` (1 left, -1 right) of `at`, `half` away from a line in
    // `direction`.
    fn offset(&self, at: Point, direction: Point, side: f64, out: &mut Vec<Point>) {
        out.push((
            at.0 - side * self.half * direction.1,
            at.1 + side * self.half * direction.0,
        ));
    }

    // The vertex between lines in directions `before` and `after`, on one
    // side of the path.
    fn join(&self, at: Point, before: Point, after: Point, side: f64, out: &mut Vec<Point>) {
        let cross = before.0 * after.1 - before.1 * after.0;
        let dot = before.0 * after.0 + before.1 * after.1;
        if cross.abs() < 1e-12 && dot > 0.0 {
            self.offset(at, before, side, out);
            return;
        }
        // The side is on the outside of the turn when the turn is away from it.
        let outer = if cross.abs() < 1e-12 {
            side > 0.0
        } else {
            cross * side < 0.0
        };
        if !outer {
            // Through the vertex, which the fill rule makes whole.
            self.offset(at, before, side, out);
            out.push(at);
            self.offset(at, after, side, out);
            return;
        }
        match self.style.join {
            LineJoin::Miter => {
                // The miter's length over the width is 1 / sin(half the angle
                // between the lines), which is 1 / cos(half the turn).
                let cos_turn = dot;
                let ratio = (2.0 / (1.0 + cos_turn)).sqrt();
                if ratio <= self.style.miter_limit && 1.0 + cos_turn > 1e-9 {
                    let n1 = (-side * before.1, side * before.0);
                    let n2 = (-side * after.1, side * after.0);
                    let k = self.half / (1.0 + cos_turn);
                    out.push((at.0 + (n1.0 + n2.0) * k, at.1 + (n1.1 + n2.1) * k));
                } else {
                    self.offset(at, before, side, out);
                    self.offset(at, after, side, out);
                }
            }
            LineJoin::Bevel => {
                self.offset(at, before, side, out);
                self.offset(at, after, side, out);
            }
            LineJoin::Round => {
                let a1 = (side * before.0).atan2(-side * before.1);
                let mut a2 = (side * after.0).atan2(-side * after.1);
                // The short way round, in the direction of the turn.
                let mut delta = a2 - a1;
                while delta > std::f64::consts::PI {
                    delta -= 2.0 * std::f64::consts::PI;
                }
                while delta < -std::f64::consts::PI {
                    delta += 2.0 * std::f64::consts::PI;
                }
                // Turning back on itself, the arc goes round the end.
                if cross.abs() < 1e-12 {
                    delta = -side * std::f64::consts::PI;
                }
                a2 = a1 + delta;
                let pieces = ((a2 - a1).abs() / self.step).ceil().max(1.0) as usize;
                for i in 0..=pieces {
                    let angle = a1 + (a2 - a1) * i as f64 / pieces as f64;
                    out.push((
                        at.0 + self.half * angle.cos(),
                        at.1 + self.half * angle.sin(),
                    ));
                }
            }
        }
    }

    // The end of a line at `at` that goes on in `direction`: from the left
    // side of it to the right.
    fn cap(&self, at: Point, direction: Point, out: &mut Vec<Point>) {
        match self.style.cap {
            LineCap::Butt => {}
            LineCap::Square => {
                let (dx, dy) = (direction.0 * self.half, direction.1 * self.half);
                self.offset(at, direction, 1.0, out);
                let left = *out.last().unwrap_or(&at);
                out.push((left.0 + dx, left.1 + dy));
                self.offset(at, direction, -1.0, out);
                let right = *out.last().unwrap_or(&at);
                out.pop();
                out.push((right.0 + dx, right.1 + dy));
            }
            LineCap::Round => {
                // From the left normal, through the direction, to the right.
                let a1 = direction.0.atan2(-direction.1);
                let pieces = (std::f64::consts::PI / self.step).ceil().max(2.0) as usize;
                for i in 1..pieces {
                    let angle = a1 - std::f64::consts::PI * i as f64 / pieces as f64;
                    out.push((
                        at.0 + self.half * angle.cos(),
                        at.1 + self.half * angle.sin(),
                    ));
                }
            }
        }
    }

    // A subpath that is a single point: a dot with round caps, a square with
    // square caps, and nothing with butt ones.
    fn dot(&self, at: Point, out: &mut Vec<Vec<Point>>) {
        let half = self.half;
        match self.style.cap {
            LineCap::Butt => {}
            LineCap::Square => out.push(vec![
                (at.0 - half, at.1 - half),
                (at.0 + half, at.1 - half),
                (at.0 + half, at.1 + half),
                (at.0 - half, at.1 + half),
            ]),
            LineCap::Round => {
                let pieces = (2.0 * std::f64::consts::PI / self.step).ceil().max(8.0) as usize;
                out.push(
                    (0..pieces)
                        .map(|i| {
                            let angle = 2.0 * std::f64::consts::PI * i as f64 / pieces as f64;
                            (at.0 + half * angle.cos(), at.1 + half * angle.sin())
                        })
                        .collect(),
                );
            }
        }
    }
}
