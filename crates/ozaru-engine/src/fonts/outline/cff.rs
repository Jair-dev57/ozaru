//! The Compact Font Format (Adobe Technical Note #5176) and its Type 2
//! charstrings (#5177): the fonts of `/FontFile3` and the outlines of
//! OpenType fonts with PostScript outlines.

#![allow(clippy::chunks_exact_to_as_chunks)]

use std::collections::HashMap;
use std::ops::Range;

use crate::render::Path;

/// The names of the strings with a number from 0 to 228 (Appendix A), which
/// are the glyphs of the Standard and ISOAdobe sets.
const STANDARD_STRINGS: &str = ".notdef space exclam quotedbl numbersign dollar percent ampersand quoteright parenleft parenright asterisk plus comma hyphen period slash zero one two three four five six seven eight nine colon semicolon less equal greater question at A B C D E F G H I J K L M N O P Q R S T U V W X Y Z bracketleft backslash bracketright asciicircum underscore quoteleft a b c d e f g h i j k l m n o p q r s t u v w x y z braceleft bar braceright asciitilde exclamdown cent sterling fraction yen florin section currency quotesingle quotedblleft guillemotleft guilsinglleft guilsinglright fi fl endash dagger daggerdbl periodcentered paragraph bullet quotesinglbase quotedblbase quotedblright guillemotright ellipsis perthousand questiondown grave acute circumflex tilde macron breve dotaccent dieresis ring cedilla hungarumlaut ogonek caron emdash AE ordfeminine Lslash Oslash OE ordmasculine ae dotlessi lslash oslash oe germandbls onesuperior logicalnot mu trademark Eth onehalf plusminus Thorn onequarter divide brokenbar degree thorn threequarters twosuperior registered minus eth multiply threesuperior copyright Aacute Acircumflex Adieresis Agrave Aring Atilde Ccedilla Eacute Ecircumflex Edieresis Egrave Iacute Icircumflex Idieresis Igrave Ntilde Oacute Ocircumflex Odieresis Ograve Otilde Scaron Uacute Ucircumflex Udieresis Ugrave Yacute Ydieresis Zcaron aacute acircumflex adieresis agrave aring atilde ccedilla eacute ecircumflex edieresis egrave iacute icircumflex idieresis igrave ntilde oacute ocircumflex odieresis ograve otilde scaron uacute ucircumflex udieresis ugrave yacute ydieresis zcaron";

/// How many strings the standard ones are.
const STANDARD_COUNT: usize = 391;

fn standard_string(sid: usize) -> Option<&'static str> {
    STANDARD_STRINGS.split(' ').nth(sid)
}

/// An INDEX: the byte ranges of its objects, and where it ends.
fn read_index(data: &[u8], at: usize) -> Option<(Vec<Range<usize>>, usize)> {
    let count = usize::from(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?));
    if count == 0 {
        return Some((Vec::new(), at + 2));
    }
    let size = usize::from(*data.get(at + 2)?);
    if !(1..=4).contains(&size) {
        return None;
    }
    let offsets_at = at + 3;
    let base = offsets_at + (count + 1) * size - 1;
    let offset = |i: usize| -> Option<usize> {
        let bytes = data.get(offsets_at + i * size..offsets_at + (i + 1) * size)?;
        Some(bytes.iter().fold(0usize, |v, &b| v << 8 | usize::from(b)))
    };
    let mut ranges = Vec::with_capacity(count.min(1 << 16));
    let mut previous = offset(0)?;
    for i in 1..=count {
        let next = offset(i)?;
        if next < previous || base + next > data.len() {
            return None;
        }
        ranges.push(base + previous..base + next);
        previous = next;
    }
    Some((ranges, base + previous))
}

/// The operands of a DICT, by operator (two-byte operators are `0x0C00 + b`).
fn read_dict(data: &[u8]) -> HashMap<u16, Vec<f64>> {
    let mut dict = HashMap::new();
    let mut operands: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        i += 1;
        match b0 {
            0..=21 => {
                let operator = if b0 == 12 {
                    let b1 = data.get(i).copied().unwrap_or(0);
                    i += 1;
                    0x0C00 + u16::from(b1)
                } else {
                    u16::from(b0)
                };
                dict.insert(operator, std::mem::take(&mut operands));
            }
            28 => {
                let v = i16::from_be_bytes([
                    data.get(i).copied().unwrap_or(0),
                    data.get(i + 1).copied().unwrap_or(0),
                ]);
                operands.push(f64::from(v));
                i += 2;
            }
            29 => {
                let bytes: [u8; 4] = data
                    .get(i..i + 4)
                    .and_then(|b| b.try_into().ok())
                    .unwrap_or([0; 4]);
                operands.push(f64::from(i32::from_be_bytes(bytes)));
                i += 4;
            }
            30 => {
                let mut text = String::new();
                'nibbles: while i < data.len() {
                    let byte = data[i];
                    i += 1;
                    for nibble in [byte >> 4, byte & 15] {
                        match nibble {
                            0..=9 => text.push((b'0' + nibble) as char),
                            10 => text.push('.'),
                            11 => text.push('E'),
                            12 => text.push_str("E-"),
                            14 => text.push('-'),
                            15 => break 'nibbles,
                            _ => {}
                        }
                    }
                }
                operands.push(text.parse().unwrap_or(0.0));
            }
            32..=246 => operands.push(f64::from(b0) - 139.0),
            247..=250 => {
                let b1 = f64::from(data.get(i).copied().unwrap_or(0));
                i += 1;
                operands.push((f64::from(b0) - 247.0) * 256.0 + b1 + 108.0);
            }
            251..=254 => {
                let b1 = f64::from(data.get(i).copied().unwrap_or(0));
                i += 1;
                operands.push(-(f64::from(b0) - 251.0) * 256.0 - b1 - 108.0);
            }
            _ => {}
        }
        if operands.len() > 48 {
            operands.clear();
        }
    }
    dict
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Private {
    subrs: Vec<Range<usize>>,
}

/// A CFF font, read for its outlines, glyph names and encoding.
#[derive(Debug, Clone, PartialEq)]
pub struct Cff {
    data: Vec<u8>,
    charstrings: Vec<Range<usize>>,
    global_subrs: Vec<Range<usize>>,
    strings: Vec<Range<usize>>,
    // The string (or for CID fonts, the CID) of each glyph.
    charset: Vec<u16>,
    privates: Vec<Private>,
    fd_select: Vec<u8>,
    pub is_cid: bool,
    /// From glyph space to text space; 0.001 scale unless the font says.
    pub font_matrix: [f64; 6],
    encoding: HashMap<u8, u16>,
    by_name: HashMap<Vec<u8>, u16>,
    by_cid: HashMap<u16, u16>,
}

impl Cff {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 4 || data[0] != 1 {
            return None;
        }
        let header = usize::from(data[2]);
        let (_names, after) = read_index(data, header)?;
        let (top_dicts, after) = read_index(data, after)?;
        let (strings, after) = read_index(data, after)?;
        let (global_subrs, _) = read_index(data, after)?;
        let top = read_dict(&data[top_dicts.first()?.clone()]);
        let charstrings_at = top.get(&17)?.first().copied()? as usize;
        let (charstrings, _) = read_index(data, charstrings_at)?;
        let glyph_count = charstrings.len();
        let is_cid = top.contains_key(&0x0C1E);
        let font_matrix = match top.get(&0x0C07) {
            Some(m) if m.len() == 6 && m[0] != 0.0 => [m[0], m[1], m[2], m[3], m[4], m[5]],
            _ => [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
        };

        let read_private = |dict: &HashMap<u16, Vec<f64>>| -> Private {
            let Some(&[size, offset]) = dict.get(&18).map(Vec::as_slice) else {
                return Private::default();
            };
            let (size, offset) = (size as usize, offset as usize);
            let Some(bytes) = data.get(offset..offset + size) else {
                return Private::default();
            };
            let private = read_dict(bytes);
            let subrs = private
                .get(&19)
                .and_then(|v| v.first())
                .and_then(|&rel| read_index(data, offset + rel as usize))
                .map(|(ranges, _)| ranges)
                .unwrap_or_default();
            Private { subrs }
        };
        let mut privates = Vec::new();
        let mut fd_select = Vec::new();
        if is_cid {
            let fd_array_at = top.get(&0x0C24)?.first().copied()? as usize;
            let (fonts, _) = read_index(data, fd_array_at)?;
            for range in fonts {
                privates.push(read_private(&read_dict(&data[range])));
            }
            if let Some(&at) = top.get(&0x0C25).and_then(|v| v.first()) {
                fd_select = read_fd_select(data, at as usize, glyph_count)?;
            }
        } else {
            privates.push(read_private(&top));
        }
        let charset_at = top.get(&15).and_then(|v| v.first()).copied().unwrap_or(0.0) as usize;
        let charset = read_charset(data, charset_at, glyph_count, is_cid)?;
        let mut cff = Self {
            data: data.to_vec(),
            charstrings,
            global_subrs,
            strings,
            charset,
            privates,
            fd_select,
            is_cid,
            font_matrix,
            encoding: HashMap::new(),
            by_name: HashMap::new(),
            by_cid: HashMap::new(),
        };
        if is_cid {
            for (gid, &cid) in cff.charset.iter().enumerate() {
                cff.by_cid.entry(cid).or_insert(gid as u16);
            }
        } else {
            for gid in 0..glyph_count {
                if let Some(name) = cff.glyph_name(gid as u16) {
                    cff.by_name.entry(name.to_vec()).or_insert(gid as u16);
                }
            }
            let encoding_at = top.get(&16).and_then(|v| v.first()).copied().unwrap_or(0.0) as usize;
            if encoding_at > 1 {
                cff.encoding = read_encoding(data, encoding_at, glyph_count).unwrap_or_default();
            }
        }
        Some(cff)
    }

    pub fn glyph_count(&self) -> usize {
        self.charstrings.len()
    }

    /// The name of glyph `gid`, `None` for CID fonts.
    pub fn glyph_name(&self, gid: u16) -> Option<&[u8]> {
        if self.is_cid {
            return None;
        }
        let sid = usize::from(*self.charset.get(usize::from(gid))?);
        if sid < STANDARD_COUNT {
            return standard_string(sid).map(str::as_bytes);
        }
        let range = self.strings.get(sid - STANDARD_COUNT)?;
        self.data.get(range.clone())
    }

    pub fn glyph_by_name(&self, name: &[u8]) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    /// The glyph of a CID in a CID-keyed font.
    pub fn glyph_by_cid(&self, cid: u16) -> Option<u16> {
        self.by_cid.get(&cid).copied()
    }

    /// The glyph the font's own encoding gives a code.
    pub fn builtin_glyph(&self, code: u8) -> Option<u16> {
        self.encoding.get(&code).copied()
    }

    /// The outline of glyph `gid`, in glyph space units, y up. `seac_name`
    /// finds the glyph of a Standard encoding code, for accented characters
    /// made of two.
    pub fn outline(&self, gid: u16, standard: &dyn Fn(u8) -> Option<u16>) -> Option<Path> {
        let mut path = Path::new();
        let mut interpreter = Interpreter::new(self, usize::from(gid));
        interpreter.run(usize::from(gid), standard, (0.0, 0.0), &mut path)?;
        Some(path)
    }
}

fn read_fd_select(data: &[u8], at: usize, glyphs: usize) -> Option<Vec<u8>> {
    let mut select = vec![0u8; glyphs];
    match *data.get(at)? {
        0 => {
            for (i, slot) in select.iter_mut().enumerate() {
                *slot = *data.get(at + 1 + i)?;
            }
        }
        3 => {
            let ranges = usize::from(u16::from_be_bytes(
                data.get(at + 1..at + 3)?.try_into().ok()?,
            ));
            for r in 0..ranges {
                let base = at + 3 + 3 * r;
                let first = usize::from(u16::from_be_bytes(
                    data.get(base..base + 2)?.try_into().ok()?,
                ));
                let fd = *data.get(base + 2)?;
                let next = usize::from(u16::from_be_bytes(
                    data.get(base + 3..base + 5)?.try_into().ok()?,
                ));
                for slot in select.iter_mut().take(next.min(glyphs)).skip(first) {
                    *slot = fd;
                }
            }
        }
        _ => return None,
    }
    Some(select)
}

fn read_charset(data: &[u8], at: usize, glyphs: usize, is_cid: bool) -> Option<Vec<u16>> {
    // The predefined sets: ISOAdobe has the glyphs 1 to 228 in order.
    if at <= 2 {
        return Some((0..glyphs as u16).collect());
    }
    let mut charset = vec![0u16];
    let mut i = at + 1;
    let read = |i: usize| data.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    match *data.get(at)? {
        0 => {
            while charset.len() < glyphs {
                charset.push(read(i)?);
                i += 2;
            }
        }
        format @ (1 | 2) => {
            while charset.len() < glyphs {
                let first = read(i)?;
                let left = if format == 1 {
                    let v = u16::from(*data.get(i + 2)?);
                    i += 3;
                    v
                } else {
                    let v = read(i + 2)?;
                    i += 4;
                    v
                };
                for k in 0..=left {
                    if charset.len() < glyphs {
                        charset.push(first.wrapping_add(k));
                    }
                }
            }
        }
        _ => return None,
    }
    let _ = is_cid;
    Some(charset)
}

fn read_encoding(data: &[u8], at: usize, glyphs: usize) -> Option<HashMap<u8, u16>> {
    let mut encoding = HashMap::new();
    let format = *data.get(at)?;
    let mut i = at + 1;
    match format & 0x7F {
        0 => {
            let count = usize::from(*data.get(i)?);
            i += 1;
            for g in 0..count {
                let code = *data.get(i + g)?;
                if g + 1 < glyphs {
                    encoding.insert(code, (g + 1) as u16);
                }
            }
            i += count;
        }
        1 => {
            let ranges = usize::from(*data.get(i)?);
            i += 1;
            let mut gid = 1u16;
            for r in 0..ranges {
                let first = *data.get(i + 2 * r)?;
                let left = *data.get(i + 2 * r + 1)?;
                for k in 0..=left {
                    if usize::from(gid) < glyphs {
                        encoding.insert(first.wrapping_add(k), gid);
                    }
                    gid += 1;
                }
            }
            i += 2 * ranges;
        }
        _ => return None,
    }
    // Supplements: codes of glyphs given by their strings, which are rare.
    let _ = i;
    Some(encoding)
}

// A charstring interpreter (Type 2, section 4).
struct Interpreter<'f> {
    font: &'f Cff,
    private: usize,
    stack: Vec<f64>,
    x: f64,
    y: f64,
    open: bool,
    stems: usize,
    first_op: bool,
    steps: usize,
    transients: [f64; 32],
}

const MAX_STEPS: usize = 1_000_000;
const MAX_SUBR_DEPTH: usize = 10;

impl<'f> Interpreter<'f> {
    fn new(font: &'f Cff, gid: usize) -> Self {
        let fd = font.fd_select.get(gid).copied().map_or(0, usize::from);
        Self {
            font,
            private: fd.min(font.privates.len().saturating_sub(1)),
            stack: Vec::with_capacity(48),
            x: 0.0,
            y: 0.0,
            open: false,
            stems: 0,
            first_op: true,
            steps: 0,
            transients: [0.0; 32],
        }
    }

    fn bias(count: usize) -> i64 {
        if count < 1240 {
            107
        } else if count < 33900 {
            1131
        } else {
            32768
        }
    }

    fn run(
        &mut self,
        gid: usize,
        standard: &dyn Fn(u8) -> Option<u16>,
        origin: (f64, f64),
        path: &mut Path,
    ) -> Option<()> {
        let range = self.font.charstrings.get(gid)?.clone();
        self.x = origin.0;
        self.y = origin.1;
        let code = self.font.data.get(range)?.to_vec();
        self.execute(&code, 0, standard, path)?;
        if self.open {
            path.close();
            self.open = false;
        }
        Some(())
    }

    fn move_to(&mut self, dx: f64, dy: f64, path: &mut Path) {
        if self.open {
            path.close();
        }
        self.x += dx;
        self.y += dy;
        path.move_to(self.x, self.y);
        self.open = true;
    }

    fn line_to(&mut self, dx: f64, dy: f64, path: &mut Path) {
        self.x += dx;
        self.y += dy;
        path.line_to(self.x, self.y);
    }

    fn curve_to(&mut self, d: [f64; 6], path: &mut Path) {
        let (x1, y1) = (self.x + d[0], self.y + d[1]);
        let (x2, y2) = (x1 + d[2], y1 + d[3]);
        self.x = x2 + d[4];
        self.y = y2 + d[5];
        path.curve_to(x1, y1, x2, y2, self.x, self.y);
    }

    // Drops the width, the first argument that isn't one of the operator's,
    // before the first stack-clearing operator.
    fn take_width(&mut self, expected_even: Option<usize>, at_least: Option<usize>) {
        if !self.first_op {
            return;
        }
        self.first_op = false;
        let has_width = match (expected_even, at_least) {
            (Some(_), _) => self.stack.len() % 2 == 1,
            (None, Some(n)) => self.stack.len() > n,
            _ => false,
        };
        if has_width && !self.stack.is_empty() {
            self.stack.remove(0);
        }
    }

    fn execute(
        &mut self,
        code: &[u8],
        depth: usize,
        standard: &dyn Fn(u8) -> Option<u16>,
        path: &mut Path,
    ) -> Option<bool> {
        let mut i = 0;
        while i < code.len() {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                return None;
            }
            let b0 = code[i];
            i += 1;
            match b0 {
                28 => {
                    let v = i16::from_be_bytes([*code.get(i)?, *code.get(i + 1)?]);
                    self.push(f64::from(v))?;
                    i += 2;
                }
                32..=246 => self.push(f64::from(b0) - 139.0)?,
                247..=250 => {
                    self.push((f64::from(b0) - 247.0) * 256.0 + f64::from(*code.get(i)?) + 108.0)?;
                    i += 1;
                }
                251..=254 => {
                    self.push(-(f64::from(b0) - 251.0) * 256.0 - f64::from(*code.get(i)?) - 108.0)?;
                    i += 1;
                }
                255 => {
                    let v = i32::from_be_bytes(code.get(i..i + 4)?.try_into().ok()?);
                    self.push(f64::from(v) / 65536.0)?;
                    i += 4;
                }
                1 | 3 | 18 | 23 => {
                    self.take_width(Some(0), None);
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                }
                19 | 20 => {
                    self.take_width(Some(0), None);
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                    i += self.stems.div_ceil(8);
                }
                21 => {
                    self.take_width(None, Some(2));
                    let n = self.stack.len();
                    if n >= 2 {
                        let (dx, dy) = (self.stack[n - 2], self.stack[n - 1]);
                        self.move_to(dx, dy, path);
                    }
                    self.stack.clear();
                }
                22 => {
                    self.take_width(None, Some(1));
                    if let Some(&dx) = self.stack.last() {
                        self.move_to(dx, 0.0, path);
                    }
                    self.stack.clear();
                }
                4 => {
                    self.take_width(None, Some(1));
                    if let Some(&dy) = self.stack.last() {
                        self.move_to(0.0, dy, path);
                    }
                    self.stack.clear();
                }
                5 => {
                    let args = std::mem::take(&mut self.stack);
                    for pair in args.chunks_exact(2) {
                        self.line_to(pair[0], pair[1], path);
                    }
                }
                6 | 7 => {
                    let args = std::mem::take(&mut self.stack);
                    let mut horizontal = b0 == 6;
                    for &d in &args {
                        if horizontal {
                            self.line_to(d, 0.0, path);
                        } else {
                            self.line_to(0.0, d, path);
                        }
                        horizontal = !horizontal;
                    }
                }
                8 => {
                    let args = std::mem::take(&mut self.stack);
                    for c in args.chunks_exact(6) {
                        self.curve_to([c[0], c[1], c[2], c[3], c[4], c[5]], path);
                    }
                }
                24 => {
                    let args = std::mem::take(&mut self.stack);
                    let curves = args.len().saturating_sub(2) / 6;
                    for c in args.chunks_exact(6).take(curves) {
                        self.curve_to([c[0], c[1], c[2], c[3], c[4], c[5]], path);
                    }
                    if let Some(l) = args.get(curves * 6..curves * 6 + 2) {
                        self.line_to(l[0], l[1], path);
                    }
                }
                25 => {
                    let args = std::mem::take(&mut self.stack);
                    let lines = args.len().saturating_sub(6) / 2;
                    for l in args.chunks_exact(2).take(lines) {
                        self.line_to(l[0], l[1], path);
                    }
                    if let Some(c) = args.get(lines * 2..lines * 2 + 6) {
                        self.curve_to([c[0], c[1], c[2], c[3], c[4], c[5]], path);
                    }
                }
                26 => {
                    let args = std::mem::take(&mut self.stack);
                    let (mut dx1, rest) = if args.len() % 4 == 1 {
                        (args[0], &args[1..])
                    } else {
                        (0.0, &args[..])
                    };
                    for c in rest.chunks_exact(4) {
                        self.curve_to([dx1, c[0], c[1], c[2], 0.0, c[3]], path);
                        dx1 = 0.0;
                    }
                }
                27 => {
                    let args = std::mem::take(&mut self.stack);
                    let (mut dy1, rest) = if args.len() % 4 == 1 {
                        (args[0], &args[1..])
                    } else {
                        (0.0, &args[..])
                    };
                    for c in rest.chunks_exact(4) {
                        self.curve_to([c[0], dy1, c[1], c[2], c[3], 0.0], path);
                        dy1 = 0.0;
                    }
                }
                30 | 31 => {
                    let args = std::mem::take(&mut self.stack);
                    let mut horizontal = b0 == 31;
                    let mut k = 0;
                    while k + 4 <= args.len() {
                        let last = args.len() - k == 5;
                        let extra = if last { args[k + 4] } else { 0.0 };
                        let c = &args[k..k + 4];
                        if horizontal {
                            self.curve_to([c[0], 0.0, c[1], c[2], extra, c[3]], path);
                        } else {
                            self.curve_to([0.0, c[0], c[1], c[2], c[3], extra], path);
                        }
                        horizontal = !horizontal;
                        k += 4;
                    }
                }
                10 | 29 => {
                    let subrs = if b0 == 10 {
                        &self.font.privates.get(self.private)?.subrs
                    } else {
                        &self.font.global_subrs
                    };
                    let index = self.stack.pop()? as i64 + Self::bias(subrs.len());
                    let range = subrs.get(usize::try_from(index).ok()?)?.clone();
                    if depth >= MAX_SUBR_DEPTH {
                        return None;
                    }
                    let body = self.font.data.get(range)?.to_vec();
                    if self.execute(&body, depth + 1, standard, path)? {
                        return Some(true);
                    }
                }
                11 => return Some(false),
                14 => {
                    // With four arguments, an accented character of two
                    // glyphs (section 4.1, seac-like endchar).
                    self.take_width(None, Some(if self.stack.len() >= 4 { 4 } else { 0 }));
                    if self.stack.len() >= 4 {
                        let n = self.stack.len();
                        let (adx, ady, bchar, achar) = (
                            self.stack[n - 4],
                            self.stack[n - 3],
                            self.stack[n - 2] as u8,
                            self.stack[n - 1] as u8,
                        );
                        self.stack.clear();
                        if self.open {
                            path.close();
                            self.open = false;
                        }
                        for (code, origin) in [(bchar, (0.0, 0.0)), (achar, (adx, ady))] {
                            if let Some(gid) = standard(code) {
                                let mut part = Interpreter::new(self.font, usize::from(gid));
                                part.steps = self.steps;
                                part.run(usize::from(gid), &|_| None, origin, path)?;
                            }
                        }
                    }
                    return Some(true);
                }
                12 => {
                    let b1 = *code.get(i)?;
                    i += 1;
                    self.escape(b1, path)?;
                }
                _ => {}
            }
        }
        Some(false)
    }

    fn push(&mut self, value: f64) -> Option<()> {
        if self.stack.len() >= 192 {
            return None;
        }
        self.stack.push(value);
        Some(())
    }

    // The two-byte operators: flexes and arithmetic.
    fn escape(&mut self, op: u8, path: &mut Path) -> Option<()> {
        match op {
            34 => {
                let a = std::mem::take(&mut self.stack);
                if a.len() >= 7 {
                    self.curve_to([a[0], 0.0, a[1], a[2], a[3], 0.0], path);
                    self.curve_to([a[4], 0.0, a[5], -a[2], a[6], 0.0], path);
                }
            }
            35 => {
                let a = std::mem::take(&mut self.stack);
                if a.len() >= 12 {
                    self.curve_to([a[0], a[1], a[2], a[3], a[4], a[5]], path);
                    self.curve_to([a[6], a[7], a[8], a[9], a[10], a[11]], path);
                }
            }
            36 => {
                let a = std::mem::take(&mut self.stack);
                if a.len() >= 9 {
                    self.curve_to([a[0], a[1], a[2], a[3], a[4], 0.0], path);
                    self.curve_to([a[5], 0.0, a[6], a[7], a[8], -(a[1] + a[3] + a[7])], path);
                }
            }
            37 => {
                let a = std::mem::take(&mut self.stack);
                if a.len() >= 11 {
                    let (dx, dy) = (
                        a[0] + a[2] + a[4] + a[6] + a[8],
                        a[1] + a[3] + a[5] + a[7] + a[9],
                    );
                    self.curve_to([a[0], a[1], a[2], a[3], a[4], a[5]], path);
                    let last = if dx.abs() > dy.abs() {
                        [a[6], a[7], a[8], a[9], a[10], -dy]
                    } else {
                        [a[6], a[7], a[8], a[9], -dx, a[10]]
                    };
                    self.curve_to(last, path);
                }
            }
            3 => self.binary(|a, b| f64::from(u8::from(a != 0.0 && b != 0.0)))?,
            4 => self.binary(|a, b| f64::from(u8::from(a != 0.0 || b != 0.0)))?,
            5 => {
                let a = self.stack.pop()?;
                self.push(f64::from(u8::from(a == 0.0)))?;
            }
            9 => {
                let a = self.stack.pop()?;
                self.push(a.abs())?;
            }
            10 => self.binary(|a, b| a + b)?,
            11 => self.binary(|a, b| a - b)?,
            12 => self.binary(|a, b| if b == 0.0 { 0.0 } else { a / b })?,
            14 => {
                let a = self.stack.pop()?;
                self.push(-a)?;
            }
            15 => self.binary(|a, b| f64::from(u8::from(a == b)))?,
            18 => {
                self.stack.pop()?;
            }
            20 => {
                let index = self.stack.pop()? as usize;
                let value = self.stack.pop()?;
                *self.transients.get_mut(index)? = value;
            }
            21 => {
                let index = self.stack.pop()? as usize;
                let value = *self.transients.get(index)?;
                self.push(value)?;
            }
            22 => {
                let (v2, v1, s2, s1) = (
                    self.stack.pop()?,
                    self.stack.pop()?,
                    self.stack.pop()?,
                    self.stack.pop()?,
                );
                self.push(if v1 <= v2 { s1 } else { s2 })?;
            }
            23 => self.push(0.5)?,
            24 => self.binary(|a, b| a * b)?,
            26 => {
                let a = self.stack.pop()?;
                self.push(a.max(0.0).sqrt())?;
            }
            27 => {
                let a = *self.stack.last()?;
                self.push(a)?;
            }
            28 => {
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.push(b)?;
                self.push(a)?;
            }
            29 => {
                let index = self.stack.pop()?;
                let n = self.stack.len();
                let at = if index < 0.0 { 0 } else { index as usize };
                let value = *self.stack.get(n.checked_sub(1 + at)?)?;
                self.push(value)?;
            }
            30 => {
                let shift = self.stack.pop()? as i64;
                let count = self.stack.pop()? as usize;
                let n = self.stack.len();
                let start = n.checked_sub(count)?;
                if count > 0 {
                    let slice = &mut self.stack[start..];
                    let k = shift.rem_euclid(count as i64) as usize;
                    slice.rotate_right(k);
                }
            }
            _ => self.stack.clear(),
        }
        Some(())
    }

    fn binary(&mut self, f: impl Fn(f64, f64) -> f64) -> Option<()> {
        let b = self.stack.pop()?;
        let a = self.stack.pop()?;
        self.push(f(a, b))
    }
}
