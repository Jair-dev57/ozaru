use super::chars::{hex_value, is_regular, is_whitespace};
use super::{Name, PdfString, StringFormat, Token, TokenKind};
use crate::base::{Error, ErrorKind, Limit, Limits, Result};

/// Splits PDF bytes into tokens (ISO 32000-2, sections 7.2 and 7.3).
///
/// Whitespace and comments are skipped. After an error the lexer shouldn't be used
/// again.
pub struct Lexer<'a> {
    input: &'a [u8],
    position: usize,
    limits: Limits,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8], limits: Limits) -> Self {
        Self::starting_at(input, 0, limits)
    }

    /// A lexer that starts reading at `position`. Offsets in tokens and errors are
    /// still counted from the start of `input`.
    pub fn starting_at(input: &'a [u8], position: usize, limits: Limits) -> Self {
        Self {
            input,
            position,
            limits,
        }
    }

    /// Offset of the next byte the lexer will read.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Returns the next token, or `None` at the end of the input.
    pub fn next_token(&mut self) -> Result<Option<Token<'a>>> {
        self.skip_whitespace_and_comments();
        let start = self.position;
        let Some(byte) = self.peek() else {
            return Ok(None);
        };

        let kind = match byte {
            b'(' => self.literal_string()?,
            b'<' if self.peek_at(1) == Some(b'<') => self.symbol(2, TokenKind::DictionaryStart),
            b'<' => self.hexadecimal_string()?,
            b'>' if self.peek_at(1) == Some(b'>') => self.symbol(2, TokenKind::DictionaryEnd),
            b'[' => self.symbol(1, TokenKind::ArrayStart),
            b']' => self.symbol(1, TokenKind::ArrayEnd),
            b'{' => self.symbol(1, TokenKind::ProcedureStart),
            b'}' => self.symbol(1, TokenKind::ProcedureEnd),
            b'/' => self.name()?,
            b')' | b'>' => return Err(Error::new(ErrorKind::UnexpectedByte(byte), start)),
            _ => self.number_or_keyword()?,
        };

        Ok(Some(Token {
            kind,
            start,
            end: self.position,
        }))
    }

    fn peek(&self) -> Option<u8> {
        self.peek_at(0)
    }

    fn peek_at(&self, distance: usize) -> Option<u8> {
        self.input.get(self.position + distance).copied()
    }

    fn symbol(&mut self, length: usize, kind: TokenKind<'a>) -> TokenKind<'a> {
        self.position += length;
        kind
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) {
                self.position += 1;
            } else if byte == b'%' {
                while self.peek().is_some_and(|b| b != b'\r' && b != b'\n') {
                    self.position += 1;
                }
            } else {
                break;
            }
        }
    }

    // Moves past an end-of-line marker (CR, LF or CR LF), if there is one here.
    fn skip_end_of_line(&mut self) -> bool {
        match self.peek() {
            Some(b'\r') => {
                self.position += 1;
                if self.peek() == Some(b'\n') {
                    self.position += 1;
                }
                true
            }
            Some(b'\n') => {
                self.position += 1;
                true
            }
            _ => false,
        }
    }

    fn take_regular_run(&mut self) -> &'a [u8] {
        let start = self.position;
        while self.peek().is_some_and(is_regular) {
            self.position += 1;
        }
        &self.input[start..self.position]
    }

    fn check_length(&self, length: usize, limit: Limit, start: usize) -> Result<()> {
        let maximum = match limit {
            Limit::StringLength => self.limits.max_string_length,
            Limit::NameLength => self.limits.max_name_length,
            _ => usize::MAX,
        };
        if length > maximum {
            return Err(Error::new(ErrorKind::LimitExceeded(limit), start));
        }
        Ok(())
    }

    // (text) with nested parentheses and escapes (section 7.3.4.2).
    fn literal_string(&mut self) -> Result<TokenKind<'a>> {
        let start = self.position;
        self.position += 1;
        let mut bytes = Vec::new();
        let mut depth = 1;

        loop {
            let Some(byte) = self.peek() else {
                return Err(Error::new(ErrorKind::UnexpectedEnd, self.position));
            };
            match byte {
                b'\\' => {
                    self.position += 1;
                    self.escape_sequence(&mut bytes)?;
                }
                b'\r' | b'\n' => {
                    // An end of line inside a string is read as a single line feed.
                    self.skip_end_of_line();
                    bytes.push(b'\n');
                }
                _ => {
                    self.position += 1;
                    if byte == b'(' {
                        depth += 1;
                    } else if byte == b')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    bytes.push(byte);
                }
            }
            self.check_length(bytes.len(), Limit::StringLength, start)?;
        }

        Ok(TokenKind::String(PdfString::new(
            bytes,
            StringFormat::Literal,
        )))
    }

    fn escape_sequence(&mut self, bytes: &mut Vec<u8>) -> Result<()> {
        let Some(byte) = self.peek() else {
            return Err(Error::new(ErrorKind::UnexpectedEnd, self.position));
        };
        if self.skip_end_of_line() {
            // A backslash before an end of line joins the two lines.
            return Ok(());
        }
        self.position += 1;
        match byte {
            b'n' => bytes.push(b'\n'),
            b'r' => bytes.push(b'\r'),
            b't' => bytes.push(b'\t'),
            b'b' => bytes.push(b'\x08'),
            b'f' => bytes.push(b'\x0c'),
            b'0'..=b'7' => {
                // One to three octal digits. Overflow of the high-order bits is
                // ignored, as the standard says.
                let mut value = u32::from(byte - b'0');
                for _ in 0..2 {
                    match self.peek() {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            self.position += 1;
                        }
                        _ => break,
                    }
                }
                bytes.push((value & 0xff) as u8);
            }
            // `\(`, `\)` and `\\` stand for themselves. For any other character the
            // backslash is ignored, as the standard says.
            other => bytes.push(other),
        }
        Ok(())
    }

    // <48656C6C6F> (section 7.3.4.3).
    fn hexadecimal_string(&mut self) -> Result<TokenKind<'a>> {
        let start = self.position;
        self.position += 1;
        let mut bytes = Vec::new();
        let mut high_digit = None;

        loop {
            let Some(byte) = self.peek() else {
                return Err(Error::new(ErrorKind::UnexpectedEnd, self.position));
            };
            if byte == b'>' {
                self.position += 1;
                break;
            }
            if !is_whitespace(byte) {
                let Some(digit) = hex_value(byte) else {
                    return Err(Error::new(ErrorKind::InvalidHexString, self.position));
                };
                match high_digit.take() {
                    None => high_digit = Some(digit),
                    Some(high) => {
                        bytes.push(high << 4 | digit);
                        self.check_length(bytes.len(), Limit::StringLength, start)?;
                    }
                }
            }
            self.position += 1;
        }

        // An odd number of digits behaves as if a final 0 followed.
        if let Some(high) = high_digit {
            bytes.push(high << 4);
            self.check_length(bytes.len(), Limit::StringLength, start)?;
        }
        Ok(TokenKind::String(PdfString::new(
            bytes,
            StringFormat::Hexadecimal,
        )))
    }

    // /Name with #xx escapes (section 7.3.5).
    fn name(&mut self) -> Result<TokenKind<'a>> {
        let start = self.position;
        self.position += 1;
        let raw = self.take_regular_run();
        // Most names have no escapes, so they are copied straight from the input.
        if !raw.contains(&b'#') {
            self.check_length(raw.len(), Limit::NameLength, start)?;
            return Ok(TokenKind::Name(Name::new(raw)));
        }
        let mut bytes = Vec::with_capacity(raw.len());
        let mut index = 0;

        while index < raw.len() {
            let escaped = raw
                .get(index + 1..index + 3)
                .and_then(|pair| Some(hex_value(pair[0])? << 4 | hex_value(pair[1])?));
            match (raw[index], escaped) {
                (b'#', Some(byte)) => {
                    bytes.push(byte);
                    index += 3;
                }
                // A `#` without two hex digits is kept as is. PDF 1.0 allowed it.
                (byte, _) => {
                    bytes.push(byte);
                    index += 1;
                }
            }
        }

        self.check_length(bytes.len(), Limit::NameLength, start)?;
        Ok(TokenKind::Name(Name::new(bytes)))
    }

    fn number_or_keyword(&mut self) -> Result<TokenKind<'a>> {
        let start = self.position;
        let run = self.take_regular_run();
        let looks_numeric = matches!(run[0], b'0'..=b'9' | b'+' | b'-' | b'.');
        if !looks_numeric {
            return Ok(TokenKind::Keyword(run));
        }
        parse_number(run).map_err(|kind| Error::new(kind, start))
    }
}

// Numbers have an optional sign, digits and at most one period, with at least one
// digit (section 7.3.3). Exponents such as `1e5` are not allowed.
fn parse_number(run: &[u8]) -> std::result::Result<TokenKind<'static>, ErrorKind> {
    let digits = run
        .strip_prefix(b"+")
        .or(run.strip_prefix(b"-"))
        .unwrap_or(run);
    let periods = digits.iter().filter(|&&b| b == b'.').count();
    let valid = periods <= 1
        && digits.iter().any(u8::is_ascii_digit)
        && digits.iter().all(|&b| b.is_ascii_digit() || b == b'.');
    if !valid {
        return Err(ErrorKind::InvalidNumber);
    }

    // Only ASCII digits, signs and a period remain, so the text is valid UTF-8.
    let text = std::str::from_utf8(run).map_err(|_| ErrorKind::InvalidNumber)?;
    if periods == 0 {
        text.parse()
            .map(TokenKind::Integer)
            .map_err(|_| ErrorKind::NumberOutOfRange)
    } else {
        text.parse()
            .map(TokenKind::Real)
            .map_err(|_| ErrorKind::InvalidNumber)
    }
}
