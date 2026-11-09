use std::collections::BTreeMap;

use super::resolved;
use crate::base::{DiagnosticKind, Diagnostics};
use crate::syntax::{Name, Object};
use crate::xref::Resolver;

/// A predefined encoding of single-byte character codes to glyph names
/// (ISO 32000-2, Annex D).
///
/// `MacRoman` and `WinAnsi` can be named in a font's `/Encoding`. `Standard`,
/// `Symbol` and `ZapfDingbats` are the built-in encodings of the standard 14
/// fonts. PDFDocEncoding isn't a font encoding: it is for text strings, and is
/// decoded by [`crate::document::decode_text_string`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseEncoding {
    /// Adobe standard Latin-text encoding, the built-in encoding of most Type 1
    /// fonts.
    Standard,
    /// Mac OS standard encoding for Latin text.
    MacRoman,
    /// Windows code page 1252.
    WinAnsi,
    /// The built-in encoding of the Symbol font.
    Symbol,
    /// The built-in encoding of the ZapfDingbats font.
    ZapfDingbats,
}

impl BaseEncoding {
    /// The encoding with the name `name`, as it appears in `/Encoding` or
    /// `/BaseEncoding`. `StandardEncoding` isn't a valid value, but some writers
    /// use it, so it is accepted.
    pub fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"WinAnsiEncoding" => Some(Self::WinAnsi),
            b"MacRomanEncoding" => Some(Self::MacRoman),
            b"StandardEncoding" => Some(Self::Standard),
            _ => None,
        }
    }

    /// The glyph name of `code`, or `None` if the encoding doesn't define it.
    pub fn glyph_name(self, code: u8) -> Option<&'static str> {
        let table = match self {
            Self::Standard => &STANDARD,
            Self::MacRoman => &MAC_ROMAN,
            Self::WinAnsi => &WIN_ANSI,
            Self::Symbol => &SYMBOL,
            Self::ZapfDingbats => &ZAPF_DINGBATS,
        };
        table[usize::from(code)]
    }
}

/// The encoding of a simple font: a base encoding with the changes of its
/// `/Differences` array (ISO 32000-2, section 9.6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoding {
    base: BaseEncoding,
    // False for the built-in encoding of a font program, which only has its
    // own codes.
    pub(crate) base_used: bool,
    differences: BTreeMap<u8, Name>,
}

impl Encoding {
    /// `base` with no differences.
    pub fn new(base: BaseEncoding) -> Self {
        Self {
            base,
            base_used: true,
            differences: BTreeMap::new(),
        }
    }

    /// The base encoding. For the built-in encoding of a font program, which
    /// has no base, `Standard`, which isn't used: see `has_base`.
    pub fn base(&self) -> BaseEncoding {
        self.base
    }

    /// Whether codes that the differences don't change use the base encoding.
    /// False for the built-in encoding of an embedded Type 1 font.
    pub fn has_base(&self) -> bool {
        self.base_used
    }

    /// The codes that `/Differences` changes, with their glyph names.
    pub fn differences(&self) -> &BTreeMap<u8, Name> {
        &self.differences
    }

    /// Gives `code` the glyph name `name`.
    pub fn set(&mut self, code: u8, name: Name) {
        self.differences.insert(code, name);
    }

    /// The glyph name of `code`: the one from `/Differences` if it has one, and
    /// the base encoding's otherwise.
    pub fn glyph_name(&self, code: u8) -> Option<&[u8]> {
        match self.differences.get(&code) {
            Some(name) => Some(name.as_bytes()),
            None if self.base_used => self.base.glyph_name(code).map(str::as_bytes),
            None => None,
        }
    }
}

/// Reads a font's `/Encoding` entry, a name or an encoding dictionary, possibly
/// through references (ISO 32000-2, section 9.6.5).
///
/// `built_in` is the font's built-in encoding, used when the entry doesn't name a
/// base encoding: a predefined one, or the one in an embedded font program. An
/// entry that can't be used, an unknown name and invalid items of
/// `/Differences` are reported at `offset`, the font's offset.
pub fn read_encoding(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    built_in: &Encoding,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> Encoding {
    let entry = resolved(resolver, entry);
    match entry.as_ref() {
        Object::Name(name) => base_encoding(name, built_in, offset, diagnostics),
        Object::Dictionary(dictionary) => {
            let mut encoding = match dictionary
                .get("BaseEncoding")
                .map(|base| resolved(resolver, base))
            {
                Some(base) => match base.as_ref() {
                    Object::Name(name) => base_encoding(name, built_in, offset, diagnostics),
                    _ => {
                        diagnostics.push(DiagnosticKind::InvalidEncoding, offset);
                        built_in.clone()
                    }
                },
                None => built_in.clone(),
            };
            if let Some(differences) = dictionary.get("Differences") {
                let differences = resolved(resolver, differences);
                match differences.as_array() {
                    Some(items) => apply_differences(&mut encoding, items, offset, diagnostics),
                    None => diagnostics.push(DiagnosticKind::InvalidDifferences, offset),
                }
            }
            encoding
        }
        _ => {
            diagnostics.push(DiagnosticKind::InvalidEncoding, offset);
            built_in.clone()
        }
    }
}

// The encoding that `name` names, or `built_in` if it isn't known.
fn base_encoding(
    name: &Name,
    built_in: &Encoding,
    offset: usize,
    diagnostics: &mut Diagnostics,
) -> Encoding {
    match BaseEncoding::from_name(name.as_bytes()) {
        Some(base) => Encoding::new(base),
        None => {
            diagnostics.push(DiagnosticKind::UnknownEncoding, offset);
            built_in.clone()
        }
    }
}

// `/Differences` is a list of codes, each followed by the names of the glyphs
// from that code on: `[32 /space 65 /A /B]` gives 32 to `space`, 65 to `A` and
// 66 to `B`. Names before the first code, and those after code 255, are skipped.
fn apply_differences(
    encoding: &mut Encoding,
    items: &[Object],
    offset: usize,
    diagnostics: &mut Diagnostics,
) {
    let mut code: Option<i64> = None;
    let mut invalid = false;
    for item in items {
        match item {
            Object::Integer(start) => code = Some(*start),
            Object::Name(name) => match code.and_then(|code| u8::try_from(code).ok()) {
                Some(byte) => {
                    encoding.set(byte, name.clone());
                    code = Some(i64::from(byte) + 1);
                }
                None => invalid = true,
            },
            _ => invalid = true,
        }
    }
    if invalid {
        diagnostics.push(DiagnosticKind::InvalidDifferences, offset);
    }
}

type Table = [Option<&'static str>; 256];

// The Latin character set (Annex D.2): each glyph with its code, in octal as in
// the standard, in the standard, Mac OS and Windows encodings. 0 means that the
// encoding doesn't have it.
const LATIN: &[(&str, u8, u8, u8)] = &[
    ("A", 0o101, 0o101, 0o101),
    ("AE", 0o341, 0o256, 0o306),
    ("Aacute", 0, 0o347, 0o301),
    ("Acircumflex", 0, 0o345, 0o302),
    ("Adieresis", 0, 0o200, 0o304),
    ("Agrave", 0, 0o313, 0o300),
    ("Aring", 0, 0o201, 0o305),
    ("Atilde", 0, 0o314, 0o303),
    ("B", 0o102, 0o102, 0o102),
    ("C", 0o103, 0o103, 0o103),
    ("Ccedilla", 0, 0o202, 0o307),
    ("D", 0o104, 0o104, 0o104),
    ("E", 0o105, 0o105, 0o105),
    ("Eacute", 0, 0o203, 0o311),
    ("Ecircumflex", 0, 0o346, 0o312),
    ("Edieresis", 0, 0o350, 0o313),
    ("Egrave", 0, 0o351, 0o310),
    ("Eth", 0, 0, 0o320),
    ("Euro", 0, 0, 0o200),
    ("F", 0o106, 0o106, 0o106),
    ("G", 0o107, 0o107, 0o107),
    ("H", 0o110, 0o110, 0o110),
    ("I", 0o111, 0o111, 0o111),
    ("Iacute", 0, 0o352, 0o315),
    ("Icircumflex", 0, 0o353, 0o316),
    ("Idieresis", 0, 0o354, 0o317),
    ("Igrave", 0, 0o355, 0o314),
    ("J", 0o112, 0o112, 0o112),
    ("K", 0o113, 0o113, 0o113),
    ("L", 0o114, 0o114, 0o114),
    ("Lslash", 0o350, 0, 0),
    ("M", 0o115, 0o115, 0o115),
    ("N", 0o116, 0o116, 0o116),
    ("Ntilde", 0, 0o204, 0o321),
    ("O", 0o117, 0o117, 0o117),
    ("OE", 0o352, 0o316, 0o214),
    ("Oacute", 0, 0o356, 0o323),
    ("Ocircumflex", 0, 0o357, 0o324),
    ("Odieresis", 0, 0o205, 0o326),
    ("Ograve", 0, 0o361, 0o322),
    ("Oslash", 0o351, 0o257, 0o330),
    ("Otilde", 0, 0o315, 0o325),
    ("P", 0o120, 0o120, 0o120),
    ("Q", 0o121, 0o121, 0o121),
    ("R", 0o122, 0o122, 0o122),
    ("S", 0o123, 0o123, 0o123),
    ("Scaron", 0, 0, 0o212),
    ("T", 0o124, 0o124, 0o124),
    ("Thorn", 0, 0, 0o336),
    ("U", 0o125, 0o125, 0o125),
    ("Uacute", 0, 0o362, 0o332),
    ("Ucircumflex", 0, 0o363, 0o333),
    ("Udieresis", 0, 0o206, 0o334),
    ("Ugrave", 0, 0o364, 0o331),
    ("V", 0o126, 0o126, 0o126),
    ("W", 0o127, 0o127, 0o127),
    ("X", 0o130, 0o130, 0o130),
    ("Y", 0o131, 0o131, 0o131),
    ("Yacute", 0, 0, 0o335),
    ("Ydieresis", 0, 0o331, 0o237),
    ("Z", 0o132, 0o132, 0o132),
    ("Zcaron", 0, 0, 0o216),
    ("a", 0o141, 0o141, 0o141),
    ("aacute", 0, 0o207, 0o341),
    ("acircumflex", 0, 0o211, 0o342),
    ("acute", 0o302, 0o253, 0o264),
    ("adieresis", 0, 0o212, 0o344),
    ("ae", 0o361, 0o276, 0o346),
    ("agrave", 0, 0o210, 0o340),
    ("ampersand", 0o046, 0o046, 0o046),
    ("aring", 0, 0o214, 0o345),
    ("asciicircum", 0o136, 0o136, 0o136),
    ("asciitilde", 0o176, 0o176, 0o176),
    ("asterisk", 0o052, 0o052, 0o052),
    ("at", 0o100, 0o100, 0o100),
    ("atilde", 0, 0o213, 0o343),
    ("b", 0o142, 0o142, 0o142),
    ("backslash", 0o134, 0o134, 0o134),
    ("bar", 0o174, 0o174, 0o174),
    ("braceleft", 0o173, 0o173, 0o173),
    ("braceright", 0o175, 0o175, 0o175),
    ("bracketleft", 0o133, 0o133, 0o133),
    ("bracketright", 0o135, 0o135, 0o135),
    ("breve", 0o306, 0o371, 0),
    ("brokenbar", 0, 0, 0o246),
    ("bullet", 0o267, 0o245, 0o225),
    ("c", 0o143, 0o143, 0o143),
    ("caron", 0o317, 0o377, 0),
    ("ccedilla", 0, 0o215, 0o347),
    ("cedilla", 0o313, 0o374, 0o270),
    ("cent", 0o242, 0o242, 0o242),
    ("circumflex", 0o303, 0o366, 0o210),
    ("colon", 0o072, 0o072, 0o072),
    ("comma", 0o054, 0o054, 0o054),
    ("copyright", 0, 0o251, 0o251),
    ("currency", 0o250, 0o333, 0o244),
    ("d", 0o144, 0o144, 0o144),
    ("dagger", 0o262, 0o240, 0o206),
    ("daggerdbl", 0o263, 0o340, 0o207),
    ("degree", 0, 0o241, 0o260),
    ("dieresis", 0o310, 0o254, 0o250),
    ("divide", 0, 0o326, 0o367),
    ("dollar", 0o044, 0o044, 0o044),
    ("dotaccent", 0o307, 0o372, 0),
    ("dotlessi", 0o365, 0o365, 0),
    ("e", 0o145, 0o145, 0o145),
    ("eacute", 0, 0o216, 0o351),
    ("ecircumflex", 0, 0o220, 0o352),
    ("edieresis", 0, 0o221, 0o353),
    ("egrave", 0, 0o217, 0o350),
    ("eight", 0o070, 0o070, 0o070),
    ("ellipsis", 0o274, 0o311, 0o205),
    ("emdash", 0o320, 0o321, 0o227),
    ("endash", 0o261, 0o320, 0o226),
    ("equal", 0o075, 0o075, 0o075),
    ("eth", 0, 0, 0o360),
    ("exclam", 0o041, 0o041, 0o041),
    ("exclamdown", 0o241, 0o301, 0o241),
    ("f", 0o146, 0o146, 0o146),
    ("fi", 0o256, 0o336, 0),
    ("five", 0o065, 0o065, 0o065),
    ("fl", 0o257, 0o337, 0),
    ("florin", 0o246, 0o304, 0o203),
    ("four", 0o064, 0o064, 0o064),
    ("fraction", 0o244, 0o332, 0),
    ("g", 0o147, 0o147, 0o147),
    ("germandbls", 0o373, 0o247, 0o337),
    ("grave", 0o301, 0o140, 0o140),
    ("greater", 0o076, 0o076, 0o076),
    ("guillemotleft", 0o253, 0o307, 0o253),
    ("guillemotright", 0o273, 0o310, 0o273),
    ("guilsinglleft", 0o254, 0o334, 0o213),
    ("guilsinglright", 0o255, 0o335, 0o233),
    ("h", 0o150, 0o150, 0o150),
    ("hungarumlaut", 0o315, 0o375, 0),
    ("hyphen", 0o055, 0o055, 0o055),
    ("i", 0o151, 0o151, 0o151),
    ("iacute", 0, 0o222, 0o355),
    ("icircumflex", 0, 0o224, 0o356),
    ("idieresis", 0, 0o225, 0o357),
    ("igrave", 0, 0o223, 0o354),
    ("j", 0o152, 0o152, 0o152),
    ("k", 0o153, 0o153, 0o153),
    ("l", 0o154, 0o154, 0o154),
    ("less", 0o074, 0o074, 0o074),
    ("logicalnot", 0, 0o302, 0o254),
    ("lslash", 0o370, 0, 0),
    ("m", 0o155, 0o155, 0o155),
    ("macron", 0o305, 0o370, 0o257),
    ("mu", 0, 0o265, 0o265),
    ("multiply", 0, 0, 0o327),
    ("n", 0o156, 0o156, 0o156),
    ("nine", 0o071, 0o071, 0o071),
    ("ntilde", 0, 0o226, 0o361),
    ("numbersign", 0o043, 0o043, 0o043),
    ("o", 0o157, 0o157, 0o157),
    ("oacute", 0, 0o227, 0o363),
    ("ocircumflex", 0, 0o231, 0o364),
    ("odieresis", 0, 0o232, 0o366),
    ("oe", 0o372, 0o317, 0o234),
    ("ogonek", 0o316, 0o376, 0),
    ("ograve", 0, 0o230, 0o362),
    ("one", 0o061, 0o061, 0o061),
    ("onehalf", 0, 0, 0o275),
    ("onequarter", 0, 0, 0o274),
    ("onesuperior", 0, 0, 0o271),
    ("ordfeminine", 0o343, 0o273, 0o252),
    ("ordmasculine", 0o353, 0o274, 0o272),
    ("oslash", 0o371, 0o277, 0o370),
    ("otilde", 0, 0o233, 0o365),
    ("p", 0o160, 0o160, 0o160),
    ("paragraph", 0o266, 0o246, 0o266),
    ("parenleft", 0o050, 0o050, 0o050),
    ("parenright", 0o051, 0o051, 0o051),
    ("percent", 0o045, 0o045, 0o045),
    ("period", 0o056, 0o056, 0o056),
    ("periodcentered", 0o264, 0o341, 0o267),
    ("perthousand", 0o275, 0o344, 0o211),
    ("plus", 0o053, 0o053, 0o053),
    ("plusminus", 0, 0o261, 0o261),
    ("q", 0o161, 0o161, 0o161),
    ("question", 0o077, 0o077, 0o077),
    ("questiondown", 0o277, 0o300, 0o277),
    ("quotedbl", 0o042, 0o042, 0o042),
    ("quotedblbase", 0o271, 0o343, 0o204),
    ("quotedblleft", 0o252, 0o322, 0o223),
    ("quotedblright", 0o272, 0o323, 0o224),
    ("quoteleft", 0o140, 0o324, 0o221),
    ("quoteright", 0o047, 0o325, 0o222),
    ("quotesinglbase", 0o270, 0o342, 0o202),
    ("quotesingle", 0o251, 0o047, 0o047),
    ("r", 0o162, 0o162, 0o162),
    ("registered", 0, 0o250, 0o256),
    ("ring", 0o312, 0o373, 0),
    ("s", 0o163, 0o163, 0o163),
    ("scaron", 0, 0, 0o232),
    ("section", 0o247, 0o244, 0o247),
    ("semicolon", 0o073, 0o073, 0o073),
    ("seven", 0o067, 0o067, 0o067),
    ("six", 0o066, 0o066, 0o066),
    ("slash", 0o057, 0o057, 0o057),
    ("space", 0o040, 0o040, 0o040),
    ("sterling", 0o243, 0o243, 0o243),
    ("t", 0o164, 0o164, 0o164),
    ("thorn", 0, 0, 0o376),
    ("three", 0o063, 0o063, 0o063),
    ("threequarters", 0, 0, 0o276),
    ("threesuperior", 0, 0, 0o263),
    ("tilde", 0o304, 0o367, 0o230),
    ("trademark", 0, 0o252, 0o231),
    ("two", 0o062, 0o062, 0o062),
    ("twosuperior", 0, 0, 0o262),
    ("u", 0o165, 0o165, 0o165),
    ("uacute", 0, 0o234, 0o372),
    ("ucircumflex", 0, 0o236, 0o373),
    ("udieresis", 0, 0o237, 0o374),
    ("ugrave", 0, 0o235, 0o371),
    ("underscore", 0o137, 0o137, 0o137),
    ("v", 0o166, 0o166, 0o166),
    ("w", 0o167, 0o167, 0o167),
    ("x", 0o170, 0o170, 0o170),
    ("y", 0o171, 0o171, 0o171),
    ("yacute", 0, 0, 0o375),
    ("ydieresis", 0, 0o330, 0o377),
    ("yen", 0o245, 0o264, 0o245),
    ("z", 0o172, 0o172, 0o172),
    ("zcaron", 0, 0, 0o236),
    ("zero", 0o060, 0o060, 0o060),
];

// Which column of `LATIN` a table is built from.
#[derive(Clone, Copy)]
enum Column {
    Standard,
    MacRoman,
    WinAnsi,
}

const fn latin(column: Column) -> Table {
    let mut table: Table = [None; 256];
    let mut i = 0;
    while i < LATIN.len() {
        let (name, standard, mac_roman, win_ansi) = LATIN[i];
        let code = match column {
            Column::Standard => standard,
            Column::MacRoman => mac_roman,
            Column::WinAnsi => win_ansi,
        };
        if code != 0 {
            table[code as usize] = Some(name);
        }
        i += 1;
    }
    table
}

const STANDARD: Table = latin(Column::Standard);

// Notes 5 and 6 of Annex D.2: the space is also 0o312 in MacRomanEncoding, and
// 0o240 in WinAnsiEncoding, where the hyphen is also 0o255.
const MAC_ROMAN: Table = {
    let mut table = latin(Column::MacRoman);
    table[0o312] = Some("space");
    table
};

const WIN_ANSI: Table = {
    let mut table = latin(Column::WinAnsi);
    table[0o240] = Some("space");
    table[0o255] = Some("hyphen");
    // Every unused code above 40 octal is the bullet (Annex D.2, note 6), and
    // writers use them: ReportLab writes bullets as 177 octal.
    let mut code = 0o41;
    while code < 256 {
        if table[code].is_none() {
            table[code] = Some("bullet");
        }
        code += 1;
    }
    table
};

const fn from_codes(codes: &[(u8, &'static str)]) -> Table {
    let mut table: Table = [None; 256];
    let mut i = 0;
    while i < codes.len() {
        let (code, name) = codes[i];
        table[code as usize] = Some(name);
        i += 1;
    }
    table
}

// The built-in encoding of the Symbol font (Annex D.5), by code in decimal.
const SYMBOL: Table = from_codes(&[
    (32, "space"),
    (33, "exclam"),
    (34, "universal"),
    (35, "numbersign"),
    (36, "existential"),
    (37, "percent"),
    (38, "ampersand"),
    (39, "suchthat"),
    (40, "parenleft"),
    (41, "parenright"),
    (42, "asteriskmath"),
    (43, "plus"),
    (44, "comma"),
    (45, "minus"),
    (46, "period"),
    (47, "slash"),
    (48, "zero"),
    (49, "one"),
    (50, "two"),
    (51, "three"),
    (52, "four"),
    (53, "five"),
    (54, "six"),
    (55, "seven"),
    (56, "eight"),
    (57, "nine"),
    (58, "colon"),
    (59, "semicolon"),
    (60, "less"),
    (61, "equal"),
    (62, "greater"),
    (63, "question"),
    (64, "congruent"),
    (65, "Alpha"),
    (66, "Beta"),
    (67, "Chi"),
    (68, "Delta"),
    (69, "Epsilon"),
    (70, "Phi"),
    (71, "Gamma"),
    (72, "Eta"),
    (73, "Iota"),
    (74, "theta1"),
    (75, "Kappa"),
    (76, "Lambda"),
    (77, "Mu"),
    (78, "Nu"),
    (79, "Omicron"),
    (80, "Pi"),
    (81, "Theta"),
    (82, "Rho"),
    (83, "Sigma"),
    (84, "Tau"),
    (85, "Upsilon"),
    (86, "sigma1"),
    (87, "Omega"),
    (88, "Xi"),
    (89, "Psi"),
    (90, "Zeta"),
    (91, "bracketleft"),
    (92, "therefore"),
    (93, "bracketright"),
    (94, "perpendicular"),
    (95, "underscore"),
    (96, "radicalex"),
    (97, "alpha"),
    (98, "beta"),
    (99, "chi"),
    (100, "delta"),
    (101, "epsilon"),
    (102, "phi"),
    (103, "gamma"),
    (104, "eta"),
    (105, "iota"),
    (106, "phi1"),
    (107, "kappa"),
    (108, "lambda"),
    (109, "mu"),
    (110, "nu"),
    (111, "omicron"),
    (112, "pi"),
    (113, "theta"),
    (114, "rho"),
    (115, "sigma"),
    (116, "tau"),
    (117, "upsilon"),
    (118, "omega1"),
    (119, "omega"),
    (120, "xi"),
    (121, "psi"),
    (122, "zeta"),
    (123, "braceleft"),
    (124, "bar"),
    (125, "braceright"),
    (126, "similar"),
    (160, "Euro"),
    (161, "Upsilon1"),
    (162, "minute"),
    (163, "lessequal"),
    (164, "fraction"),
    (165, "infinity"),
    (166, "florin"),
    (167, "club"),
    (168, "diamond"),
    (169, "heart"),
    (170, "spade"),
    (171, "arrowboth"),
    (172, "arrowleft"),
    (173, "arrowup"),
    (174, "arrowright"),
    (175, "arrowdown"),
    (176, "degree"),
    (177, "plusminus"),
    (178, "second"),
    (179, "greaterequal"),
    (180, "multiply"),
    (181, "proportional"),
    (182, "partialdiff"),
    (183, "bullet"),
    (184, "divide"),
    (185, "notequal"),
    (186, "equivalence"),
    (187, "approxequal"),
    (188, "ellipsis"),
    (189, "arrowvertex"),
    (190, "arrowhorizex"),
    (191, "carriagereturn"),
    (192, "aleph"),
    (193, "Ifraktur"),
    (194, "Rfraktur"),
    (195, "weierstrass"),
    (196, "circlemultiply"),
    (197, "circleplus"),
    (198, "emptyset"),
    (199, "intersection"),
    (200, "union"),
    (201, "propersuperset"),
    (202, "reflexsuperset"),
    (203, "notsubset"),
    (204, "propersubset"),
    (205, "reflexsubset"),
    (206, "element"),
    (207, "notelement"),
    (208, "angle"),
    (209, "gradient"),
    (210, "registerserif"),
    (211, "copyrightserif"),
    (212, "trademarkserif"),
    (213, "product"),
    (214, "radical"),
    (215, "dotmath"),
    (216, "logicalnot"),
    (217, "logicaland"),
    (218, "logicalor"),
    (219, "arrowdblboth"),
    (220, "arrowdblleft"),
    (221, "arrowdblup"),
    (222, "arrowdblright"),
    (223, "arrowdbldown"),
    (224, "lozenge"),
    (225, "angleleft"),
    (226, "registersans"),
    (227, "copyrightsans"),
    (228, "trademarksans"),
    (229, "summation"),
    (230, "parenlefttp"),
    (231, "parenleftex"),
    (232, "parenleftbt"),
    (233, "bracketlefttp"),
    (234, "bracketleftex"),
    (235, "bracketleftbt"),
    (236, "bracelefttp"),
    (237, "braceleftmid"),
    (238, "braceleftbt"),
    (239, "braceex"),
    (241, "angleright"),
    (242, "integral"),
    (243, "integraltp"),
    (244, "integralex"),
    (245, "integralbt"),
    (246, "parenrighttp"),
    (247, "parenrightex"),
    (248, "parenrightbt"),
    (249, "bracketrighttp"),
    (250, "bracketrightex"),
    (251, "bracketrightbt"),
    (252, "bracerighttp"),
    (253, "bracerightmid"),
    (254, "bracerightbt"),
]);

// The built-in encoding of the ZapfDingbats font (Annex D.6), by code in
// decimal. Its glyph names are `a` followed by a number.
const ZAPF_DINGBATS: Table = from_codes(&[
    (32, "space"),
    (33, "a1"),
    (34, "a2"),
    (35, "a202"),
    (36, "a3"),
    (37, "a4"),
    (38, "a5"),
    (39, "a119"),
    (40, "a118"),
    (41, "a117"),
    (42, "a11"),
    (43, "a12"),
    (44, "a13"),
    (45, "a14"),
    (46, "a15"),
    (47, "a16"),
    (48, "a105"),
    (49, "a17"),
    (50, "a18"),
    (51, "a19"),
    (52, "a20"),
    (53, "a21"),
    (54, "a22"),
    (55, "a23"),
    (56, "a24"),
    (57, "a25"),
    (58, "a26"),
    (59, "a27"),
    (60, "a28"),
    (61, "a6"),
    (62, "a7"),
    (63, "a8"),
    (64, "a9"),
    (65, "a10"),
    (66, "a29"),
    (67, "a30"),
    (68, "a31"),
    (69, "a32"),
    (70, "a33"),
    (71, "a34"),
    (72, "a35"),
    (73, "a36"),
    (74, "a37"),
    (75, "a38"),
    (76, "a39"),
    (77, "a40"),
    (78, "a41"),
    (79, "a42"),
    (80, "a43"),
    (81, "a44"),
    (82, "a45"),
    (83, "a46"),
    (84, "a47"),
    (85, "a48"),
    (86, "a49"),
    (87, "a50"),
    (88, "a51"),
    (89, "a52"),
    (90, "a53"),
    (91, "a54"),
    (92, "a55"),
    (93, "a56"),
    (94, "a57"),
    (95, "a58"),
    (96, "a59"),
    (97, "a60"),
    (98, "a61"),
    (99, "a62"),
    (100, "a63"),
    (101, "a64"),
    (102, "a65"),
    (103, "a66"),
    (104, "a67"),
    (105, "a68"),
    (106, "a69"),
    (107, "a70"),
    (108, "a71"),
    (109, "a72"),
    (110, "a73"),
    (111, "a74"),
    (112, "a203"),
    (113, "a75"),
    (114, "a204"),
    (115, "a76"),
    (116, "a77"),
    (117, "a78"),
    (118, "a79"),
    (119, "a81"),
    (120, "a82"),
    (121, "a83"),
    (122, "a84"),
    (123, "a97"),
    (124, "a98"),
    (125, "a99"),
    (126, "a100"),
    (128, "a89"),
    (129, "a90"),
    (130, "a93"),
    (131, "a94"),
    (132, "a91"),
    (133, "a92"),
    (134, "a205"),
    (135, "a85"),
    (136, "a206"),
    (137, "a86"),
    (138, "a87"),
    (139, "a88"),
    (140, "a95"),
    (141, "a96"),
    (161, "a101"),
    (162, "a102"),
    (163, "a103"),
    (164, "a104"),
    (165, "a106"),
    (166, "a107"),
    (167, "a108"),
    (168, "a112"),
    (169, "a111"),
    (170, "a110"),
    (171, "a109"),
    (172, "a120"),
    (173, "a121"),
    (174, "a122"),
    (175, "a123"),
    (176, "a124"),
    (177, "a125"),
    (178, "a126"),
    (179, "a127"),
    (180, "a128"),
    (181, "a129"),
    (182, "a130"),
    (183, "a131"),
    (184, "a132"),
    (185, "a133"),
    (186, "a134"),
    (187, "a135"),
    (188, "a136"),
    (189, "a137"),
    (190, "a138"),
    (191, "a139"),
    (192, "a140"),
    (193, "a141"),
    (194, "a142"),
    (195, "a143"),
    (196, "a144"),
    (197, "a145"),
    (198, "a146"),
    (199, "a147"),
    (200, "a148"),
    (201, "a149"),
    (202, "a150"),
    (203, "a151"),
    (204, "a152"),
    (205, "a153"),
    (206, "a154"),
    (207, "a155"),
    (208, "a156"),
    (209, "a157"),
    (210, "a158"),
    (211, "a159"),
    (212, "a160"),
    (213, "a161"),
    (214, "a163"),
    (215, "a164"),
    (216, "a196"),
    (217, "a165"),
    (218, "a192"),
    (219, "a166"),
    (220, "a167"),
    (221, "a168"),
    (222, "a169"),
    (223, "a170"),
    (224, "a171"),
    (225, "a172"),
    (226, "a173"),
    (227, "a162"),
    (228, "a174"),
    (229, "a175"),
    (230, "a176"),
    (231, "a177"),
    (232, "a178"),
    (233, "a179"),
    (234, "a193"),
    (235, "a180"),
    (236, "a199"),
    (237, "a181"),
    (238, "a200"),
    (239, "a182"),
    (241, "a201"),
    (242, "a183"),
    (243, "a184"),
    (244, "a197"),
    (245, "a185"),
    (246, "a194"),
    (247, "a198"),
    (248, "a186"),
    (249, "a195"),
    (250, "a187"),
    (251, "a188"),
    (252, "a189"),
    (253, "a190"),
    (254, "a191"),
]);
