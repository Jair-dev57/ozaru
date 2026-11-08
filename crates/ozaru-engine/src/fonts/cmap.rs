use crate::base::Limits;
use crate::syntax::{Lexer, Name, TokenKind};

/// A character code read from a CMap: its value and how many bytes it takes,
/// from 1 to 4. `<41>` and `<0041>` have the same value but different lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code {
    pub value: u32,
    pub length: u8,
}

/// Where a CMap maps a code: to a string, usually UTF-16BE text in a ToUnicode
/// CMap, or to a glyph name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    String(Vec<u8>),
    Name(Name),
}

/// The destination of a `bfrange`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeDestination {
    /// The first code maps to this string, and each next code to the string with
    /// its last byte one higher.
    Start(Vec<u8>),
    /// One destination for each code of the range, in order. It can be shorter
    /// than the range.
    List(Vec<Destination>),
}

/// The mappings of a CMap file, in the order they appear (Adobe Technical Note
/// #5014, and ISO 32000-2, section 9.10.3 for ToUnicode CMaps).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CMapData {
    /// The `codespacerange` entries: the codes that the CMap reads, and how long
    /// they are.
    pub codespace: Vec<(Code, Code)>,
    /// The `bfchar` entries.
    pub chars: Vec<(Code, Destination)>,
    /// The `bfrange` entries.
    pub ranges: Vec<(Code, Code, RangeDestination)>,
    /// Whether some entries or some syntax couldn't be read and were skipped.
    pub damaged: bool,
}

/// Reads the mappings of the CMap in `data`.
///
/// CMaps are small PostScript programs, but only their mapping sections matter
/// here: `begincodespacerange`, `beginbfchar` and `beginbfrange`. Everything
/// else is skipped. Entries that can't be read are skipped too, and if the
/// syntax is broken, reading stops there; both set `damaged`.
pub fn parse_cmap(data: &[u8], limits: Limits) -> CMapData {
    let mut cmap = CMapData::default();
    let mut lexer = Lexer::new(data, limits);
    let mut operands: Vec<Operand> = Vec::new();
    let mut section: Option<Section> = None;

    loop {
        let token = match lexer.next_token() {
            Ok(Some(token)) => token,
            Ok(None) => break,
            Err(_) => {
                cmap.damaged = true;
                break;
            }
        };
        let operand = match token.kind {
            TokenKind::Keyword(keyword) => {
                match keyword {
                    b"begincodespacerange" => section = Some(Section::Codespace),
                    b"beginbfchar" => section = Some(Section::Chars),
                    b"beginbfrange" => section = Some(Section::Ranges),
                    b"endcodespacerange" | b"endbfchar" | b"endbfrange" => {
                        if let Some(section) = section.take() {
                            cmap.damaged |= section.add(&operands, &mut cmap);
                        }
                    }
                    _ => {}
                }
                operands.clear();
                continue;
            }
            TokenKind::String(string) => Operand::String(string.into_bytes()),
            TokenKind::Name(name) => Operand::Name(name),
            TokenKind::ArrayStart if section.is_some() => match read_array(&mut lexer, limits) {
                Some(items) => Operand::Array(items),
                None => {
                    cmap.damaged = true;
                    break;
                }
            },
            _ => Operand::Other,
        };
        if section.is_some() {
            operands.push(operand);
        }
    }
    // A section that never ends still counts.
    if let Some(section) = section {
        cmap.damaged |= section.add(&operands, &mut cmap);
    }
    cmap
}

#[derive(Debug, Clone, Copy)]
enum Section {
    Codespace,
    Chars,
    Ranges,
}

#[derive(Debug, Clone)]
enum Operand {
    String(Vec<u8>),
    Name(Name),
    Array(Vec<Destination>),
    Other,
}

impl Section {
    // Adds the entries in `operands` to `cmap`. True if some couldn't be read.
    fn add(self, operands: &[Operand], cmap: &mut CMapData) -> bool {
        let size = match self {
            Self::Codespace | Self::Chars => 2,
            Self::Ranges => 3,
        };
        let mut damaged = !operands.len().is_multiple_of(size);
        for entry in operands.chunks_exact(size) {
            let added = match (self, entry) {
                (Self::Codespace, [Operand::String(low), Operand::String(high)]) => {
                    range(low, high).map(|(low, high)| cmap.codespace.push((low, high)))
                }
                (Self::Chars, [Operand::String(source), destination]) => code(source)
                    .zip(self::destination(destination))
                    .map(|entry| cmap.chars.push(entry)),
                (Self::Ranges, [Operand::String(low), Operand::String(high), destination]) => {
                    let destination = match destination {
                        Operand::String(start) => Some(RangeDestination::Start(start.clone())),
                        Operand::Array(list) => Some(RangeDestination::List(list.clone())),
                        _ => None,
                    };
                    range(low, high)
                        .zip(destination)
                        .map(|((low, high), destination)| {
                            cmap.ranges.push((low, high, destination))
                        })
                }
                _ => None,
            };
            damaged |= added.is_none();
        }
        damaged
    }
}

// A code of 1 to 4 bytes.
fn code(bytes: &[u8]) -> Option<Code> {
    if bytes.is_empty() || bytes.len() > 4 {
        return None;
    }
    let value = bytes
        .iter()
        .fold(0, |value, &byte| value << 8 | u32::from(byte));
    Some(Code {
        value,
        length: bytes.len() as u8,
    })
}

// Two codes of the same length, the first not above the second.
fn range(low: &[u8], high: &[u8]) -> Option<(Code, Code)> {
    let (low, high) = (code(low)?, code(high)?);
    (low.length == high.length && low.value <= high.value).then_some((low, high))
}

fn destination(operand: &Operand) -> Option<Destination> {
    match operand {
        Operand::String(bytes) => Some(Destination::String(bytes.clone())),
        Operand::Name(name) => Some(Destination::Name(name.clone())),
        _ => None,
    }
}

// The items of an array after its `[`, up to its `]`: strings and names. Other
// items are skipped. None if the syntax is broken or the array never ends.
fn read_array(lexer: &mut Lexer<'_>, limits: Limits) -> Option<Vec<Destination>> {
    let mut items = Vec::new();
    loop {
        let token = lexer.next_token().ok()??;
        match token.kind {
            TokenKind::ArrayEnd => return Some(items),
            TokenKind::String(string) => items.push(Destination::String(string.into_bytes())),
            TokenKind::Name(name) => items.push(Destination::Name(name)),
            _ => {}
        }
        if items.len() > limits.max_array_length {
            return None;
        }
    }
}
