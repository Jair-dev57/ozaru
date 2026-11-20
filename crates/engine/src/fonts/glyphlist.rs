use std::sync::OnceLock;

/// Which list of glyph names maps names to Unicode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphList {
    /// The Adobe Glyph List, for every font but ZapfDingbats.
    Adobe,
    /// The ITC Zapf Dingbats Glyph List, whose names are `a1`, `a2` and so on,
    /// for the ZapfDingbats font. Names that it doesn't have, such as `space`,
    /// are looked up in the Adobe Glyph List.
    ZapfDingbats,
}

/// The Unicode text of the glyph name `name`, following the Adobe Glyph List
/// Specification (section 2, "Mapping glyph names to Unicode"):
///
/// 1. Whatever follows the first period is dropped: `a.sc` is `a`.
/// 2. The rest is split at underscores into components: `f_f_i` is `f`, `f` and
///    `i`.
/// 3. Each component maps through `list` (for ZapfDingbats, its own list and
///    then the Adobe one), or else is `uni` followed by groups of
///    four hexadecimal digits, or `u` followed by four to six, giving those code
///    points. Other components give nothing.
///
/// The specification only allows uppercase hexadecimal digits, but some writers
/// use lowercase ones, so both are accepted. `None` if no component gives
/// anything.
pub fn glyph_to_unicode(name: &[u8], list: GlyphList) -> Option<String> {
    let name = match name.iter().position(|&byte| byte == b'.') {
        Some(period) => &name[..period],
        None => name,
    };
    let mut text = String::new();
    for component in name.split(|&byte| byte == b'_') {
        component_to_unicode(component, list, &mut text);
    }
    (!text.is_empty()).then_some(text)
}

fn component_to_unicode(component: &[u8], list: GlyphList, text: &mut String) {
    let Ok(component) = std::str::from_utf8(component) else {
        return;
    };
    let mapped = match list {
        GlyphList::ZapfDingbats => lookup(component, GlyphList::ZapfDingbats)
            .or_else(|| lookup(component, GlyphList::Adobe)),
        GlyphList::Adobe => lookup(component, GlyphList::Adobe),
    };
    if let Some(mapped) = mapped {
        text.push_str(mapped);
    } else if let Some(digits) = component.strip_prefix("uni") {
        if !digits.is_empty() && digits.len().is_multiple_of(4) {
            let chars: Option<Vec<char>> = digits
                .as_bytes()
                .chunks(4)
                .map(|group| scalar(group, 0xFFFF))
                .collect();
            text.extend(chars.into_iter().flatten());
        }
    } else if let Some(digits) = component.strip_prefix('u')
        && (4..=6).contains(&digits.len())
        && let Some(c) = scalar(digits.as_bytes(), 0x10FFFF)
    {
        text.push(c);
    }
}

// Hexadecimal digits as a Unicode scalar value up to `max`. Surrogates aren't.
fn scalar(digits: &[u8], max: u32) -> Option<char> {
    if !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let value = u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?;
    if value > max {
        return None;
    }
    char::from_u32(value)
}

// The text of `name` in `list`, if it has it.
fn lookup(name: &str, list: GlyphList) -> Option<&'static str> {
    let entries = match list {
        GlyphList::Adobe => {
            static ADOBE: OnceLock<Vec<(&str, String)>> = OnceLock::new();
            ADOBE.get_or_init(|| parse(include_str!("data/glyphlist.txt")))
        }
        GlyphList::ZapfDingbats => {
            static ZAPF_DINGBATS: OnceLock<Vec<(&str, String)>> = OnceLock::new();
            ZAPF_DINGBATS.get_or_init(|| parse(include_str!("data/zapfdingbats.txt")))
        }
    };
    let index = entries
        .binary_search_by(|(entry, _)| (*entry).cmp(name))
        .ok()?;
    Some(entries[index].1.as_str())
}

// The lists are lines of a name and its code points in hexadecimal, separated
// by a semicolon: `Aacute;00C1`, or `dalethatafpatah;05D3 05B2` for a name with
// several. Lines that start with `#` are comments.
fn parse(list: &'static str) -> Vec<(&'static str, String)> {
    let mut entries: Vec<(&str, String)> = list
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (name, codes) = line.split_once(';')?;
            let text = codes
                .split(' ')
                .map(|code| scalar(code.as_bytes(), 0x10FFFF))
                .collect::<Option<String>>()?;
            Some((name, text))
        })
        .collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    entries
}
