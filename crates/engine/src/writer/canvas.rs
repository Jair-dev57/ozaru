//! A canvas: the content of a page being drawn, as the operators of a content
//! stream (ISO 32000-2, section 8), to add to a page with
//! [`Editor::draw`](super::Editor::draw).
//!
//! Positions are in the default user space of the page: points, from the
//! bottom left corner, y growing upwards. Text uses the standard 14 fonts,
//! which every reader has, with the characters their encodings have.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use super::font::EmbeddedFont;
use crate::base::{Error, ErrorKind, Result};
use crate::fonts::{BaseEncoding, GlyphList, StandardFont, glyph_to_unicode};

/// The name a font has in the resources of the page it is drawn on, which
/// depends only on the font, so drawing twice on a page shares it.
pub fn font_resource(font: StandardFont) -> String {
    format!("Oz{}", font.name())
}

/// The name a graphics state of opacities has in the resources of the page,
/// which depends only on them.
pub fn state_resource(fill: f64, stroke: f64) -> String {
    format!(
        "OzA{}_{}",
        (fill * 1000.0).round(),
        (stroke * 1000.0).round()
    )
}

/// How the ends of a stroked line look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// How the corners of a stroked path look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// What is drawn so far on a page. Build it with the methods, in the order
/// things are painted, and give it to the editor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Canvas {
    content: Vec<u8>,
    fonts: Vec<StandardFont>,
    // The fill and stroke opacities of each graphics state used.
    states: Vec<(f64, f64)>,
    // The fonts of the user, each with the glyphs drawn with it and the
    // character that drew each, in the order they were first used.
    embedded: Vec<(Arc<EmbeddedFont>, BTreeMap<u16, char>)>,
}

// A number as the shortest text that keeps four decimals.
fn num(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" || text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

fn finite(values: &[f64]) -> Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::NonFiniteNumber, 0))
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing has been drawn.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// The operators drawn so far.
    pub fn content(&self) -> &[u8] {
        &self.content
    }

    /// The fonts used, each under the name [`font_resource`] gives it.
    pub fn fonts(&self) -> &[StandardFont] {
        &self.fonts
    }

    /// The opacities used, each under the name [`state_resource`] gives it.
    pub fn states(&self) -> &[(f64, f64)] {
        &self.states
    }

    /// The fonts of the user drawn with, each with the glyphs drawn and a
    /// character for each. The name of the font in the content is
    /// [`embedded_placeholder`] of its position, until the editor gives it its
    /// real one.
    pub fn embedded_fonts(&self) -> &[(Arc<EmbeddedFont>, BTreeMap<u16, char>)] {
        &self.embedded
    }

    fn op(&mut self, text: &str) {
        self.content.extend_from_slice(text.as_bytes());
        self.content.push(b'\n');
    }

    /// Saves the graphics state (`q`): colors, lines, opacity and the
    /// matrix come back with [`restore`](Self::restore).
    pub fn save(&mut self) {
        self.op("q");
    }

    /// Restores the graphics state saved last (`Q`).
    pub fn restore(&mut self) {
        self.op("Q");
    }

    /// Multiplies the current matrix by `[a b c d e f]` (`cm`): everything drawn
    /// after is moved, turned or scaled by it.
    pub fn transform(&mut self, matrix: [f64; 6]) -> Result<()> {
        finite(&matrix)?;
        let numbers: Vec<String> = matrix.iter().map(|v| num(*v)).collect();
        self.op(&format!("{} cm", numbers.join(" ")));
        Ok(())
    }

    /// Moves the origin to `(x, y)` and turns what comes after counterclockwise
    /// by `degrees` around it.
    pub fn rotate_around(&mut self, x: f64, y: f64, degrees: f64) -> Result<()> {
        finite(&[x, y, degrees])?;
        let (sin, cos) = degrees.to_radians().sin_cos();
        self.transform([cos, sin, -sin, cos, x, y])
    }

    /// The color areas are filled with, red, green and blue from 0 to 255.
    pub fn set_fill_color(&mut self, [red, green, blue]: [u8; 3]) {
        let level = |v: u8| num(f64::from(v) / 255.0);
        self.op(&format!(
            "{} {} {} rg",
            level(red),
            level(green),
            level(blue)
        ));
    }

    /// The color lines are stroked with.
    pub fn set_stroke_color(&mut self, [red, green, blue]: [u8; 3]) {
        let level = |v: u8| num(f64::from(v) / 255.0);
        self.op(&format!(
            "{} {} {} RG",
            level(red),
            level(green),
            level(blue)
        ));
    }

    /// The width of stroked lines, in points.
    pub fn set_line_width(&mut self, width: f64) -> Result<()> {
        finite(&[width])?;
        self.op(&format!("{} w", num(width.max(0.0))));
        Ok(())
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        let value = match cap {
            LineCap::Butt => 0,
            LineCap::Round => 1,
            LineCap::Square => 2,
        };
        self.op(&format!("{value} J"));
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        let value = match join {
            LineJoin::Miter => 0,
            LineJoin::Round => 1,
            LineJoin::Bevel => 2,
        };
        self.op(&format!("{value} j"));
    }

    /// A dashed line: `lengths` alternate between dash and gap, starting
    /// `phase` points in. An empty list is a solid line.
    pub fn set_dash(&mut self, lengths: &[f64], phase: f64) -> Result<()> {
        finite(lengths)?;
        finite(&[phase])?;
        let items: Vec<String> = lengths.iter().map(|v| num(v.max(0.0))).collect();
        self.op(&format!("[{}] {} d", items.join(" "), num(phase)));
        Ok(())
    }

    /// How opaque what is filled and what is stroked are after this, from 0
    /// (invisible) to 1 (solid).
    pub fn set_opacity(&mut self, fill: f64, stroke: f64) -> Result<()> {
        finite(&[fill, stroke])?;
        let state = (fill.clamp(0.0, 1.0), stroke.clamp(0.0, 1.0));
        if !self.states.contains(&state) {
            self.states.push(state);
        }
        self.op(&format!("/{} gs", state_resource(state.0, state.1)));
        Ok(())
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<()> {
        finite(&[x, y])?;
        self.op(&format!("{} {} m", num(x), num(y)));
        Ok(())
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> Result<()> {
        finite(&[x, y])?;
        self.op(&format!("{} {} l", num(x), num(y)));
        Ok(())
    }

    /// A Bézier curve to `(x, y)` with the control points `(x1, y1)` and `(x2, y2)`.
    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) -> Result<()> {
        finite(&[x1, y1, x2, y2, x, y])?;
        let numbers: Vec<String> = [x1, y1, x2, y2, x, y].iter().map(|v| num(*v)).collect();
        self.op(&format!("{} c", numbers.join(" ")));
        Ok(())
    }

    /// Closes the path with a line to where it started.
    pub fn close_path(&mut self) {
        self.op("h");
    }

    /// A rectangle with its bottom left corner at `(x, y)`.
    pub fn rectangle(&mut self, x: f64, y: f64, width: f64, height: f64) -> Result<()> {
        finite(&[x, y, width, height])?;
        self.op(&format!(
            "{} {} {} {} re",
            num(x),
            num(y),
            num(width),
            num(height)
        ));
        Ok(())
    }

    /// An ellipse inside the rectangle from `(x0, y0)` to `(x1, y1)`, made of
    /// four curves.
    pub fn ellipse(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> Result<()> {
        finite(&[x0, y0, x1, y1])?;
        // The control points that make a Bézier curve a quarter of an ellipse.
        const K: f64 = 0.552_284_749_830_793_4;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
        self.move_to(cx + rx, cy)?;
        self.curve_to(cx + rx, cy + K * ry, cx + K * rx, cy + ry, cx, cy + ry)?;
        self.curve_to(cx - K * rx, cy + ry, cx - rx, cy + K * ry, cx - rx, cy)?;
        self.curve_to(cx - rx, cy - K * ry, cx - K * rx, cy - ry, cx, cy - ry)?;
        self.curve_to(cx + K * rx, cy - ry, cx + rx, cy - K * ry, cx + rx, cy)?;
        self.close_path();
        Ok(())
    }

    /// Fills the path with the fill color (`f`).
    pub fn fill(&mut self) {
        self.op("f");
    }

    /// Strokes the path with the stroke color (`S`).
    pub fn stroke(&mut self) {
        self.op("S");
    }

    /// Fills and strokes the path (`B`).
    pub fn fill_and_stroke(&mut self) {
        self.op("B");
    }

    /// Ends the path without painting it (`n`).
    pub fn end_path(&mut self) {
        self.op("n");
    }

    /// Draws `text` in `font` at `size` points, its baseline starting at
    /// `(x, y)`, filled with the fill color.
    ///
    /// It is an `UnsupportedCharacter` error if the font doesn't have a
    /// character of the text: the standard fonts have the Latin letters of
    /// Western Europe, and Symbol and ZapfDingbats their own signs.
    pub fn text(
        &mut self,
        x: f64,
        y: f64,
        text: &str,
        font: StandardFont,
        size: f64,
    ) -> Result<()> {
        finite(&[x, y, size])?;
        let bytes = encode(font, text)?;
        if !self.fonts.contains(&font) {
            self.fonts.push(font);
        }
        let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
        self.op("BT");
        self.op(&format!("/{} {} Tf", font_resource(font), num(size)));
        self.op(&format!("{} {} Td", num(x), num(y)));
        self.op(&format!("<{hex}> Tj"));
        self.op("ET");
        Ok(())
    }

    /// Draws `text` in a font of the user at `size` points, as
    /// [`text`](Self::text) does with the standard fonts. Only the glyphs
    /// drawn are embedded in the document.
    ///
    /// It is an `UnsupportedCharacter` error if the font doesn't have a
    /// character of the text.
    pub fn text_embedded(
        &mut self,
        x: f64,
        y: f64,
        text: &str,
        font: &Arc<EmbeddedFont>,
        size: f64,
    ) -> Result<()> {
        finite(&[x, y, size])?;
        let glyphs = font.glyphs(text)?;
        let at = match self
            .embedded
            .iter()
            .position(|(known, _)| Arc::ptr_eq(known, font) || **known == **font)
        {
            Some(at) => at,
            None => {
                self.embedded.push((Arc::clone(font), BTreeMap::new()));
                self.embedded.len() - 1
            }
        };
        let hex: String = glyphs.iter().map(|(gid, _)| format!("{gid:04X}")).collect();
        for (gid, character) in glyphs {
            self.embedded[at].1.entry(gid).or_insert(character);
        }
        self.op("BT");
        self.op(&format!("/{} {} Tf", embedded_placeholder(at), num(size)));
        self.op(&format!("{} {} Td", num(x), num(y)));
        self.op(&format!("<{hex}> Tj"));
        self.op("ET");
        Ok(())
    }
}

/// The name the canvas gives, in its content, to the font of the user at
/// position `index` of [`Canvas::embedded_fonts`]; the editor replaces it with
/// one that depends on the glyphs the font ends up with.
pub fn embedded_placeholder(index: usize) -> String {
    format!("OzEmbedded{index}_")
}

/// The width of `text` in `font` at `size` points. It is an
/// `UnsupportedCharacter` error if the font doesn't have a character.
pub fn text_width(font: StandardFont, size: f64, text: &str) -> Result<f64> {
    let encoding = font.built_in_encoding();
    let encoding = if matches!(encoding, BaseEncoding::Standard) {
        BaseEncoding::WinAnsi
    } else {
        encoding
    };
    let mut total = 0.0;
    for code in encode(font, text)? {
        let width = encoding
            .glyph_name(code)
            .and_then(|name| font.glyph_width(name.as_bytes()))
            .unwrap_or(0.0);
        total += width;
    }
    Ok(total * size / 1000.0)
}

// The codes of `text` in the encoding the canvas writes `font` with.
fn encode(font: StandardFont, text: &str) -> Result<Vec<u8>> {
    let table = code_table(font);
    text.chars()
        .map(|c| {
            table
                .get(&c)
                .copied()
                .ok_or_else(|| Error::new(ErrorKind::UnsupportedCharacter, 0))
        })
        .collect()
}

// Each font's characters and their codes: WinAnsi for the text fonts, their own
// encoding for Symbol and ZapfDingbats. The lowest code of a character wins.
fn code_table(font: StandardFont) -> &'static HashMap<char, u8> {
    static TABLES: OnceLock<Vec<HashMap<char, u8>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        let fonts = [
            StandardFont::Courier,
            StandardFont::CourierBold,
            StandardFont::CourierOblique,
            StandardFont::CourierBoldOblique,
            StandardFont::Helvetica,
            StandardFont::HelveticaBold,
            StandardFont::HelveticaOblique,
            StandardFont::HelveticaBoldOblique,
            StandardFont::TimesRoman,
            StandardFont::TimesBold,
            StandardFont::TimesItalic,
            StandardFont::TimesBoldItalic,
            StandardFont::Symbol,
            StandardFont::ZapfDingbats,
        ];
        fonts
            .iter()
            .map(|font| {
                let (encoding, list) = match font {
                    StandardFont::Symbol => (BaseEncoding::Symbol, GlyphList::Adobe),
                    StandardFont::ZapfDingbats => {
                        (BaseEncoding::ZapfDingbats, GlyphList::ZapfDingbats)
                    }
                    _ => (BaseEncoding::WinAnsi, GlyphList::Adobe),
                };
                let mut table = HashMap::new();
                for code in 32..=255_u8 {
                    let Some(name) = encoding.glyph_name(code) else {
                        continue;
                    };
                    // A font only draws the glyphs it has.
                    if font.glyph_width(name.as_bytes()).is_none() {
                        continue;
                    }
                    let Some(unicode) = glyph_to_unicode(name.as_bytes(), list) else {
                        continue;
                    };
                    let mut chars = unicode.chars();
                    if let (Some(c), None) = (chars.next(), chars.next()) {
                        table.entry(c).or_insert(code);
                    }
                }
                table
            })
            .collect()
    });
    &tables[font as usize]
}
