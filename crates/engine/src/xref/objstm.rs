use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limits, Result};
use crate::filters::decode_data;
use crate::syntax::{IndirectObject, Lexer, Object, ObjectId, Parser, Stream, TokenKind};

/// An object stream, already decoded (ISO 32000-2, section 7.5.7): a stream of
/// `/Type /ObjStm` that packs `/N` objects together, so they can be compressed.
///
/// Its data starts with `/N` pairs of integers, the number of each object and its
/// offset after `/First`, followed by the objects themselves:
///
/// ```text
/// 11 0 12 25 13 51
/// << /Type /Page /Parent 2 0 R >>
/// ...
/// ```
///
/// Objects inside it can't be streams, and their generation is always 0.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectStream {
    // Object number and offset after `first` of each object, in order.
    objects: Vec<(u32, usize)>,
    data: Vec<u8>,
    first: usize,
    // Where the stream data starts in the file, for errors and diagnostics.
    offset: usize,
    limits: Limits,
}

/// An object stream parsed by [`parse_object_stream`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedObjectStream {
    pub stream: ObjectStream,
    /// Problems found and worked around while decoding and reading its header.
    pub diagnostics: Diagnostics,
}

/// An object read from an object stream by [`ObjectStream::object`].
#[derive(Debug, Clone, PartialEq)]
pub struct CompressedObject {
    pub object: IndirectObject,
    /// Problems found and worked around while parsing it.
    pub diagnostics: Diagnostics,
}

/// Decodes the object stream `stream`, found in `file`, and reads its header.
///
/// Offsets inside the decoded data don't exist in the file, so errors and
/// diagnostics about them are reported where the stream data starts.
pub fn parse_object_stream(
    stream: &Stream,
    file: &[u8],
    limits: Limits,
) -> Result<ParsedObjectStream> {
    let raw = stream
        .raw_data(file)
        .ok_or_else(|| Error::new(ErrorKind::UnexpectedEnd, file.len()))?;
    parse_object_stream_data(stream, raw, limits)
}

/// Like [`parse_object_stream`], for raw `data` that isn't in the file as it is,
/// such as the decrypted data of an encrypted file. It is still decoded with the
/// stream's filters.
pub fn parse_object_stream_data(
    stream: &Stream,
    data: &[u8],
    limits: Limits,
) -> Result<ParsedObjectStream> {
    let offset = stream.data_range().start;
    let dictionary = stream.dictionary();
    let kind = dictionary.get("Type").and_then(Object::as_name);
    if kind.is_none_or(|kind| kind.as_bytes() != b"ObjStm") {
        return Err(Error::new(ErrorKind::NotAnObjectStream, offset));
    }
    let invalid = || Error::new(ErrorKind::InvalidObjectStream, offset);
    let count = non_negative(dictionary.get("N")).ok_or_else(invalid)?;
    let first = non_negative(dictionary.get("First")).ok_or_else(invalid)?;

    let decoded = decode_data(dictionary, data, offset, limits)?;
    let mut diagnostics = decoded.diagnostics;
    let data = decoded.data;
    if first > data.len() {
        return Err(invalid());
    }

    // The header ends at `/First`, so a short header can't read the objects.
    let mut lexer = Lexer::new(&data[..first], limits);
    let mut integer = || match lexer.next_token() {
        Ok(Some(token)) => match token.kind {
            TokenKind::Integer(value) => Ok(Some(value)),
            _ => Err(invalid()),
        },
        Ok(None) => Ok(None),
        Err(_) => Err(invalid()),
    };
    let mut objects = Vec::new();
    while objects.len() < count {
        let (Some(number), Some(position)) = (integer()?, integer()?) else {
            diagnostics.push(DiagnosticKind::ObjectStreamHeaderTooShort, offset);
            break;
        };
        let number = u32::try_from(number).map_err(|_| invalid())?;
        let position = usize::try_from(position).map_err(|_| invalid())?;
        objects.push((number, position));
    }

    Ok(ParsedObjectStream {
        stream: ObjectStream {
            objects,
            data,
            first,
            offset,
            limits,
        },
        diagnostics,
    })
}

impl ObjectStream {
    /// Number of objects listed in the header.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The object number at `index`, as the header lists it.
    pub fn number(&self, index: u32) -> Option<u32> {
        let index = usize::try_from(index).ok()?;
        self.objects.get(index).map(|&(number, _)| number)
    }

    /// The index of object `number`, for when the cross-reference entry gives the
    /// wrong one.
    pub fn index_of(&self, number: u32) -> Option<u32> {
        let index = self.objects.iter().position(|&(n, _)| n == number)?;
        u32::try_from(index).ok()
    }

    /// Parses the object at `index`. It is an [`ErrorKind::InvalidObjectStream`]
    /// error if there's no such index or its offset is past the data.
    pub fn object(&self, index: u32) -> Result<CompressedObject> {
        let invalid = || Error::new(ErrorKind::InvalidObjectStream, self.offset);
        let &(number, position) = usize::try_from(index)
            .ok()
            .and_then(|index| self.objects.get(index))
            .ok_or_else(invalid)?;
        let start = self
            .first
            .checked_add(position)
            .filter(|&start| start <= self.data.len())
            .ok_or_else(invalid)?;

        let mut parser = Parser::starting_at(&self.data, start, self.limits);
        let object = match parser.next_object() {
            Ok(Some(object)) => object,
            Ok(None) => return Err(Error::new(ErrorKind::UnexpectedEnd, self.offset)),
            Err(error) => return Err(Error::new(error.kind(), self.offset)),
        };
        let mut diagnostics = Diagnostics::new(self.limits.max_diagnostics);
        for diagnostic in parser.diagnostics().iter() {
            diagnostics.push(diagnostic.kind(), self.offset);
        }
        Ok(CompressedObject {
            object: IndirectObject::new(ObjectId::new(number, 0), object),
            diagnostics,
        })
    }
}

fn non_negative(value: Option<&Object>) -> Option<usize> {
    usize::try_from(value?.as_integer()?).ok()
}
