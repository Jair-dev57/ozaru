/// Decodes a text string, the kind of string used for text outside of page
/// content, such as the title of a document (ISO 32000-2, section 7.9.2.2).
///
/// - **UTF-16BE**, after the byte order mark `FE FF`. The escape sequences that
///   mark a language, `1B ... 1B`, are removed. A lone surrogate or an odd last
///   byte becomes U+FFFD.
/// - **UTF-16LE**, after `FF FE`. The standard doesn't allow it, but some
///   writers use it.
/// - **UTF-8**, after `EF BB BF`, as PDF 2.0 allows. Invalid sequences become
///   U+FFFD.
/// - **PDFDocEncoding** otherwise, a single-byte encoding close to Latin-1
///   (Annex D.2). Its undefined codes 0x7F and 0x9F become U+FFFD, and 0xAD is
///   the soft hyphen, as in Latin-1.
pub fn decode_text_string(bytes: &[u8]) -> String {
    match bytes {
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, u16::from_le_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => bytes.iter().map(|&byte| pdf_doc_char(byte)).collect(),
    }
}

fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let (pairs, odd_byte) = bytes.as_chunks::<2>();
    let units = pairs.iter().map(|&pair| unit(pair));
    let mut text: String = char::decode_utf16(without_language_escapes(units))
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if !odd_byte.is_empty() {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    text
}

// The code units outside of the language escapes `1B ... 1B` (ISO 32000-2,
// section 7.9.2.2.1).
fn without_language_escapes(units: impl Iterator<Item = u16>) -> impl Iterator<Item = u16> {
    let mut in_escape = false;
    units.filter(move |&unit| {
        if unit == 0x1B {
            in_escape = !in_escape;
            return false;
        }
        !in_escape
    })
}

// PDFDocEncoding from 0x18 to 0x1F: spacing diacritics.
const DIACRITICS: [char; 8] = ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'];

// PDFDocEncoding from 0x80 to 0xA0. 0x9F is undefined.
const HIGH: [char; 33] = [
    '•', '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚', '™',
    'ﬁ', 'ﬂ', 'Ł', 'Œ', 'Š', 'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '\u{FFFD}', '€',
];

fn pdf_doc_char(byte: u8) -> char {
    match byte {
        0x18..=0x1F => DIACRITICS[usize::from(byte - 0x18)],
        0x7F => char::REPLACEMENT_CHARACTER,
        0x80..=0xA0 => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}
