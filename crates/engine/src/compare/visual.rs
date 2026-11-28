//! How two drawings of a page differ.

use crate::content::Matrix;
use crate::document::{Page, Rectangle};
use crate::render::{Pixmap, page_matrix};

/// Where and how much two drawings of a page differ.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualDifference {
    /// The share of the pixels that differ, from 0 to 1.
    pub fraction: f64,
    /// The places that differ, as rectangles in the default user space of the
    /// page: nearby pixels that differ are one rectangle.
    pub regions: Vec<Rectangle>,
}

// How far a pixel is from another: the most any of its colors differs, over a
// white paper.
fn distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    let over_white = |c: u8, alpha: u8| {
        ((u32::from(c) * u32::from(alpha) + 255 * (255 - u32::from(alpha))) / 255) as i32
    };
    (0..3)
        .map(|i| (over_white(a[i], a[3]) - over_white(b[i], b[3])).unsigned_abs())
        .max()
        .unwrap_or(0) as u8
}

// The size of the squares the pixels that differ are grouped in.
const CELL: usize = 8;

/// How `a` and `b`, two drawings of `page` at `scale`, differ. `None` if they
/// aren't the same size. A pixel differs if any of its colors does by more than
/// `threshold`.
pub fn visual_difference(
    a: &Pixmap,
    b: &Pixmap,
    page: &Page,
    scale: f64,
    threshold: u8,
) -> Option<VisualDifference> {
    if (a.width(), a.height()) != (b.width(), b.height()) || a.width() == 0 || a.height() == 0 {
        return None;
    }
    let (width, height) = (a.width(), a.height());
    let (cols, rows) = (width.div_ceil(CELL), height.div_ceil(CELL));
    let mut cells = vec![false; cols * rows];
    let mut different = 0_usize;
    for y in 0..height {
        for x in 0..width {
            let (Some(pa), Some(pb)) = (a.pixel(x, y), b.pixel(x, y)) else {
                continue;
            };
            if distance(pa, pb) > threshold {
                different += 1;
                cells[(y / CELL) * cols + x / CELL] = true;
            }
        }
    }
    let fraction = different as f64 / (width * height) as f64;
    // Neighboring cells that differ make one region.
    let (matrix, _, _) = page_matrix(page, scale);
    let inverse = matrix.inverse().unwrap_or(Matrix::IDENTITY);
    let mut seen = vec![false; cells.len()];
    let mut regions = Vec::new();
    for start in 0..cells.len() {
        if !cells[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut c0, mut r0, mut c1, mut r1) = (cols, rows, 0, 0);
        while let Some(cell) = stack.pop() {
            let (c, r) = (cell % cols, cell / cols);
            c0 = c0.min(c);
            r0 = r0.min(r);
            c1 = c1.max(c);
            r1 = r1.max(r);
            for (dc, dr) in [
                (-1_i64, -1_i64),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let (nc, nr) = (c as i64 + dc, r as i64 + dr);
                if nc < 0 || nr < 0 || nc >= cols as i64 || nr >= rows as i64 {
                    continue;
                }
                let next = nr as usize * cols + nc as usize;
                if cells[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        let corners = [
            (c0 * CELL, r0 * CELL),
            ((c1 + 1) * CELL, r0 * CELL),
            ((c1 + 1) * CELL, (r1 + 1) * CELL),
            (c0 * CELL, (r1 + 1) * CELL),
        ]
        .map(|(x, y)| inverse.apply(x.min(width) as f64, y.min(height) as f64));
        regions.push(Rectangle::around(&corners));
    }
    Some(VisualDifference { fraction, regions })
}

/// A picture of how `a` and `b`, two drawings of a page, differ: what is in
/// both is a faded gray, what is only in `a` is red and what is only in `b` is
/// green. Rows of RGB, `width` by `height`; `None` if they aren't the same size.
pub fn diff_image(a: &Pixmap, b: &Pixmap, threshold: u8) -> Option<(usize, usize, Vec<u8>)> {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return None;
    }
    let (width, height) = (a.width(), a.height());
    let over_white = |c: u8, alpha: u8| {
        ((u32::from(c) * u32::from(alpha) + 255 * (255 - u32::from(alpha))) / 255) as u8
    };
    let gray = |p: [u8; 4]| {
        let value = (u32::from(over_white(p[0], p[3])) * 299
            + u32::from(over_white(p[1], p[3])) * 587
            + u32::from(over_white(p[2], p[3])) * 114)
            / 1000;
        value as u8
    };
    let mut out = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let (Some(pa), Some(pb)) = (a.pixel(x, y), b.pixel(x, y)) else {
                out.extend([255, 255, 255]);
                continue;
            };
            let (ga, gb) = (gray(pa), gray(pb));
            if distance(pa, pb) > threshold {
                // Darker means there is ink.
                if ga < gb {
                    out.extend([220, 40, 40]);
                } else {
                    out.extend([30, 160, 60]);
                }
            } else {
                // Faded, so what changed stands out.
                let faded = 255 - (255 - gb) / 3;
                out.extend([faded, faded, faded]);
            }
        }
    }
    Some((width, height, out))
}
