use super::skip_whitespace;
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limits, Result};
use crate::syntax::chars::is_regular;
use crate::syntax::{Lexer, Token, TokenKind};

/// How many bytes from the end of the file are searched for `startxref`. Writers
/// put it in the last few lines, but some files have junk after `%%EOF`.
const SEARCH_WINDOW: usize = 1024;

const KEYWORD: &[u8] = b"startxref";

/// The `startxref` found by [`find_startxref`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startxref {
    /// Offset of the last cross-reference section, a table or a stream.
    pub offset: usize,
    /// Problems found and worked around while reading it.
    pub diagnostics: Diagnostics,
}

/// Finds the offset of the last cross-reference section (ISO 32000-2, section
/// 7.5.5). A file ends with:
///
/// ```text
/// startxref
/// 1234
/// %%EOF
/// ```
///
/// Only the last `startxref` near the end of the file counts, because each
/// incremental update appends a new one.
pub fn find_startxref(input: &[u8], limits: Limits) -> Result<Startxref> {
    let window = input.len().saturating_sub(SEARCH_WINDOW);
    let last = input.len().saturating_sub(KEYWORD.len());
    let keyword = (window..=last)
        .rev()
        .find(|&position| is_keyword_at(input, position))
        .ok_or_else(|| Error::new(ErrorKind::MissingStartxref, input.len()))?;
    let keyword_end = keyword + KEYWORD.len();

    let invalid = |offset| Error::new(ErrorKind::InvalidStartxref, offset);
    let (offset, end) = match Lexer::starting_at(input, keyword_end, limits).next_token() {
        Ok(Some(Token {
            kind: TokenKind::Integer(value),
            start,
            end,
        })) => match usize::try_from(value) {
            Ok(offset) if offset < input.len() => (offset, end),
            _ => return Err(invalid(start)),
        },
        Ok(Some(token)) => return Err(invalid(token.start)),
        Ok(None) | Err(_) => return Err(invalid(keyword_end)),
    };

    let mut diagnostics = Diagnostics::new(limits.max_diagnostics);
    let marker = skip_whitespace(input, end);
    if !input[marker..].starts_with(b"%%EOF") {
        diagnostics.push(DiagnosticKind::MissingEndOfFileMarker, marker);
    }
    Ok(Startxref {
        offset,
        diagnostics,
    })
}

// `startxref` is at `position` and isn't part of a longer word, such as
// `xstartxref`.
fn is_keyword_at(input: &[u8], position: usize) -> bool {
    let end = position + KEYWORD.len();
    input[position..].starts_with(KEYWORD)
        && (position == 0 || !is_regular(input[position - 1]))
        && input.get(end).is_none_or(|&byte| !is_regular(byte))
}
