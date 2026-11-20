use super::path::Contour;

/// A rectangle of pixels: `x0` and `y0` are in it, `x1` and `y1` aren't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn width(&self) -> usize {
        (self.x1 - self.x0).max(0) as usize
    }

    pub fn height(&self) -> usize {
        (self.y1 - self.y0).max(0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub fn intersect(&self, other: &Self) -> Self {
        Self::new(
            self.x0.max(other.x0),
            self.y0.max(other.y0),
            self.x1.min(other.x1),
            self.y1.min(other.y1),
        )
    }
}

/// Which parts of a path are inside it (ISO 32000-2, section 8.5.3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    /// A point is inside if the path winds around it.
    NonZero,
    /// A point is inside if a ray from it crosses the path an odd number of
    /// times.
    EvenOdd,
}

/// How much of each pixel of a rectangle a shape covers, from 0 to 255.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub rect: IRect,
    /// The rows from the top, `rect.width()` coverages each.
    pub data: Vec<u8>,
}

impl Mask {
    /// The coverage of the pixel at `x` and `y`, which is 0 outside `rect`.
    pub fn at(&self, x: i32, y: i32) -> u8 {
        if x < self.rect.x0 || x >= self.rect.x1 || y < self.rect.y0 || y >= self.rect.y1 {
            return 0;
        }
        let row = (y - self.rect.y0) as usize;
        self.data[row * self.rect.width() + (x - self.rect.x0) as usize]
    }
}

/// How much of each pixel along one axis the span from `low` to `high`
/// covers, from 0 to 255, for the pixels `from` to `to` (not included).
fn span_coverage(low: f64, high: f64, from: i32, to: i32) -> Vec<u8> {
    (from..to)
        .map(|i| {
            let overlap = (high.min(f64::from(i) + 1.0) - low.max(f64::from(i))).clamp(0.0, 1.0);
            (overlap * 255.0 + 0.5) as u8
        })
        .collect()
}

/// The coverage of the rectangle from `(x0, y0)` to `(x1, y1)`, with sides
/// along the axes, inside `limit`: the same as `fill_contours` gives for it,
/// without sweeping edges. `None` if it covers no pixel of `limit`.
pub fn rect_mask(x0: f64, y0: f64, x1: f64, y1: f64, limit: &IRect) -> Option<Mask> {
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let clamp = |v: f64| v.clamp(-1e9, 1e9);
    let rect = IRect::new(
        clamp(x0.floor()) as i32,
        clamp(y0.floor()) as i32,
        clamp(x1.ceil()) as i32,
        clamp(y1.ceil()) as i32,
    )
    .intersect(limit);
    if rect.is_empty() {
        return None;
    }
    let columns = span_coverage(x0, x1, rect.x0, rect.x1);
    let rows = span_coverage(y0, y1, rect.y0, rect.y1);
    let width = rect.width();
    let mut data = Vec::with_capacity(width * rect.height());
    let mut previous: Option<(u8, usize)> = None;
    for &row in &rows {
        match previous {
            // Rows of the same coverage are copies of each other.
            Some((value, start)) if value == row => {
                data.extend_from_within(start..start + width);
            }
            _ => {
                let start = data.len();
                data.extend(
                    columns
                        .iter()
                        .map(|&c| ((u32::from(c) * u32::from(row) + 127) / 255) as u8),
                );
                previous = Some((row, start));
            }
        }
    }
    Some(Mask { rect, data })
}

/// The pixels the points of `contours` lie in, and the rectangle `limit` cuts
/// them to. `None` if there is none.
fn bounds(contours: &[Contour], limit: &IRect) -> Option<IRect> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for point in contours.iter().flat_map(|c| &c.points) {
        x0 = x0.min(point.0);
        y0 = y0.min(point.1);
        x1 = x1.max(point.0);
        y1 = y1.max(point.1);
    }
    if x0 > x1 {
        return None;
    }
    let clamp = |v: f64| v.clamp(-1e9, 1e9);
    let rect = IRect::new(
        clamp(x0.floor()) as i32,
        clamp(y0.floor()) as i32,
        clamp(x1.ceil()) as i32,
        clamp(y1.ceil()) as i32,
    )
    .intersect(limit);
    (!rect.is_empty()).then_some(rect)
}

/// Rasterizes the shape that `contours` make, each closed with a line to its
/// start, inside `limit`, with anti-aliasing: the coverage of every pixel is
/// the area of it inside the shape. `None` if the shape covers no pixel of
/// `limit`.
///
/// For each edge, the signed area it leaves to its right is added to the
/// pixels it crosses, as a difference with the pixel before it; summing a
/// row from the left then gives the winding number weighted by area at every
/// pixel, which the fill rule turns into coverage.
pub fn fill_contours(contours: &[Contour], rule: FillRule, limit: &IRect) -> Option<Mask> {
    let rect = bounds(contours, limit)?;
    let (width, height) = (rect.width(), rect.height());
    let stride = width + 2;
    let mut accumulated = vec![0f32; stride * height];
    let (ox, oy) = (f64::from(rect.x0), f64::from(rect.y0));
    for contour in contours {
        let points = &contour.points;
        if points.len() < 2 {
            continue;
        }
        let mut previous = points[points.len() - 1];
        for &point in points {
            add_edge(
                &mut accumulated,
                stride,
                (width as f64, height as f64),
                (previous.0 - ox, previous.1 - oy),
                (point.0 - ox, point.1 - oy),
            );
            previous = point;
        }
    }
    let mut data = vec![0u8; width * height];
    for y in 0..height {
        let row = &accumulated[y * stride..y * stride + width];
        let mut sum = 0f32;
        for (x, &delta) in row.iter().enumerate() {
            sum += delta;
            let coverage = match rule {
                FillRule::NonZero => sum.abs().min(1.0),
                FillRule::EvenOdd => {
                    let t = sum.abs() % 2.0;
                    if t > 1.0 { 2.0 - t } else { t }
                }
            };
            data[y * width + x] = (coverage * 255.0 + 0.5) as u8;
        }
    }
    Some(Mask { rect, data })
}

/// Adds the area an edge leaves to its right to `accumulated`, rows of
/// `stride` cells, `size` wide and high.
fn add_edge(
    accumulated: &mut [f32],
    stride: usize,
    size: (f64, f64),
    from: (f64, f64),
    to: (f64, f64),
) {
    let (width, height) = size;
    let (mut x0, mut y0, mut x1, mut y1) = (from.0, from.1, to.0, to.1);
    if y0 == y1 {
        return;
    }
    let direction = if y0 < y1 {
        1.0
    } else {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
        -1.0
    };
    if y1 <= 0.0 || y0 >= height {
        return;
    }
    let slope = (x1 - x0) / (y1 - y0);
    let top = y0.max(0.0);
    let bottom = y1.min(height);
    let first = top.floor() as usize;
    let last = bottom.ceil() as usize;
    for row in first..last {
        let ya = top.max(row as f64);
        let yb = bottom.min(row as f64 + 1.0);
        let dy = yb - ya;
        if dy <= 0.0 {
            continue;
        }
        // An edge left or right of the rectangle keeps its place in the
        // rows, as the edge of the rectangle.
        let xa = (x0 + (ya - y0) * slope).clamp(0.0, width);
        let xb = (x0 + (yb - y0) * slope).clamp(0.0, width);
        let d = (dy * direction) as f32;
        let (left, right) = if xa < xb { (xa, xb) } else { (xb, xa) };
        let left_cell = left.floor() as usize;
        let right_cell = right.ceil() as usize;
        let cells = &mut accumulated[row * stride..(row + 1) * stride];
        if right_cell <= left_cell + 1 {
            // Inside one pixel: its two sides share the area by where the
            // edge is, on average.
            let middle = (0.5 * (xa + xb) - left_cell as f64) as f32;
            cells[left_cell] += d - d * middle;
            cells[left_cell + 1] += d * middle;
        } else {
            let s = (1.0 / (right - left)) as f32;
            let left_part = (left - left_cell as f64) as f32;
            let first_area = 0.5 * s * (1.0 - left_part) * (1.0 - left_part);
            let right_part = (right - right_cell as f64 + 1.0) as f32;
            let last_area = 0.5 * s * right_part * right_part;
            cells[left_cell] += d * first_area;
            if right_cell == left_cell + 2 {
                cells[left_cell + 1] += d * (1.0 - first_area - last_area);
            } else {
                let second_area = s * (1.5 - left_part);
                cells[left_cell + 1] += d * (second_area - first_area);
                for cell in &mut cells[left_cell + 2..right_cell - 1] {
                    *cell += d * s;
                }
                let before_last = second_area + (right_cell - left_cell - 3) as f32 * s;
                cells[right_cell - 1] += d * (1.0 - before_last - last_area);
            }
            cells[right_cell] += d * last_area;
        }
    }
}
