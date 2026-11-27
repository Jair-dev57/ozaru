//! TrueType and OpenType fonts with `glyf` outlines (the OpenType
//! specification, `glyf`, `loca`, `head`, `maxp` and `cmap`).

use std::collections::HashMap;

use super::mac_names::MAC_GLYPH_NAMES;
use crate::render::Path;

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn i16_at(data: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The tables of a font file, each as its offset and length, for a font
/// collection those of its first font.
pub(crate) fn table_directory(data: &[u8]) -> Option<HashMap<[u8; 4], (usize, usize)>> {
    let mut base = 0;
    // A font collection: the first font.
    if data.get(..4)? == b"ttcf" {
        base = u32_at(data, 12)? as usize;
    }
    let count = usize::from(u16_at(data, base + 4)?);
    let mut tables: HashMap<[u8; 4], (usize, usize)> = HashMap::new();
    for i in 0..count {
        let at = base + 12 + 16 * i;
        let tag: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
        let offset = u32_at(data, at + 8)? as usize;
        let length = u32_at(data, at + 12)? as usize;
        if offset <= data.len() {
            tables.insert(tag, (offset, length.min(data.len() - offset)));
        }
    }
    Some(tables)
}

/// A `cmap` subtable: which glyph each character code has.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterMap {
    pub platform: u16,
    pub encoding: u16,
    map: HashMap<u32, u16>,
}

impl CharacterMap {
    pub fn glyph(&self, code: u32) -> Option<u16> {
        self.map.get(&code).copied().filter(|&gid| gid != 0)
    }

    /// The characters the map has, in order.
    pub fn codes(&self) -> Vec<u32> {
        let mut codes: Vec<u32> = self.map.keys().copied().collect();
        codes.sort_unstable();
        codes
    }

    /// Whether the characters are Unicode: the Unicode platform, or Windows with
    /// the Unicode BMP or full repertoire encodings.
    pub fn is_unicode(&self) -> bool {
        self.platform == 0 || self.platform == 3 && matches!(self.encoding, 1 | 10)
    }
}

/// A TrueType font, read for its outlines and character maps.
#[derive(Debug, Clone, PartialEq)]
pub struct TrueType {
    data: Vec<u8>,
    glyf: usize,
    glyf_length: usize,
    loca: usize,
    long_loca: bool,
    glyph_count: usize,
    pub units_per_em: u16,
    /// The `cmap` subtables, in the order of the font.
    pub maps: Vec<CharacterMap>,
    /// The offset and length of the `CFF ` table, for OpenType fonts with
    /// PostScript outlines.
    pub cff: Option<(usize, usize)>,
    /// The glyph names of the `post` table, when it has them.
    names: HashMap<Vec<u8>, u16>,
    /// The name of each glyph, from the `post` table.
    post: PostNames,
}

// How the `post` table names glyphs.
#[derive(Debug, Clone, PartialEq)]
enum PostNames {
    None,
    // Format 1: the first 258 glyphs have the standard Macintosh names.
    Standard,
    // Format 2: an index for each glyph: under 258 a standard name, and from there
    // the strings the table holds.
    Indexed {
        indices: Vec<u16>,
        strings: Vec<Vec<u8>>,
    },
}

/// The most nested composite glyphs a glyph may have.
const MAX_DEPTH: usize = 8;

/// The most points a glyph may have, composite ones counted whole.
const MAX_POINTS: usize = 100_000;

impl TrueType {
    /// Reads a font that has outlines.
    pub fn parse(data: &[u8]) -> Option<Self> {
        Self::parse_tables(data, true)
    }

    /// Reads the tables of a font, with outlines or not: for the characters of
    /// its glyphs, which is all text extraction needs.
    pub fn parse_tables(data: &[u8], outlines: bool) -> Option<Self> {
        let tables = table_directory(data)?;
        let head = tables.get(b"head")?.0;
        let units_per_em = u16_at(data, head + 18)?;
        let long_loca = i16_at(data, head + 50)? != 0;
        let glyph_count = usize::from(u16_at(data, tables.get(b"maxp")?.0 + 4)?);
        let (loca, glyf, glyf_length) = match (tables.get(b"loca"), tables.get(b"glyf")) {
            (Some(loca), Some(glyf)) => (loca.0, glyf.0, glyf.1),
            _ => (0, 0, 0),
        };
        let cff = tables.get(b"CFF ").copied();
        if outlines && cff.is_none() && glyf_length == 0 {
            return None;
        }
        let maps = tables
            .get(b"cmap")
            .map(|&(offset, length)| read_cmap(&data[offset..offset + length]))
            .unwrap_or_default();
        let post_table = tables
            .get(b"post")
            .map(|&(offset, length)| &data[offset..offset + length]);
        let names = post_table
            .map(|table| read_post(table, glyph_count))
            .unwrap_or_default();
        let post = post_table.map_or(PostNames::None, |table| read_post_names(table, glyph_count));
        Some(Self {
            data: data.to_vec(),
            glyf,
            glyf_length,
            loca,
            long_loca,
            glyph_count,
            units_per_em: if units_per_em == 0 {
                1000
            } else {
                units_per_em
            },
            maps,
            cff,
            names,
            post,
        })
    }

    pub fn glyph_count(&self) -> usize {
        self.glyph_count
    }

    /// The bytes of the `CFF ` table of an OpenType font.
    pub fn cff_table(&self) -> Option<&[u8]> {
        let (offset, length) = self.cff?;
        self.data.get(offset..offset + length)
    }

    /// The name the `post` table gives to glyph `gid`.
    pub fn glyph_name(&self, gid: u16) -> Option<&[u8]> {
        let standard = |index: usize| MAC_GLYPH_NAMES.get(index).map(|name| name.as_bytes());
        match &self.post {
            PostNames::None => None,
            PostNames::Standard => standard(usize::from(gid)),
            PostNames::Indexed { indices, strings } => {
                let index = usize::from(*indices.get(usize::from(gid))?);
                if index < 258 {
                    standard(index)
                } else {
                    strings.get(index - 258).map(Vec::as_slice)
                }
            }
        }
    }

    /// The glyph named `name` in the `post` table.
    pub fn glyph_by_name(&self, name: &[u8]) -> Option<u16> {
        self.names.get(name).copied()
    }

    /// The character map of a platform and encoding.
    pub fn map(&self, platform: u16, encoding: u16) -> Option<&CharacterMap> {
        self.maps
            .iter()
            .find(|m| m.platform == platform && m.encoding == encoding)
    }

    fn glyph_range(&self, gid: usize) -> Option<(usize, usize)> {
        if gid >= self.glyph_count {
            return None;
        }
        let (start, end) = if self.long_loca {
            (
                u32_at(&self.data, self.loca + 4 * gid)? as usize,
                u32_at(&self.data, self.loca + 4 * gid + 4)? as usize,
            )
        } else {
            (
                usize::from(u16_at(&self.data, self.loca + 2 * gid)?) * 2,
                usize::from(u16_at(&self.data, self.loca + 2 * gid + 2)?) * 2,
            )
        };
        (start < end && end <= self.glyf_length).then_some((self.glyf + start, self.glyf + end))
    }

    /// The outline of glyph `gid`, in font units, y up. An empty path for a
    /// glyph without contours, and `None` for one that doesn't exist or that
    /// is damaged.
    pub fn outline(&self, gid: u16) -> Option<Path> {
        let mut path = Path::new();
        let mut budget = MAX_POINTS;
        self.add_glyph(
            usize::from(gid),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            0,
            &mut budget,
            &mut path,
        )?;
        Some(path)
    }

    fn add_glyph(
        &self,
        gid: usize,
        matrix: [f64; 6],
        depth: usize,
        budget: &mut usize,
        path: &mut Path,
    ) -> Option<()> {
        if gid >= self.glyph_count {
            return None;
        }
        let Some((start, end)) = self.glyph_range(gid) else {
            // No outline: a space.
            return Some(());
        };
        let data = self.data.get(start..end)?;
        let contours = i16_at(data, 0)?;
        if contours >= 0 {
            self.simple_glyph(data, contours as usize, matrix, budget, path)
        } else {
            if depth >= MAX_DEPTH {
                return None;
            }
            let mut at = 10;
            loop {
                let flags = u16_at(data, at)?;
                let component = usize::from(u16_at(data, at + 2)?);
                at += 4;
                let (dx, dy) = if flags & 1 != 0 {
                    let v = (i16_at(data, at)?, i16_at(data, at + 2)?);
                    at += 4;
                    (f64::from(v.0), f64::from(v.1))
                } else {
                    let v = (*data.get(at)? as i8, *data.get(at + 1)? as i8);
                    at += 2;
                    (f64::from(v.0), f64::from(v.1))
                };
                // Arguments that are points to match, not offsets, are
                // taken as no offset.
                let (dx, dy) = if flags & 2 != 0 { (dx, dy) } else { (0.0, 0.0) };
                let f2dot14 = |at: usize| i16_at(data, at).map(|v| f64::from(v) / 16384.0);
                let (a, b, c, d) = if flags & 8 != 0 {
                    let s = f2dot14(at)?;
                    at += 2;
                    (s, 0.0, 0.0, s)
                } else if flags & 0x40 != 0 {
                    let (x, y) = (f2dot14(at)?, f2dot14(at + 2)?);
                    at += 4;
                    (x, 0.0, 0.0, y)
                } else if flags & 0x80 != 0 {
                    let v = (
                        f2dot14(at)?,
                        f2dot14(at + 2)?,
                        f2dot14(at + 4)?,
                        f2dot14(at + 6)?,
                    );
                    at += 8;
                    (v.0, v.1, v.2, v.3)
                } else {
                    (1.0, 0.0, 0.0, 1.0)
                };
                // The component's own matrix first, then the parent's.
                let own = [a, b, c, d, dx, dy];
                let combined = [
                    own[0] * matrix[0] + own[1] * matrix[2],
                    own[0] * matrix[1] + own[1] * matrix[3],
                    own[2] * matrix[0] + own[3] * matrix[2],
                    own[2] * matrix[1] + own[3] * matrix[3],
                    own[4] * matrix[0] + own[5] * matrix[2] + matrix[4],
                    own[4] * matrix[1] + own[5] * matrix[3] + matrix[5],
                ];
                self.add_glyph(component, combined, depth + 1, budget, path)?;
                if flags & 0x20 == 0 {
                    return Some(());
                }
            }
        }
    }

    fn simple_glyph(
        &self,
        data: &[u8],
        contours: usize,
        matrix: [f64; 6],
        budget: &mut usize,
        path: &mut Path,
    ) -> Option<()> {
        if contours == 0 {
            return Some(());
        }
        let mut ends = Vec::with_capacity(contours);
        for i in 0..contours {
            ends.push(usize::from(u16_at(data, 10 + 2 * i)?));
        }
        let points = ends.last()? + 1;
        if points > *budget || ends.windows(2).any(|w| w[1] < w[0]) {
            return None;
        }
        *budget -= points;
        let mut at = 10 + 2 * contours;
        let instructions = usize::from(u16_at(data, at)?);
        at += 2 + instructions;
        let mut flags = Vec::with_capacity(points);
        while flags.len() < points {
            let flag = *data.get(at)?;
            at += 1;
            flags.push(flag);
            if flag & 8 != 0 {
                let repeat = usize::from(*data.get(at)?);
                at += 1;
                for _ in 0..repeat {
                    if flags.len() < points {
                        flags.push(flag);
                    }
                }
            }
        }
        let mut coordinates = |short: u8, same: u8| -> Option<Vec<i32>> {
            let mut values = Vec::with_capacity(points);
            let mut value = 0i32;
            for &flag in &flags {
                if flag & short != 0 {
                    let delta = i32::from(*data.get(at)?);
                    at += 1;
                    value += if flag & same != 0 { delta } else { -delta };
                } else if flag & same == 0 {
                    value += i32::from(i16_at(data, at)?);
                    at += 2;
                }
                values.push(value);
            }
            Some(values)
        };
        let xs = coordinates(2, 16)?;
        let ys = coordinates(4, 32)?;
        let place = |i: usize| {
            let (x, y) = (f64::from(xs[i]), f64::from(ys[i]));
            (
                x * matrix[0] + y * matrix[2] + matrix[4],
                x * matrix[1] + y * matrix[3] + matrix[5],
            )
        };
        let mut start = 0;
        for &end in &ends {
            let points: Vec<((f64, f64), bool)> = (start..=end)
                .map(|i| (place(i), flags[i] & 1 != 0))
                .collect();
            contour(&points, path);
            start = end + 1;
        }
        Some(())
    }
}

// Adds a closed contour of on and off curve points, which make quadratic
// curves, as cubic ones. Two off curve points in a row have an on curve point
// between them.
fn contour(points: &[((f64, f64), bool)], path: &mut Path) {
    if points.is_empty() {
        return;
    }
    let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    // Start at an on curve point, or between two off curve ones.
    let (start, first) = match points.iter().position(|p| p.1) {
        Some(i) => (points[i].0, i + 1),
        None => (mid(points[0].0, points[points.len() - 1].0), 0),
    };
    path.move_to(start.0, start.1);
    let mut current = start;
    let count = if points.iter().any(|p| p.1) {
        points.len() - 1
    } else {
        points.len()
    };
    let mut pending: Option<(f64, f64)> = None;
    let quad = |path: &mut Path, from: (f64, f64), control: (f64, f64), to: (f64, f64)| {
        path.curve_to(
            from.0 + 2.0 / 3.0 * (control.0 - from.0),
            from.1 + 2.0 / 3.0 * (control.1 - from.1),
            to.0 + 2.0 / 3.0 * (control.0 - to.0),
            to.1 + 2.0 / 3.0 * (control.1 - to.1),
            to.0,
            to.1,
        );
    };
    for k in 0..count {
        let (point, on) = points[(first + k) % points.len()];
        match (pending, on) {
            (None, true) => {
                path.line_to(point.0, point.1);
                current = point;
            }
            (None, false) => pending = Some(point),
            (Some(control), true) => {
                quad(path, current, control, point);
                current = point;
                pending = None;
            }
            (Some(control), false) => {
                let middle = mid(control, point);
                quad(path, current, control, middle);
                current = middle;
                pending = Some(point);
            }
        }
    }
    if let Some(control) = pending {
        quad(path, current, control, start);
    }
    path.close();
}

fn read_cmap(table: &[u8]) -> Vec<CharacterMap> {
    let mut maps = Vec::new();
    let Some(count) = u16_at(table, 2) else {
        return maps;
    };
    for i in 0..usize::from(count) {
        let record = 4 + 8 * i;
        let (Some(platform), Some(encoding), Some(offset)) = (
            u16_at(table, record),
            u16_at(table, record + 2),
            u32_at(table, record + 4),
        ) else {
            continue;
        };
        if let Some(map) = table
            .get(offset as usize..)
            .and_then(read_subtable)
            .filter(|m| !m.is_empty())
        {
            maps.push(CharacterMap {
                platform,
                encoding,
                map,
            });
        }
    }
    maps
}

// The most characters one subtable is read for.
const MAX_CODES: usize = 200_000;

fn read_subtable(table: &[u8]) -> Option<HashMap<u32, u16>> {
    let mut map = HashMap::new();
    match u16_at(table, 0)? {
        0 => {
            for code in 0..256usize {
                let gid = *table.get(6 + code)?;
                if gid != 0 {
                    map.insert(code as u32, u16::from(gid));
                }
            }
        }
        4 => {
            let segments = usize::from(u16_at(table, 6)?) / 2;
            let ends = 14;
            let starts = ends + 2 * segments + 2;
            let deltas = starts + 2 * segments;
            let ranges = deltas + 2 * segments;
            for i in 0..segments {
                let end = u32::from(u16_at(table, ends + 2 * i)?);
                let start = u32::from(u16_at(table, starts + 2 * i)?);
                let delta = i32::from(i16_at(table, deltas + 2 * i)?);
                let range = usize::from(u16_at(table, ranges + 2 * i)?);
                if start > end || end == 0xFFFF && start == 0xFFFF {
                    continue;
                }
                for code in start..=end {
                    let gid = if range == 0 {
                        (code as i32 + delta) as u16
                    } else {
                        let at = ranges + 2 * i + range + 2 * (code - start) as usize;
                        match u16_at(table, at)? {
                            0 => 0,
                            g => (i32::from(g) + delta) as u16,
                        }
                    };
                    if gid != 0 {
                        map.insert(code, gid);
                    }
                    if map.len() > MAX_CODES {
                        return None;
                    }
                }
            }
        }
        6 => {
            let first = u32::from(u16_at(table, 6)?);
            let count = usize::from(u16_at(table, 8)?);
            for i in 0..count {
                let gid = u16_at(table, 10 + 2 * i)?;
                if gid != 0 {
                    map.insert(first + i as u32, gid);
                }
            }
        }
        12 => {
            let groups = u32_at(table, 12)? as usize;
            for i in 0..groups.min(MAX_CODES) {
                let at = 16 + 12 * i;
                let (start, end, gid) = (
                    u32_at(table, at)?,
                    u32_at(table, at + 4)?,
                    u32_at(table, at + 8)?,
                );
                if end < start || end - start > 0x10_0000 {
                    continue;
                }
                for code in start..=end {
                    let g = gid + (code - start);
                    if g != 0 && g <= 0xFFFF {
                        map.insert(code, g as u16);
                    }
                    if map.len() > MAX_CODES {
                        return None;
                    }
                }
            }
        }
        10 => {
            let first = u32_at(table, 12)?;
            let count = u32_at(table, 16)? as usize;
            for i in 0..count.min(MAX_CODES) {
                let gid = u16_at(table, 20 + 2 * i)?;
                if gid != 0 {
                    map.insert(first + i as u32, gid);
                }
            }
        }
        13 => {
            // Many characters, one glyph each range: fonts of last resort.
            let groups = u32_at(table, 12)? as usize;
            for i in 0..groups.min(MAX_CODES) {
                let at = 16 + 12 * i;
                let (start, end, gid) = (
                    u32_at(table, at)?,
                    u32_at(table, at + 4)?,
                    u32_at(table, at + 8)?,
                );
                if end < start || end - start > 0x10_0000 || gid == 0 || gid > 0xFFFF {
                    continue;
                }
                for code in start..=end {
                    map.insert(code, gid as u16);
                    if map.len() > MAX_CODES {
                        return None;
                    }
                }
            }
        }
        _ => return None,
    }
    Some(map)
}

// The glyph names of `post` format 2.0, by name.
fn read_post(table: &[u8], glyph_count: usize) -> HashMap<Vec<u8>, u16> {
    let mut names = HashMap::new();
    if u32_at(table, 0) != Some(0x0002_0000) {
        return names;
    }
    let Some(count) = u16_at(table, 32).map(usize::from) else {
        return names;
    };
    let count = count.min(glyph_count);
    // The names after the indices, as Pascal strings.
    let mut strings: Vec<&[u8]> = Vec::new();
    let mut at = 34 + 2 * usize::from(u16_at(table, 32).unwrap_or(0));
    while let Some(&length) = table.get(at) {
        let Some(name) = table.get(at + 1..at + 1 + usize::from(length)) else {
            break;
        };
        strings.push(name);
        at += 1 + usize::from(length);
    }
    for gid in 0..count {
        let Some(index) = u16_at(table, 34 + 2 * gid).map(usize::from) else {
            break;
        };
        // Indices under 258 are the standard Macintosh names, which the
        // engine doesn't keep: those glyphs are found by their characters.
        if index >= 258
            && let Some(name) = strings.get(index - 258)
        {
            names.insert(name.to_vec(), gid as u16);
        }
    }
    names
}

// The names of glyphs by `post` format: 1, the standard names, or 2, indices and
// strings.
fn read_post_names(table: &[u8], glyph_count: usize) -> PostNames {
    match u32_at(table, 0) {
        Some(0x0001_0000) => PostNames::Standard,
        Some(0x0002_0000) => {
            let Some(count) = u16_at(table, 32).map(usize::from) else {
                return PostNames::None;
            };
            let count = count.min(glyph_count);
            let indices: Vec<u16> = (0..count)
                .map_while(|gid| u16_at(table, 34 + 2 * gid))
                .collect();
            let mut strings: Vec<Vec<u8>> = Vec::new();
            let mut at = 34 + 2 * usize::from(u16_at(table, 32).unwrap_or(0));
            while let Some(&length) = table.get(at) {
                let Some(name) = table.get(at + 1..at + 1 + usize::from(length)) else {
                    break;
                };
                strings.push(name.to_vec());
                at += 1 + usize::from(length);
            }
            PostNames::Indexed { indices, strings }
        }
        _ => PostNames::None,
    }
}
