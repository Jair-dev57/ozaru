//! Type 1 fonts (Adobe Type 1 Font Format): the clear text, the eexec
//! encrypted private part, and the charstrings of its glyphs.

use std::collections::HashMap;

use crate::render::Path;

const EEXEC_KEY: u16 = 55665;
const CHARSTRING_KEY: u16 = 4330;

fn decrypt(data: &[u8], key: u16, skip: usize) -> Vec<u8> {
    let mut r = key;
    let mut out = Vec::with_capacity(data.len());
    for &cipher in data {
        out.push(cipher ^ (r >> 8) as u8);
        r = u16::from(cipher)
            .wrapping_add(r)
            .wrapping_mul(52845)
            .wrapping_add(22719);
    }
    out.drain(..skip.min(out.len()));
    out
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

// A font file as the PFB format has it: segments after 0x80 headers. PDF
// streams hold the segments' data, but a PFB is accepted too.
fn strip_pfb(data: &[u8]) -> Vec<u8> {
    if data.first() != Some(&0x80) {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i + 6 <= data.len() && data[i] == 0x80 {
        let kind = data[i + 1];
        if kind == 3 {
            break;
        }
        let length =
            u32::from_le_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]) as usize;
        let end = (i + 6 + length).min(data.len());
        out.extend_from_slice(&data[i + 6..end]);
        i = end;
    }
    out
}

/// A Type 1 font, read for its glyph outlines.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1 {
    /// From glyph space to text space.
    pub font_matrix: [f64; 6],
    subrs: HashMap<usize, Vec<u8>>,
    glyphs: HashMap<Vec<u8>, Vec<u8>>,
}

impl Type1 {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let data = strip_pfb(data);
        let eexec = find(&data, b"eexec")?;
        let clear = &data[..eexec];
        let mut start = eexec + 5;
        while data
            .get(start)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            start += 1;
        }
        let encrypted = data.get(start..)?;
        // Hexadecimal, if the first four bytes are digits.
        let binary: Vec<u8>;
        let cipher = if encrypted.len() >= 4 && encrypted[..4].iter().all(u8::is_ascii_hexdigit) {
            let mut bytes = Vec::with_capacity(encrypted.len() / 2);
            let mut high: Option<u8> = None;
            for &b in encrypted {
                let digit = match b {
                    b'0'..=b'9' => b - b'0',
                    b'a'..=b'f' => b - b'a' + 10,
                    b'A'..=b'F' => b - b'A' + 10,
                    _ => continue,
                };
                match high.take() {
                    None => high = Some(digit),
                    Some(h) => bytes.push(h << 4 | digit),
                }
            }
            binary = bytes;
            &binary[..]
        } else {
            encrypted
        };
        let private = decrypt(cipher, EEXEC_KEY, 4);
        let font_matrix = read_font_matrix(clear).unwrap_or([0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
        let (subrs, glyphs) = read_private(&private);
        if glyphs.is_empty() {
            return None;
        }
        Some(Self {
            font_matrix,
            subrs,
            glyphs,
        })
    }

    pub fn has_glyph(&self, name: &[u8]) -> bool {
        self.glyphs.contains_key(name)
    }

    pub fn glyph_names(&self) -> impl Iterator<Item = &[u8]> {
        self.glyphs.keys().map(Vec::as_slice)
    }

    /// The outline of the glyph `name`, in glyph space units, y up.
    /// `standard` gives the name of a Standard encoding code, for accented
    /// characters made of two glyphs.
    pub fn outline(
        &self,
        name: &[u8],
        standard: &dyn Fn(u8) -> Option<&'static str>,
    ) -> Option<Path> {
        let mut path = Path::new();
        let mut interpreter = Interpreter::new(self);
        interpreter.run(name, (0.0, 0.0), standard, &mut path)?;
        Some(path)
    }
}

fn read_font_matrix(clear: &[u8]) -> Option<[f64; 6]> {
    let at = find(clear, b"/FontMatrix")?;
    let rest = &clear[at + 11..];
    let open = rest.iter().position(|&b| b == b'[' || b == b'{')?;
    let close = open + rest[open..].iter().position(|&b| b == b']' || b == b'}')?;
    let text = std::str::from_utf8(&rest[open + 1..close]).ok()?;
    let numbers: Vec<f64> = text
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let matrix: [f64; 6] = numbers.try_into().ok()?;
    (matrix[0] != 0.0).then_some(matrix)
}

fn is_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace()
        || matches!(
            b,
            b'/' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'%'
        )
}

// The next token: a name with its slash, or a run of other characters.
fn token<'a>(text: &'a [u8], i: &mut usize) -> Option<&'a [u8]> {
    while text.get(*i).is_some_and(|b| b.is_ascii_whitespace()) {
        *i += 1;
    }
    let start = *i;
    let first = *text.get(start)?;
    *i += 1;
    if matches!(first, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}') {
        return Some(&text[start..*i]);
    }
    while text.get(*i).is_some_and(|&b| !is_delimiter(b)) {
        *i += 1;
    }
    Some(&text[start..*i])
}

fn int(token: &[u8]) -> Option<i64> {
    std::str::from_utf8(token).ok()?.parse().ok()
}

type Subrs = HashMap<usize, Vec<u8>>;
type Glyphs = HashMap<Vec<u8>, Vec<u8>>;

fn read_private(text: &[u8]) -> (Subrs, Glyphs) {
    let mut len_iv = 4usize;
    let mut subrs = HashMap::new();
    let mut glyphs = HashMap::new();
    let mut in_chars = false;
    let mut i = 0;
    let mut history: [&[u8]; 4] = [b"", b"", b"", b""];
    while let Some(t) = token(text, &mut i) {
        if t == b"/lenIV" {
            if let Some(n) = token(text, &mut i).and_then(int) {
                len_iv = usize::try_from(n).unwrap_or(4).min(16);
            }
        } else if t == b"/CharStrings" {
            in_chars = true;
        } else if t == b"RD" || t == b"-|" {
            // `dup index length RD data` or `/name length RD data`.
            let length = int(history[3]).and_then(|n| usize::try_from(n).ok());
            if let Some(length) = length {
                let body_start = i + 1;
                if let Some(body) = text.get(body_start..body_start + length) {
                    let code = decrypt(body, CHARSTRING_KEY, len_iv);
                    if history[1] == b"dup" {
                        if let Some(index) = int(history[2]).and_then(|n| usize::try_from(n).ok()) {
                            subrs.insert(index, code);
                        }
                    } else if in_chars && history[2].first() == Some(&b'/') {
                        glyphs.insert(history[2][1..].to_vec(), code);
                    }
                    i = body_start + length;
                }
            }
        }
        history = [history[1], history[2], history[3], t];
    }
    (subrs, glyphs)
}

const MAX_STEPS: usize = 500_000;
const MAX_DEPTH: usize = 10;

struct Interpreter<'f> {
    font: &'f Type1,
    stack: Vec<f64>,
    postscript: Vec<f64>,
    x: f64,
    y: f64,
    // The side bearing of the glyph being drawn.
    sbx: f64,
    open: bool,
    flex: Option<Vec<(f64, f64)>>,
    steps: usize,
}

impl<'f> Interpreter<'f> {
    fn new(font: &'f Type1) -> Self {
        Self {
            font,
            stack: Vec::new(),
            postscript: Vec::new(),
            x: 0.0,
            y: 0.0,
            sbx: 0.0,
            open: false,
            flex: None,
            steps: 0,
        }
    }

    fn run(
        &mut self,
        name: &[u8],
        origin: (f64, f64),
        standard: &dyn Fn(u8) -> Option<&'static str>,
        path: &mut Path,
    ) -> Option<()> {
        let code = self.font.glyphs.get(name)?.clone();
        self.x = origin.0;
        self.y = origin.1;
        self.stack.clear();
        self.execute(&code, 0, origin, standard, path)?;
        if self.open {
            path.close();
            self.open = false;
        }
        Some(())
    }

    fn move_to(&mut self, dx: f64, dy: f64, path: &mut Path) {
        self.x += dx;
        self.y += dy;
        if let Some(points) = &mut self.flex {
            points.push((self.x, self.y));
            return;
        }
        if self.open {
            path.close();
        }
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

    fn execute(
        &mut self,
        code: &[u8],
        depth: usize,
        origin: (f64, f64),
        standard: &dyn Fn(u8) -> Option<&'static str>,
        path: &mut Path,
    ) -> Option<bool> {
        let mut i = 0;
        while i < code.len() {
            self.steps += 1;
            if self.steps > MAX_STEPS || self.stack.len() > 64 {
                return None;
            }
            let b0 = code[i];
            i += 1;
            match b0 {
                32..=246 => self.stack.push(f64::from(b0) - 139.0),
                247..=250 => {
                    self.stack
                        .push((f64::from(b0) - 247.0) * 256.0 + f64::from(*code.get(i)?) + 108.0);
                    i += 1;
                }
                251..=254 => {
                    self.stack
                        .push(-(f64::from(b0) - 251.0) * 256.0 - f64::from(*code.get(i)?) - 108.0);
                    i += 1;
                }
                255 => {
                    let v = i32::from_be_bytes(code.get(i..i + 4)?.try_into().ok()?);
                    self.stack.push(f64::from(v));
                    i += 4;
                }
                13 => {
                    // hsbw: the side bearing is where the outline starts.
                    let n = self.stack.len();
                    if n >= 2 {
                        self.sbx = self.stack[n - 2];
                        self.x = origin.0 + self.sbx;
                        self.y = origin.1;
                    }
                    self.stack.clear();
                }
                1 | 3 => self.stack.clear(),
                4 => {
                    let dy = self.stack.last().copied().unwrap_or(0.0);
                    self.move_to(0.0, dy, path);
                    self.stack.clear();
                }
                5 => {
                    let n = self.stack.len();
                    if n >= 2 {
                        let (dx, dy) = (self.stack[n - 2], self.stack[n - 1]);
                        self.line_to(dx, dy, path);
                    }
                    self.stack.clear();
                }
                6 => {
                    let dx = self.stack.last().copied().unwrap_or(0.0);
                    self.line_to(dx, 0.0, path);
                    self.stack.clear();
                }
                7 => {
                    let dy = self.stack.last().copied().unwrap_or(0.0);
                    self.line_to(0.0, dy, path);
                    self.stack.clear();
                }
                8 => {
                    let n = self.stack.len();
                    if n >= 6 {
                        let s = &self.stack[n - 6..];
                        let d = [s[0], s[1], s[2], s[3], s[4], s[5]];
                        self.curve_to(d, path);
                    }
                    self.stack.clear();
                }
                9 => {
                    if self.open {
                        path.close();
                        self.open = false;
                    }
                    self.stack.clear();
                }
                10 => {
                    let index = self.stack.pop()? as i64;
                    let body = self.font.subrs.get(&usize::try_from(index).ok()?)?.clone();
                    if depth >= MAX_DEPTH {
                        return None;
                    }
                    if self.execute(&body, depth + 1, origin, standard, path)? {
                        return Some(true);
                    }
                }
                11 => return Some(false),
                14 => return Some(true),
                21 => {
                    let n = self.stack.len();
                    if n >= 2 {
                        let (dx, dy) = (self.stack[n - 2], self.stack[n - 1]);
                        self.move_to(dx, dy, path);
                    }
                    self.stack.clear();
                }
                22 => {
                    let dx = self.stack.last().copied().unwrap_or(0.0);
                    self.move_to(dx, 0.0, path);
                    self.stack.clear();
                }
                30 => {
                    let n = self.stack.len();
                    if n >= 4 {
                        let s = &self.stack[n - 4..];
                        let d = [0.0, s[0], s[1], s[2], s[3], 0.0];
                        self.curve_to(d, path);
                    }
                    self.stack.clear();
                }
                31 => {
                    let n = self.stack.len();
                    if n >= 4 {
                        let s = &self.stack[n - 4..];
                        let d = [s[0], 0.0, s[1], s[2], 0.0, s[3]];
                        self.curve_to(d, path);
                    }
                    self.stack.clear();
                }
                12 => {
                    let b1 = *code.get(i)?;
                    i += 1;
                    match b1 {
                        // dotsection, vstem3, hstem3.
                        0..=2 => self.stack.clear(),
                        6 => {
                            let n = self.stack.len();
                            if n >= 5 {
                                let s: Vec<f64> = self.stack[n - 5..].to_vec();
                                self.stack.clear();
                                self.seac(s, origin, standard, path)?;
                            }
                            return Some(true);
                        }
                        7 => {
                            let n = self.stack.len();
                            if n >= 4 {
                                self.sbx = self.stack[n - 4];
                                self.x = origin.0 + self.sbx;
                                self.y = origin.1 + self.stack[n - 3];
                            }
                            self.stack.clear();
                        }
                        12 => {
                            let b = self.stack.pop()?;
                            let a = self.stack.pop()?;
                            self.stack.push(if b == 0.0 { 0.0 } else { a / b });
                        }
                        16 => self.other_subr(path)?,
                        17 => {
                            let value = self.postscript.pop().unwrap_or(0.0);
                            self.stack.push(value);
                        }
                        33 => {
                            let n = self.stack.len();
                            if n >= 2 {
                                self.x = self.stack[n - 2];
                                self.y = self.stack[n - 1];
                            }
                            self.stack.clear();
                        }
                        _ => self.stack.clear(),
                    }
                }
                _ => self.stack.clear(),
            }
        }
        Some(false)
    }

    // `callothersubr`: the flex and hint replacement procedures; the others
    // just move their arguments to the PostScript stack.
    fn other_subr(&mut self, path: &mut Path) -> Option<()> {
        let number = self.stack.pop()? as i64;
        let count = self.stack.pop()? as usize;
        let mut args = Vec::with_capacity(count);
        for _ in 0..count.min(self.stack.len()) {
            args.push(self.stack.pop()?);
        }
        // `args` has the last argument first.
        match number {
            1 => self.flex = Some(Vec::new()),
            2 => {}
            0 => {
                let points = self.flex.take().unwrap_or_default();
                if points.len() >= 7 {
                    let curve = |path: &mut Path, p: [(f64, f64); 3]| {
                        path.curve_to(p[0].0, p[0].1, p[1].0, p[1].1, p[2].0, p[2].1);
                    };
                    curve(path, [points[1], points[2], points[3]]);
                    curve(path, [points[4], points[5], points[6]]);
                    self.x = points[6].0;
                    self.y = points[6].1;
                }
                // The end point, which `pop pop setcurrentpoint` sets.
                if args.len() >= 2 {
                    self.postscript.push(args[0]);
                    self.postscript.push(args[1]);
                }
            }
            _ => {
                for &arg in args.iter().rev() {
                    self.postscript.push(arg);
                }
                self.postscript.reverse();
            }
        }
        Some(())
    }

    fn seac(
        &mut self,
        s: Vec<f64>,
        origin: (f64, f64),
        standard: &dyn Fn(u8) -> Option<&'static str>,
        path: &mut Path,
    ) -> Option<()> {
        let (asb, adx, ady, bchar, achar) = (s[0], s[1], s[2], s[3] as u8, s[4] as u8);
        if self.open {
            path.close();
            self.open = false;
        }
        let base = standard(bchar)?;
        let accent = standard(achar)?;
        let mut part = Interpreter::new(self.font);
        part.steps = self.steps;
        part.run(base.as_bytes(), origin, &|_| None, path)?;
        let mut part = Interpreter::new(self.font);
        let shift = (origin.0 + self.sbx + adx - asb, origin.1 + ady);
        // The accent's own origin is moved so that its side bearing lands
        // where the composite's says.
        part.run(accent.as_bytes(), (shift.0 - 0.0, shift.1), &|_| None, path)?;
        Some(())
    }
}
