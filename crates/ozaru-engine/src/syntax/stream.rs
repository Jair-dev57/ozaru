use std::ops::Range;

use super::chars::is_whitespace;
use super::{Dictionary, Object};
use crate::base::{DiagnosticKind, Error, ErrorKind, Result};

/// A PDF stream: a dictionary followed by a sequence of bytes, such as the content
/// of a page or an image (ISO 32000-2, section 7.3.8).
///
/// It doesn't copy the bytes: it records where they are in the file, so large
/// streams don't take memory twice. They are decoded only when needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    dictionary: Dictionary,
    data: Range<usize>,
}

impl Stream {
    /// `data` is the range of the stream's raw bytes in the file.
    pub fn new(dictionary: Dictionary, data: Range<usize>) -> Self {
        Self { dictionary, data }
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Where the raw, undecoded bytes are in the file.
    pub fn data_range(&self) -> Range<usize> {
        self.data.clone()
    }

    /// The raw, undecoded bytes, taken from `file`. `None` if the range doesn't fit
    /// in `file`, for example when given a different file.
    pub fn raw_data<'a>(&self, file: &'a [u8]) -> Option<&'a [u8]> {
        file.get(self.data.clone())
    }
}

// Where a stream's data is, and where its `endstream` keyword ends.
pub(super) struct LocatedData {
    pub data: Range<usize>,
    pub end: usize,
}

const ENDSTREAM: &[u8] = b"endstream";

/// Finds the data of the stream whose `stream` keyword ends at `keyword_end`
/// (ISO 32000-2, section 7.3.8).
///
/// `/Length` is used when it is a direct integer and `endstream` follows the data.
/// Otherwise the data ends at the next `endstream`, with a diagnostic: wrong
/// lengths are common in real files. The bytes of the data are never parsed, since
/// they can be binary.
pub(super) fn locate_data(
    input: &[u8],
    dictionary: &Dictionary,
    keyword_end: usize,
    report: &mut dyn FnMut(DiagnosticKind, usize),
) -> Result<LocatedData> {
    let start = data_start(input, keyword_end, report);

    let declared = match dictionary.get("Length") {
        Some(Object::Integer(length)) => Some(*length),
        // Resolving it needs the cross-reference table, which comes in phase 3.
        Some(Object::Reference(_)) => {
            report(DiagnosticKind::UnresolvedStreamLength, start);
            None
        }
        Some(_) => {
            report(DiagnosticKind::WrongStreamLength, start);
            None
        }
        None => {
            report(DiagnosticKind::MissingStreamLength, start);
            None
        }
    };

    if let Some(length) = declared {
        let data_end = usize::try_from(length)
            .ok()
            .and_then(|length| start.checked_add(length))
            .filter(|&end| end <= input.len());
        if let Some(data_end) = data_end
            && let Some(end) = endstream_after(input, data_end)
        {
            return Ok(LocatedData {
                data: start..data_end,
                end,
            });
        }
        report(DiagnosticKind::WrongStreamLength, start);
    }

    let Some(found) = find(input, start, ENDSTREAM) else {
        return Err(Error::new(ErrorKind::MissingEndstream, start));
    };
    Ok(LocatedData {
        data: start..without_trailing_end_of_line(input, start, found),
        end: found + ENDSTREAM.len(),
    })
}

// The data starts after the end of line that follows `stream`. The standard
// requires CR LF or LF; a lone CR is accepted with a diagnostic.
fn data_start(
    input: &[u8],
    keyword_end: usize,
    report: &mut dyn FnMut(DiagnosticKind, usize),
) -> usize {
    match (input.get(keyword_end), input.get(keyword_end + 1)) {
        (Some(b'\r'), Some(b'\n')) => keyword_end + 2,
        (Some(b'\n'), _) => keyword_end + 1,
        (Some(b'\r'), _) => {
            report(DiagnosticKind::StreamLineEndIsCarriageReturn, keyword_end);
            keyword_end + 1
        }
        _ => {
            report(DiagnosticKind::MissingStreamLineEnd, keyword_end);
            keyword_end
        }
    }
}

// Returns where `endstream` ends, if it follows `position` after optional
// whitespace.
fn endstream_after(input: &[u8], mut position: usize) -> Option<usize> {
    while input.get(position).copied().is_some_and(is_whitespace) {
        position += 1;
    }
    input[position..]
        .starts_with(ENDSTREAM)
        .then_some(position + ENDSTREAM.len())
}

fn find(input: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    input
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}

// The end of line before `endstream` isn't part of the data.
fn without_trailing_end_of_line(input: &[u8], start: usize, mut end: usize) -> usize {
    if end > start && input[end - 1] == b'\n' {
        end -= 1;
    }
    if end > start && input[end - 1] == b'\r' {
        end -= 1;
    }
    end
}
