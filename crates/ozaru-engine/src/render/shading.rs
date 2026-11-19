use super::blend::BlendMode;
use super::canvas::{Clip, Pixmap};
use super::raster::{IRect, Mask};
use crate::base::Limits;
use crate::content::Matrix;
use crate::functions::{Function, read_function};
use crate::images::{ColorSpace, read_color_space};
use crate::syntax::Object;
use crate::xref::Resolver;

/// How many colors the function of a shading is sampled at.
const SAMPLES: usize = 256;

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    /// A function of the position (type 1): its domain, and the matrix from
    /// its space to the shading's.
    Function {
        domain: [f64; 4],
        inverse: Matrix,
        functions: Vec<Function>,
        space: ColorSpace,
    },
    /// Colors along a line (type 2).
    Axial {
        coords: [f64; 4],
        extend: (bool, bool),
        colors: Vec<[u8; 3]>,
    },
    /// Colors between two circles (type 3).
    Radial {
        coords: [f64; 6],
        extend: (bool, bool),
        colors: Vec<[u8; 3]>,
    },
}

/// A shading (ISO 32000-2, section 8.7.4), read to give the color at any point
/// of its space.
#[derive(Debug, Clone, PartialEq)]
pub struct Shading {
    kind: Kind,
    bbox: Option<[f64; 4]>,
    /// The color painted where the shading gives none, for shading patterns.
    pub background: Option<[u8; 3]>,
}

fn numbers(entry: Option<&Object>, resolver: &mut Resolver<'_>) -> Option<Vec<f64>> {
    let object = match entry? {
        Object::Reference(id) => resolver.resolve(*id).ok()?.as_ref().clone(),
        other => other.clone(),
    };
    Some(
        object
            .as_array()?
            .iter()
            .filter_map(Object::as_number)
            .collect(),
    )
}

/// Reads the shading `entry`: types 1, 2 and 3. `None` for the others, meshes
/// that are not read yet, and for shadings that are damaged.
pub fn read_shading(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    limits: Limits,
) -> Option<Shading> {
    let object = match entry {
        Object::Reference(id) => resolver.resolve(*id).ok()?.as_ref().clone(),
        other => other.clone(),
    };
    let dictionary = object.as_dictionary()?.clone();
    let kind = dictionary.get("ShadingType")?.as_integer()?;
    let space_entry = dictionary.get("ColorSpace")?;
    let space = read_color_space(resolver, space_entry)?;
    if matches!(space, ColorSpace::Unsupported) {
        return None;
    }
    let function_entry = dictionary.get("Function");
    let functions: Vec<Function> = match function_entry {
        None => Vec::new(),
        Some(entry) => {
            let resolved = match entry {
                Object::Reference(id) => resolver.resolve(*id).ok()?.as_ref().clone(),
                other => other.clone(),
            };
            match resolved {
                Object::Array(items) => items
                    .iter()
                    .map(|item| read_function(resolver, item, limits).ok())
                    .collect::<Option<_>>()?,
                _ => vec![read_function(resolver, entry, limits).ok()?],
            }
        }
    };
    let bbox = numbers(dictionary.get("BBox"), resolver)
        .filter(|b| b.len() == 4)
        .map(|b| {
            [
                b[0].min(b[2]),
                b[1].min(b[3]),
                b[0].max(b[2]),
                b[1].max(b[3]),
            ]
        });
    let background = numbers(dictionary.get("Background"), resolver)
        .filter(|b| b.len() == space.components())
        .map(|b| space.rgb(&b));
    let extend = || {
        let flags = dictionary.get("Extend").and_then(Object::as_array);
        let flag = |i: usize| {
            flags
                .and_then(|f| f.get(i))
                .is_some_and(|v| matches!(v, Object::Boolean(true)))
        };
        (flag(0), flag(1))
    };
    let kind = match kind {
        1 => {
            let domain = numbers(dictionary.get("Domain"), resolver)
                .filter(|d| d.len() == 4)
                .map_or([0.0, 1.0, 0.0, 1.0], |d| [d[0], d[1], d[2], d[3]]);
            let matrix = numbers(dictionary.get("Matrix"), resolver)
                .filter(|m| m.len() == 6)
                .map_or(Matrix::IDENTITY, |m| {
                    Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5])
                });
            if functions.is_empty() {
                return None;
            }
            Kind::Function {
                domain,
                inverse: invert(&matrix)?,
                functions,
                space,
            }
        }
        2 | 3 => {
            let coords = numbers(dictionary.get("Coords"), resolver)?;
            let domain = numbers(dictionary.get("Domain"), resolver)
                .filter(|d| d.len() == 2)
                .map_or([0.0, 1.0], |d| [d[0], d[1]]);
            let colors = sample_colors(&functions, &space, domain)?;
            if kind == 2 {
                let c: [f64; 4] = coords.try_into().ok()?;
                Kind::Axial {
                    coords: c,
                    extend: extend(),
                    colors,
                }
            } else {
                let c: [f64; 6] = coords.try_into().ok()?;
                Kind::Radial {
                    coords: c,
                    extend: extend(),
                    colors,
                }
            }
        }
        _ => return None,
    };
    Some(Shading {
        kind,
        bbox,
        background,
    })
}

fn invert(m: &Matrix) -> Option<Matrix> {
    let det = m.a * m.d - m.b * m.c;
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    Some(Matrix::new(
        m.d / det,
        -m.b / det,
        -m.c / det,
        m.a / det,
        (m.c * m.f - m.d * m.e) / det,
        (m.b * m.e - m.a * m.f) / det,
    ))
}

// The colors of the functions at `SAMPLES` points of the domain.
fn sample_colors(
    functions: &[Function],
    space: &ColorSpace,
    domain: [f64; 2],
) -> Option<Vec<[u8; 3]>> {
    if functions.is_empty() {
        return None;
    }
    let mut colors = Vec::with_capacity(SAMPLES);
    for i in 0..SAMPLES {
        let t = domain[0] + (domain[1] - domain[0]) * i as f64 / (SAMPLES - 1) as f64;
        colors.push(space.rgb(&evaluate(functions, &[t])));
    }
    Some(colors)
}

// The values of one function with several outputs, or of several with one.
fn evaluate(functions: &[Function], input: &[f64]) -> Vec<f64> {
    if functions.len() == 1 {
        functions[0].evaluate(input)
    } else {
        functions
            .iter()
            .map(|f| f.evaluate(input).first().copied().unwrap_or(0.0))
            .collect()
    }
}

impl Shading {
    /// The color at the point `(x, y)` of the shading's space, `None` where it
    /// paints nothing.
    pub fn color_at(&self, x: f64, y: f64) -> Option<[u8; 3]> {
        if let Some(b) = &self.bbox
            && (x < b[0] || x > b[2] || y < b[1] || y > b[3])
        {
            return None;
        }
        match &self.kind {
            Kind::Function {
                domain,
                inverse,
                functions,
                space,
            } => {
                let (u, v) = inverse.apply(x, y);
                if u < domain[0] || u > domain[1] || v < domain[2] || v > domain[3] {
                    return None;
                }
                Some(space.rgb(&evaluate(functions, &[u, v])))
            }
            Kind::Axial {
                coords,
                extend,
                colors,
            } => {
                let (dx, dy) = (coords[2] - coords[0], coords[3] - coords[1]);
                let length = dx * dx + dy * dy;
                let s = if length == 0.0 {
                    return None;
                } else {
                    ((x - coords[0]) * dx + (y - coords[1]) * dy) / length
                };
                pick(colors, s, *extend)
            }
            Kind::Radial {
                coords,
                extend,
                colors,
            } => {
                let [x0, y0, r0, x1, y1, r1] = *coords;
                let (dx, dy, dr) = (x1 - x0, y1 - y0, r1 - r0);
                let (px, py) = (x - x0, y - y0);
                let a = dx * dx + dy * dy - dr * dr;
                let b = px * dx + py * dy + r0 * dr;
                let c = px * px + py * py - r0 * r0;
                // The largest s whose circle passes through the point and has
                // a radius that isn't negative, inside 0 to 1 or extended.
                let mut candidates = [None, None];
                if a.abs() < 1e-12 {
                    if b.abs() > 1e-12 {
                        candidates[0] = Some(c / (2.0 * b));
                    }
                } else {
                    let discriminant = b * b - a * c;
                    if discriminant < 0.0 {
                        return None;
                    }
                    let root = discriminant.sqrt();
                    candidates = [Some((b + root) / a), Some((b - root) / a)];
                }
                let mut best: Option<f64> = None;
                for s in candidates.into_iter().flatten() {
                    let usable =
                        r0 + s * dr >= 0.0 && (s >= 0.0 || extend.0) && (s <= 1.0 || extend.1);
                    if usable && best.is_none_or(|b| s > b) {
                        best = Some(s);
                    }
                }
                pick(colors, best?, *extend)
            }
        }
    }
}

// The color at `s` along the sampled colors, from 0 to 1; outside that, the
// end color if the shading extends, and nothing if not.
fn pick(colors: &[[u8; 3]], s: f64, extend: (bool, bool)) -> Option<[u8; 3]> {
    if !s.is_finite() {
        return None;
    }
    if s < 0.0 {
        return extend.0.then(|| colors[0]);
    }
    if s > 1.0 {
        return extend.1.then(|| colors[colors.len() - 1]);
    }
    let position = s * (colors.len() - 1) as f64;
    let (i, f) = (position.floor() as usize, position.fract());
    let (a, b) = (colors[i], colors[(i + 1).min(colors.len() - 1)]);
    Some([0, 1, 2].map(|c| (f64::from(a[c]) * (1.0 - f) + f64::from(b[c]) * f).round() as u8))
}

/// Paints `shading` where `mask` covers (the whole clip if `None`), inside
/// `clip`. `to_shading` maps device pixels to the shading's space.
#[allow(clippy::too_many_arguments)]
pub fn paint_shading(
    pixmap: &mut Pixmap,
    clip: &Clip,
    mask: Option<&Mask>,
    shading: &Shading,
    to_shading: &Matrix,
    opacity: u8,
    mode: BlendMode,
    background: bool,
) {
    let area = match mask {
        Some(mask) => mask.rect,
        None => clip.rect,
    }
    .intersect(&clip.rect)
    .intersect(&pixmap.rect());
    let area: IRect = area;
    for y in area.y0..area.y1 {
        for x in area.x0..area.x1 {
            let coverage = mask.map_or(255, |m| m.at(x, y));
            if coverage == 0 {
                continue;
            }
            let (sx, sy) = to_shading.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let color =
                shading
                    .color_at(sx, sy)
                    .or(if background { shading.background } else { None });
            let Some(color) = color else { continue };
            let alpha = (u32::from(coverage) * u32::from(opacity) / 255) as u8;
            pixmap.blend_with(x, y, color, alpha, clip, mode);
        }
    }
}
