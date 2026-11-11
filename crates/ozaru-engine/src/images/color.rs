//! The color spaces of images, and their pixels in gray or RGB (ISO 32000-2,
//! section 8.6), to write them as PNG files.

use super::description::ImageDescription;
use super::png::PngColor;
use crate::syntax::Object;
use crate::xref::Resolver;

/// The color space of an image, read.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpace {
    /// `/DeviceGray` or `/CalGray`, or an ICC profile of one component.
    Gray,
    /// `/DeviceRGB` or `/CalRGB`, or an ICC profile of three components.
    Rgb,
    /// `/DeviceCMYK`, or an ICC profile of four components.
    Cmyk,
    /// `/Lab`, with its white point and the range of its a* and b*.
    Lab { white: [f64; 3], range: [f64; 4] },
    /// `/Indexed`: a palette of up to 256 colors in `base`.
    Indexed {
        base: Box<ColorSpace>,
        high: u8,
        lookup: Vec<u8>,
    },
    /// `/Separation`, `/DeviceN` and others, whose colors need functions.
    Unsupported,
}

impl ColorSpace {
    /// How many components each pixel has.
    pub fn components(&self) -> usize {
        match self {
            Self::Gray | Self::Indexed { .. } | Self::Unsupported => 1,
            Self::Rgb | Self::Lab { .. } => 3,
            Self::Cmyk => 4,
        }
    }
}

// How deep color spaces can be in each other: an `/Indexed` base, or the
// `/Alternate` of an ICC profile.
const MAX_DEPTH: usize = 4;

/// Reads the color space `entry` of an image, `None` if it isn't one.
/// References inside it, such as an ICC profile or the lookup table of a
/// palette, are resolved with `resolver`.
pub fn read_color_space(resolver: &mut Resolver<'_>, entry: &Object) -> Option<ColorSpace> {
    read(resolver, entry, 0)
}

fn read(resolver: &mut Resolver<'_>, entry: &Object, depth: usize) -> Option<ColorSpace> {
    if depth > MAX_DEPTH {
        return None;
    }
    let entry = resolved(resolver, entry)?;
    let (family, rest): (&[u8], &[Object]) = match &entry {
        Object::Name(name) => (name.as_bytes(), &[]),
        Object::Array(items) => (items.first()?.as_name()?.as_bytes(), &items[1..]),
        _ => return None,
    };
    Some(match family {
        b"DeviceGray" | b"CalGray" | b"G" => ColorSpace::Gray,
        b"DeviceRGB" | b"CalRGB" | b"RGB" => ColorSpace::Rgb,
        b"DeviceCMYK" | b"CMYK" => ColorSpace::Cmyk,
        b"ICCBased" => {
            let id = rest.first()?.as_reference()?;
            let profile = resolver.resolve(id).ok()?;
            let dictionary = profile.as_stream()?.dictionary().clone();
            let by_count = |n: i64| match n {
                1 => Some(ColorSpace::Gray),
                3 => Some(ColorSpace::Rgb),
                4 => Some(ColorSpace::Cmyk),
                _ => None,
            };
            let count = dictionary
                .get("N")
                .and_then(|n| resolved(resolver, n))
                .and_then(|n| n.as_integer());
            match count.and_then(by_count) {
                Some(space) => space,
                None => read(resolver, dictionary.get("Alternate")?, depth + 1)?,
            }
        }
        b"Lab" => {
            let parameters = rest
                .first()
                .and_then(|p| resolved(resolver, p))
                .and_then(|p| p.as_dictionary().cloned())
                .unwrap_or_default();
            let numbers = |key: &str, default: &[f64]| -> Vec<f64> {
                parameters
                    .get(key)
                    .and_then(Object::as_array)
                    .map(|items| items.iter().filter_map(Object::as_number).collect())
                    .filter(|values: &Vec<f64>| values.len() == default.len())
                    .unwrap_or_else(|| default.to_vec())
            };
            let white = numbers("WhitePoint", &[0.9505, 1.0, 1.089]);
            let range = numbers("Range", &[-100.0, 100.0, -100.0, 100.0]);
            ColorSpace::Lab {
                white: [white[0], white[1], white[2]],
                range: [range[0], range[1], range[2], range[3]],
            }
        }
        b"Indexed" | b"I" => {
            let base = read(resolver, rest.first()?, depth + 1)?;
            if matches!(base, ColorSpace::Indexed { .. } | ColorSpace::Unsupported) {
                return Some(ColorSpace::Unsupported);
            }
            let high = resolved(resolver, rest.get(1)?)?.as_integer()?;
            let high = u8::try_from(high).ok()?;
            let lookup = match rest.get(2)? {
                Object::String(table) => table.as_bytes().to_vec(),
                Object::Reference(id) => match resolver.resolve(*id).ok()?.as_ref() {
                    Object::String(table) => table.as_bytes().to_vec(),
                    Object::Stream(_) => resolver.stream_data(*id).ok()?.data,
                    _ => return None,
                },
                _ => return None,
            };
            ColorSpace::Indexed {
                base: Box::new(base),
                high,
                lookup,
            }
        }
        _ => ColorSpace::Unsupported,
    })
}

fn resolved(resolver: &mut Resolver<'_>, entry: &Object) -> Option<Object> {
    match entry {
        Object::Reference(id) => resolver.resolve(*id).ok().map(|o| (*o).clone()),
        other => Some(other.clone()),
    }
}

/// The pixels of an image, 8 bits for each sample, in gray or RGB: its
/// samples of `bits_per_component` bits, rows starting on a byte, mapped with
/// its `/Decode` and turned into gray or RGB. `None` for color spaces whose
/// colors need functions. Missing samples are 0.
///
/// A stencil mask is black where it paints and white elsewhere. CMYK is
/// turned into RGB with the formula of section 10.4.2, without a color
/// profile, and ICC profiles are read as the device space of as many
/// components.
pub fn pixels(
    description: &ImageDescription,
    colors: Option<&ColorSpace>,
    data: &[u8],
) -> Option<(PngColor, Vec<u8>)> {
    let bits = u32::from(description.bits_per_component);
    let max = f64::from((1u32 << bits) - 1);
    let (space, components) = if description.image_mask {
        (None, 1)
    } else {
        let space = colors?;
        if matches!(space, ColorSpace::Unsupported) {
            return None;
        }
        (Some(space), space.components())
    };
    // The range each raw value maps to, by component.
    let default: Vec<f64> = match space {
        None => vec![0.0, 1.0],
        Some(ColorSpace::Indexed { .. }) => vec![0.0, max],
        Some(ColorSpace::Lab { range, .. }) => {
            vec![0.0, 100.0, range[0], range[1], range[2], range[3]]
        }
        Some(_) => [0.0, 1.0].repeat(components),
    };
    let decode = match &description.decode {
        Some(decode) if decode.len() == 2 * components => decode.clone(),
        _ => default,
    };

    let width = description.width as usize;
    let height = description.height as usize;
    let row_bytes = (width * components * bits as usize).div_ceil(8);
    let output = match space {
        None | Some(ColorSpace::Gray) => PngColor::Gray,
        Some(ColorSpace::Indexed { base, .. }) if **base == ColorSpace::Gray => PngColor::Gray,
        Some(_) => PngColor::Rgb,
    };
    let samples_out = if output == PngColor::Gray { 1 } else { 3 };
    let mut out = Vec::with_capacity(width * height * samples_out);

    // Gray and RGB of 8 bits, as they are: the rows, without their padding.
    let plain = decode.chunks(2).all(|pair| pair == [0.0, 1.0]);
    if bits == 8 && plain && matches!(space, Some(ColorSpace::Gray | ColorSpace::Rgb)) {
        let row = width * components;
        for y in 0..height {
            let start = (y * row_bytes).min(data.len());
            let end = (start + row).min(data.len());
            out.extend_from_slice(&data[start..end]);
            out.resize(out.len() + row - (end - start), 0);
        }
        return Some((output, out));
    }

    // The value of each raw sample of each component, looked up once for the
    // samples of up to 8 bits.
    let value = |c: usize, raw: u32| {
        let (low, high) = (decode[2 * c], decode[2 * c + 1]);
        low + f64::from(raw) * (high - low) / max
    };
    let table: Option<Vec<Vec<f64>>> = (bits <= 8).then(|| {
        (0..components)
            .map(|c| (0..=max as u32).map(|raw| value(c, raw)).collect())
            .collect()
    });
    // The colors of a palette, as gray or RGB bytes, made once.
    let palette: Option<Vec<u8>> = match space {
        Some(ColorSpace::Indexed { high, .. }) => {
            let mut colors = Vec::with_capacity((usize::from(*high) + 1) * samples_out);
            for index in 0..=*high {
                push_color(&mut colors, space.expect("indexed"), &[f64::from(index)]);
            }
            Some(colors)
        }
        _ => None,
    };
    let mut values = vec![0.0; components];
    for y in 0..height {
        let row = data.get(y * row_bytes..).unwrap_or(&[]);
        for x in 0..width {
            for (c, slot) in values.iter_mut().enumerate() {
                let raw = sample(row, (x * components + c) * bits as usize, bits);
                *slot = match &table {
                    Some(table) => table[c][raw as usize],
                    None => value(c, raw),
                };
            }
            match (space, &palette) {
                (None, _) => out.push(if values[0] < 0.5 { 0 } else { 255 }),
                (Some(ColorSpace::Indexed { high, .. }), Some(colors)) => {
                    let index = values[0].round().clamp(0.0, f64::from(*high)) as usize;
                    out.extend_from_slice(&colors[index * samples_out..(index + 1) * samples_out]);
                }
                (Some(space), _) => push_color(&mut out, space, &values),
            }
        }
    }
    Some((output, out))
}

// The bits of the sample that starts at bit `at` of `row`, 0 past its end.
fn sample(row: &[u8], at: usize, bits: u32) -> u32 {
    let byte = |i: usize| u32::from(row.get(i).copied().unwrap_or(0));
    match bits {
        8 => byte(at / 8),
        16 => byte(at / 8) << 8 | byte(at / 8 + 1),
        _ => (byte(at / 8) >> (8 - bits - (at % 8) as u32)) & ((1 << bits) - 1),
    }
}

// The color of `values`, in `space`, as gray or RGB bytes.
fn push_color(out: &mut Vec<u8>, space: &ColorSpace, values: &[f64]) {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match space {
        ColorSpace::Gray => out.push(byte(values[0])),
        ColorSpace::Rgb => out.extend(values[..3].iter().map(|&v| byte(v))),
        ColorSpace::Cmyk => {
            let k = values[3].clamp(0.0, 1.0);
            out.extend(
                values[..3]
                    .iter()
                    .map(|&v| byte(1.0 - (v.clamp(0.0, 1.0) + k).min(1.0))),
            );
        }
        ColorSpace::Lab { white, .. } => out.extend(lab_to_srgb(values, white).map(byte)),
        ColorSpace::Indexed { base, high, lookup } => {
            let index = values[0].round().clamp(0.0, f64::from(*high)) as usize;
            let count = base.components();
            let entry: Vec<f64> = (0..count)
                .map(|c| f64::from(lookup.get(index * count + c).copied().unwrap_or(0)) / 255.0)
                .collect();
            // A palette of Lab colors keeps its own ranges.
            let entry = match base.as_ref() {
                ColorSpace::Lab { range, .. } => vec![
                    entry[0] * 100.0,
                    range[0] + entry[1] * (range[1] - range[0]),
                    range[2] + entry[2] * (range[3] - range[2]),
                ],
                _ => entry,
            };
            push_color(out, base, &entry);
        }
        ColorSpace::Unsupported => out.push(0),
    }
}

// CIE L*a*b* with white point `white` to sRGB, from 0 to 1.
fn lab_to_srgb(lab: &[f64], white: &[f64; 3]) -> [f64; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = fy + lab[1] / 500.0;
    let fz = fy - lab[2] / 200.0;
    let g = |t: f64| {
        if t > 6.0 / 29.0 {
            t * t * t
        } else {
            3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0)
        }
    };
    let (x, y, z) = (white[0] * g(fx), white[1] * g(fy), white[2] * g(fz));
    let linear = [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ];
    linear.map(|c| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}
