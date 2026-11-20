use std::io::Write as _;

use crate::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};

/// Appends `object` to `out` as PDF syntax (ISO 32000-2, section 7.3), so that
/// parsing it gives back the same object.
///
/// - **Reals** are written in the shortest form that reads back as the same
///   number, never with an exponent, which PDF doesn't allow, and always with a
///   period, so they stay reals. Infinities and NaN, which PDF can't hold, are
///   written as `0.0`.
/// - **Strings** keep the form they were read in. Literal strings escape `(`,
///   `)` and `\`, and carriage returns, which a reader would turn into line
///   feeds.
/// - **Names** escape, as `#xx`, the bytes that aren't printable ASCII,
///   delimiters, whitespace and `#` itself.
/// - **Streams** are written as their dictionary only: their data isn't in the
///   object. [`write_indirect`] writes a stream with its data.
pub fn write_object(out: &mut Vec<u8>, object: &Object) {
    match object {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(true) => out.extend_from_slice(b"true"),
        Object::Boolean(false) => out.extend_from_slice(b"false"),
        Object::Integer(value) => {
            let _ = write!(out, "{value}");
        }
        Object::Real(value) => write_real(out, *value),
        Object::String(string) => write_string(out, string),
        Object::Name(name) => write_name(out, name),
        Object::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b' ');
                }
                write_object(out, item);
            }
            out.push(b']');
        }
        Object::Dictionary(dictionary) => write_dictionary(out, dictionary),
        Object::Stream(stream) => write_dictionary(out, stream.dictionary()),
        Object::Reference(id) => {
            let _ = write!(out, "{} {} R", id.number, id.generation);
        }
    }
}

/// Appends a dictionary: `<</Key value /Key value>>`.
pub fn write_dictionary(out: &mut Vec<u8>, dictionary: &Dictionary) {
    out.extend_from_slice(b"<<");
    for (index, (key, value)) in dictionary.iter().enumerate() {
        if index > 0 {
            out.push(b' ');
        }
        write_name(out, key);
        out.push(b' ');
        write_object(out, value);
    }
    out.extend_from_slice(b">>");
}

/// Appends the indirect object `id`: `n g obj … endobj`, and returns the offset
/// it starts at in `out`.
///
/// When `data` is given, the object is written as a stream with that data, which
/// is written as it is, already encoded by the stream's filters. Its dictionary,
/// that of `object` if it is a dictionary or a stream, gets the right `/Length`.
pub fn write_indirect(
    out: &mut Vec<u8>,
    id: ObjectId,
    object: &Object,
    data: Option<&[u8]>,
) -> usize {
    let start = out.len();
    let _ = writeln!(out, "{} {} obj", id.number, id.generation);
    match data {
        Some(data) => {
            let mut dictionary = match object {
                Object::Dictionary(dictionary) => dictionary.clone(),
                Object::Stream(stream) => stream.dictionary().clone(),
                _ => Dictionary::new(),
            };
            let length = i64::try_from(data.len()).unwrap_or(i64::MAX);
            dictionary.insert(Name::from("Length"), Object::Integer(length));
            write_dictionary(out, &dictionary);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(data);
            out.extend_from_slice(b"\nendstream");
        }
        None => write_object(out, object),
    }
    out.extend_from_slice(b"\nendobj\n");
    start
}

fn write_real(out: &mut Vec<u8>, value: f64) {
    if !value.is_finite() || value == 0.0 {
        out.extend_from_slice(b"0.0");
        return;
    }
    // Rust writes floats in their shortest exact form, without an exponent.
    let start = out.len();
    let _ = write!(out, "{value}");
    if !out[start..].contains(&b'.') {
        out.extend_from_slice(b".0");
    }
}

fn write_string(out: &mut Vec<u8>, string: &PdfString) {
    let bytes = string.as_bytes();
    match string.format() {
        StringFormat::Hexadecimal => {
            const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
            out.reserve(bytes.len() * 2 + 2);
            out.push(b'<');
            for &byte in bytes {
                out.push(DIGITS[usize::from(byte >> 4)]);
                out.push(DIGITS[usize::from(byte & 0x0F)]);
            }
            out.push(b'>');
        }
        StringFormat::Literal => {
            out.reserve(bytes.len() + 2);
            out.push(b'(');
            for &byte in bytes {
                match byte {
                    b'(' | b')' | b'\\' => out.extend_from_slice(&[b'\\', byte]),
                    b'\r' => out.extend_from_slice(b"\\r"),
                    _ => out.push(byte),
                }
            }
            out.push(b')');
        }
    }
}

fn write_name(out: &mut Vec<u8>, name: &Name) {
    // `Name`'s `Display` writes it as a PDF name, escapes included.
    let _ = write!(out, "{name}");
}
