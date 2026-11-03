use super::stream::locate_data;
use super::{Object, ObjectId, Parser, Stream, Token, TokenKind};
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limits, Result};

/// An object with a number, so others can refer to it (ISO 32000-2, section
/// 7.3.10). In a file it is written `4 0 obj ... endobj`, and `4 0 R` refers to it.
#[derive(Debug, Clone, PartialEq)]
pub struct IndirectObject {
    pub id: ObjectId,
    pub object: Object,
}

impl IndirectObject {
    pub fn new(id: ObjectId, object: Object) -> Self {
        Self { id, object }
    }
}

/// An indirect object parsed by [`parse_indirect_object`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedIndirectObject {
    pub object: IndirectObject,
    /// Offset just past `endobj`, or past the object when `endobj` is missing.
    pub end: usize,
    /// Problems found and worked around while parsing it.
    pub diagnostics: Diagnostics,
}

/// Parses the indirect object `n g obj ... endobj` that starts at `offset`,
/// including a stream (`<< ... >> stream ... endstream`).
///
/// A missing `endobj` is common in real files, so it is reported as a diagnostic
/// and the object is still returned. A stream whose `/Length` is a reference ends
/// at `endstream`, with an `UnresolvedStreamLength` diagnostic; use
/// [`parse_indirect_object_with_lengths`] to resolve it.
pub fn parse_indirect_object(
    input: &[u8],
    offset: usize,
    limits: Limits,
) -> Result<ParsedIndirectObject> {
    parse_indirect_object_with_lengths(input, offset, limits, &mut |_| None)
}

/// Like [`parse_indirect_object`], but a stream's `/Length` that is a reference is
/// passed to `resolve_length`, which returns its value if it can. When it returns
/// `None` the stream ends at `endstream`, with an `UnresolvedStreamLength`
/// diagnostic.
pub fn parse_indirect_object_with_lengths(
    input: &[u8],
    offset: usize,
    limits: Limits,
    resolve_length: &mut dyn FnMut(ObjectId) -> Option<i64>,
) -> Result<ParsedIndirectObject> {
    let mut parser = Parser::starting_at(input, offset, limits);
    let id = object_header(&mut parser)?;

    let Some(mut object) = parser.next_object()? else {
        return Err(Error::new(ErrorKind::UnexpectedEnd, parser.input_length()));
    };
    let mut object_end = parser.position();

    if let Some(Ok(token)) = parser.peek(0)
        && is_keyword(token, b"stream")
    {
        let keyword_end = token.end;
        let Object::Dictionary(dictionary) = object else {
            return Err(Error::new(ErrorKind::UnexpectedToken, token.start));
        };
        let located = locate_data(
            input,
            &dictionary,
            keyword_end,
            &mut |kind, offset| parser.report(kind, offset),
            resolve_length,
        )?;
        object = Object::Stream(Stream::new(dictionary, located.data));
        object_end = located.end;
        // The parser hasn't read the stream data, so continue after `endstream`
        // with a new one.
        let diagnostics = parser.take_diagnostics();
        parser = Parser::starting_at(input, object_end, limits);
        for diagnostic in diagnostics.iter() {
            parser.report(diagnostic.kind(), diagnostic.offset());
        }
    }

    let end = match parser.peek(0) {
        Some(Ok(token)) if is_keyword(token, b"endobj") => {
            parser.next_token()?;
            parser.position()
        }
        _ => {
            parser.report(DiagnosticKind::MissingEndobj, object_end);
            object_end
        }
    };

    Ok(ParsedIndirectObject {
        object: IndirectObject::new(id, object),
        end,
        diagnostics: parser.take_diagnostics(),
    })
}

// Reads `n g obj`, where n fits in 32 bits and g in 16 bits.
fn object_header(parser: &mut Parser<'_>) -> Result<ObjectId> {
    let number = header_integer(parser)?;
    let generation = header_integer(parser)?;
    let Some(token) = parser.next_token()? else {
        return Err(Error::new(ErrorKind::UnexpectedEnd, parser.input_length()));
    };
    if !is_keyword(&token, b"obj") {
        return Err(Error::new(ErrorKind::InvalidObjectHeader, token.start));
    }

    let invalid = |(_, start)| Error::new(ErrorKind::InvalidObjectHeader, start);
    let number = u32::try_from(number.0).map_err(|_| invalid(number))?;
    let generation = u16::try_from(generation.0).map_err(|_| invalid(generation))?;
    Ok(ObjectId::new(number, generation))
}

fn header_integer(parser: &mut Parser<'_>) -> Result<(i64, usize)> {
    match parser.next_token()? {
        Some(Token {
            kind: TokenKind::Integer(value),
            start,
            ..
        }) => Ok((value, start)),
        Some(token) => Err(Error::new(ErrorKind::InvalidObjectHeader, token.start)),
        None => Err(Error::new(ErrorKind::UnexpectedEnd, parser.input_length())),
    }
}

fn is_keyword(token: &Token, word: &[u8]) -> bool {
    matches!(&token.kind, TokenKind::Keyword(keyword) if keyword == word)
}
