use super::scan::Component;

/// The cosines of the inverse DCT: `COS[k]` is cos(k pi / 16).
fn cosines() -> [f32; 8] {
    let mut c = [0f32; 8];
    for (k, value) in c.iter_mut().enumerate() {
        *value = (k as f64 * std::f64::consts::PI / 16.0).cos() as f32;
    }
    c
}

/// The inverse DCT of one row or column of 8 values (ITU-T T.81, A.3.3),
/// split into its even and odd frequencies.
fn idct_1d(x: [f32; 8], c: &[f32; 8]) -> [f32; 8] {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let e0 = (x[0] + x[4]) * half;
    let e1 = (x[0] - x[4]) * half;
    let a = x[2] * c[2] + x[6] * c[6];
    let b = x[2] * c[6] - x[6] * c[2];
    let even = [e0 + a, e1 + b, e1 - b, e0 - a];
    // The odd part: output n takes cos((2n+1) k pi / 16) of each odd k.
    let odd = [
        x[1] * c[1] + x[3] * c[3] + x[5] * c[5] + x[7] * c[7],
        x[1] * c[3] - x[3] * c[7] - x[5] * c[1] - x[7] * c[5],
        x[1] * c[5] - x[3] * c[1] + x[5] * c[7] + x[7] * c[3],
        x[1] * c[7] - x[3] * c[5] + x[5] * c[3] - x[7] * c[1],
    ];
    [
        even[0] + odd[0],
        even[1] + odd[1],
        even[2] + odd[2],
        even[3] + odd[3],
        even[3] - odd[3],
        even[2] - odd[2],
        even[1] - odd[1],
        even[0] - odd[0],
    ]
}

/// The samples of one component, 8 bits each, from its coefficients and its
/// quantization table (in rows): `stride * 8` wide and `rows * 8` high.
pub(super) fn samples(component: &Component, quantization: &[u16; 64]) -> Vec<u8> {
    let c = cosines();
    let width = component.stride * 8;
    let mut out = vec![0u8; width * component.rows * 8];
    for by in 0..component.rows {
        for bx in 0..component.stride {
            let block = &component.coefficients[(by * component.stride + bx) * 64..][..64];
            let mut values = [0f32; 64];
            for i in 0..64 {
                values[i] = f32::from(block[i]) * f32::from(quantization[i]);
            }
            // Only the DC coefficient: a flat block.
            if block[1..].iter().all(|&v| v == 0) {
                let flat = (values[0] / 8.0 + 128.0).round().clamp(0.0, 255.0) as u8;
                for y in 0..8 {
                    let row = (by * 8 + y) * width + bx * 8;
                    out[row..row + 8].fill(flat);
                }
                continue;
            }
            // Rows, then columns, each with the 1/2 of the 2-D transform.
            let mut rows = [0f32; 64];
            for y in 0..8 {
                let mut x = [0f32; 8];
                x.copy_from_slice(&values[y * 8..y * 8 + 8]);
                let r = idct_1d(x, &c);
                for i in 0..8 {
                    rows[y * 8 + i] = r[i] * 0.5;
                }
            }
            for xi in 0..8 {
                let mut column = [0f32; 8];
                for y in 0..8 {
                    column[y] = rows[y * 8 + xi];
                }
                let r = idct_1d(column, &c);
                for y in 0..8 {
                    out[(by * 8 + y) * width + bx * 8 + xi] =
                        (r[y] * 0.5 + 128.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Like [`samples`], for a picture `n` eighths of its size (1, 2 or 4): each
/// block of coefficients gives `n` by `n` samples, from an `n` point inverse
/// DCT of its lowest frequencies (their scale in an eighth of the points is
/// `n / 8` in each direction).
pub(super) fn samples_scaled(component: &Component, quantization: &[u16; 64], n: usize) -> Vec<u8> {
    // basis[k][u]: the weight of frequency `u` at sample `k`.
    let mut basis = [[0f32; 4]; 4];
    for (k, row) in basis.iter_mut().enumerate().take(n) {
        for (u, weight) in row.iter_mut().enumerate().take(n) {
            let alpha = if u == 0 {
                (1.0 / n as f64).sqrt()
            } else {
                (2.0 / n as f64).sqrt()
            };
            let angle = (2 * k + 1) as f64 * u as f64 * std::f64::consts::PI / (2 * n) as f64;
            *weight = (alpha * angle.cos()) as f32;
        }
    }
    let scale = n as f32 / 8.0;
    let width = component.stride * n;
    let mut out = vec![0u8; width * component.rows * n];
    for by in 0..component.rows {
        for bx in 0..component.stride {
            let block = &component.coefficients[(by * component.stride + bx) * 64..][..64];
            let mut values = [[0f32; 4]; 4];
            for (v, row) in values.iter_mut().enumerate().take(n) {
                for (u, value) in row.iter_mut().enumerate().take(n) {
                    *value =
                        f32::from(block[v * 8 + u]) * f32::from(quantization[v * 8 + u]) * scale;
                }
            }
            // Along each row of frequencies, then down each column.
            let mut across = [[0f32; 4]; 4];
            for v in 0..n {
                for k in 0..n {
                    across[v][k] = (0..n).map(|u| basis[k][u] * values[v][u]).sum();
                }
            }
            for j in 0..n {
                for k in 0..n {
                    let value: f32 = (0..n).map(|v| basis[j][v] * across[v][k]).sum();
                    out[(by * n + j) * width + bx * n + k] =
                        (value + 128.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// A component's samples, `width` by `height`, made `factor_x` and `factor_y`
/// times larger: by the triangle filter where it is doubled, and by repeating
/// samples otherwise.
pub(super) fn upsample(
    plane: &[u8],
    stride: usize,
    size: (usize, usize),
    factors: (usize, usize),
    to: (usize, usize),
) -> Vec<u8> {
    let (width, height) = size;
    let (fx, fy) = factors;
    let (out_width, out_height) = to;
    let mut out = vec![0u8; out_width * out_height];
    if fx == 1 && fy == 1 {
        for y in 0..out_height {
            out[y * out_width..(y + 1) * out_width]
                .copy_from_slice(&plane[y * stride..y * stride + out_width]);
        }
        return out;
    }
    // Vertically first, in 4ths where doubled: each row is 3/4 the nearer
    // source row and 1/4 the farther one.
    let at = |x: usize, y: usize| u32::from(plane[y.min(height - 1) * stride + x.min(width - 1)]);
    // Very narrow components are repeated instead, as libjpeg does.
    let smooth = !(fx == 2 && width <= 2);
    let (smooth_x, smooth_y) = (fx == 2 && smooth, fy == 2 && smooth);
    let weight_y = if smooth_y { 4 } else { 1 };
    let weight_x = if smooth_x { 4 } else { 1 };
    let mut rows = vec![0u32; width * out_height];
    for y in 0..out_height {
        let near = y / fy;
        let far = if y % 2 == 0 {
            near.saturating_sub(1)
        } else {
            near + 1
        };
        for x in 0..width {
            rows[y * width + x] = if smooth_y {
                3 * at(x, near) + at(x, far)
            } else {
                at(x, near)
            };
        }
    }
    for y in 0..out_height {
        for x in 0..out_width {
            let source = x / fx;
            let value = if smooth_x {
                let far = if x % 2 == 0 {
                    source.saturating_sub(1)
                } else {
                    (source + 1).min(width - 1)
                };
                3 * rows[y * width + source] + rows[y * width + far]
            } else {
                rows[y * width + source]
            };
            let scale = weight_x * weight_y;
            out[y * out_width + x] = ((value + scale / 2) / scale).min(255) as u8;
        }
    }
    out
}

/// YCbCr to RGB, in place (ITU-T T.871): the same for the first three of a
/// pixel, whatever follows.
pub(super) fn ycc_to_rgb(pixel: &mut [u8]) {
    let y = i32::from(pixel[0]) << 16;
    let cb = i32::from(pixel[1]) - 128;
    let cr = i32::from(pixel[2]) - 128;
    let r = (y + 91881 * cr + 32768) >> 16;
    let g = (y - 22554 * cb - 46802 * cr + 32768) >> 16;
    let b = (y + 116130 * cb + 32768) >> 16;
    pixel[0] = r.clamp(0, 255) as u8;
    pixel[1] = g.clamp(0, 255) as u8;
    pixel[2] = b.clamp(0, 255) as u8;
}
