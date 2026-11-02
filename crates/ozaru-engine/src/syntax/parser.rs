use std::collections::VecDeque;

use super::{Dictionary, Lexer, Name, Object, ObjectId, Token, TokenKind};
use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Limit, Limits, Result};

/// Parses direct objects from PDF bytes (ISO 32000-2, section 7.3).
///
/// Nested arrays and dictionaries are handled with an explicit stack, not recursion,
/// so deeply nested input can't overflow the call stack.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    // Tokens read ahead to recognize `n g R`. A lexer error is kept in order and
    // only reported when the parser reaches it.
    lookahead: VecDeque<Result<Token>>,
    limits: Limits,
    input_length: usize,
    position: usize,
    diagnostics: Diagnostics,
}

/// An object parsed by [`parse_object`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedObject {
    pub object: Object,
    /// Offset just past the object.
    pub end: usize,
    /// Problems found and worked around while parsing it.
    pub diagnostics: Diagnostics,
}

// An array or dictionary that is still open.
enum Container {
    Array {
        items: Vec<Object>,
        start: usize,
    },
    Dictionary {
        dictionary: Dictionary,
        // The key waiting for its value, and where it starts.
        key: Option<(Name, usize)>,
        start: usize,
    },
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a [u8], limits: Limits) -> Self {
        Self {
            lexer: Lexer::new(input, limits),
            lookahead: VecDeque::new(),
            limits,
            input_length: input.len(),
            position: 0,
            diagnostics: Diagnostics::new(limits.max_diagnostics),
        }
    }

    /// Problems found and worked around so far.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// Returns the diagnostics collected so far and starts a new, empty list.
    pub fn take_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(
            &mut self.diagnostics,
            Diagnostics::new(self.limits.max_diagnostics),
        )
    }

    /// Offset just past the last object returned.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Returns the next object, or `None` at the end of the input.
    pub fn next_object(&mut self) -> Result<Option<Object>> {
        let mut open: Vec<Container> = Vec::new();

        loop {
            let Some(token) = self.next_token()? else {
                if open.is_empty() {
                    return Ok(None);
                }
                return Err(Error::new(ErrorKind::UnexpectedEnd, self.input_length));
            };

            let (value, start) = match token.kind {
                TokenKind::Integer(number) => {
                    (self.integer_or_reference(number, token.start)?, token.start)
                }
                TokenKind::Real(value) => (Object::Real(value), token.start),
                TokenKind::String(value) => (Object::String(value), token.start),
                TokenKind::Name(value) => (Object::Name(value), token.start),
                TokenKind::Keyword(word) => (keyword(&word, token.start)?, token.start),
                TokenKind::ArrayStart => {
                    self.check_depth(&open, token.start)?;
                    open.push(Container::Array {
                        items: Vec::new(),
                        start: token.start,
                    });
                    continue;
                }
                TokenKind::DictionaryStart => {
                    self.check_depth(&open, token.start)?;
                    open.push(Container::Dictionary {
                        dictionary: Dictionary::new(),
                        key: None,
                        start: token.start,
                    });
                    continue;
                }
                TokenKind::ArrayEnd => match open.pop() {
                    Some(Container::Array { items, start }) => (Object::Array(items), start),
                    _ => return Err(unexpected_token(token.start)),
                },
                TokenKind::DictionaryEnd => match open.pop() {
                    Some(Container::Dictionary {
                        dictionary,
                        key: None,
                        start,
                    }) => (Object::Dictionary(dictionary), start),
                    Some(Container::Dictionary { key: Some(_), .. }) => {
                        return Err(Error::new(ErrorKind::MissingDictionaryValue, token.start));
                    }
                    _ => return Err(unexpected_token(token.start)),
                },
                TokenKind::ProcedureStart | TokenKind::ProcedureEnd => {
                    return Err(unexpected_token(token.start));
                }
            };

            match open.last_mut() {
                None => return Ok(Some(value)),
                Some(Container::Array { items, .. }) => {
                    if items.len() >= self.limits.max_array_length {
                        return Err(limit_exceeded(Limit::ArrayLength, start));
                    }
                    items.push(value);
                }
                Some(Container::Dictionary {
                    dictionary, key, ..
                }) => match key.take() {
                    None => match value {
                        Object::Name(name) => *key = Some((name, start)),
                        _ => return Err(Error::new(ErrorKind::InvalidDictionaryKey, start)),
                    },
                    Some((name, key_start)) => {
                        if dictionary.len() >= self.limits.max_dictionary_length
                            && !dictionary.contains_key(name.as_bytes())
                        {
                            return Err(limit_exceeded(Limit::DictionaryLength, start));
                        }
                        // A repeated key keeps the last value, as PDF readers do.
                        if dictionary.insert(name, value).is_some() {
                            self.diagnostics
                                .push(DiagnosticKind::DuplicateDictionaryKey, key_start);
                        }
                    }
                },
            }
        }
    }

    fn check_depth(&self, open: &[Container], start: usize) -> Result<()> {
        if open.len() >= self.limits.depth() {
            return Err(limit_exceeded(Limit::Depth, start));
        }
        Ok(())
    }

    // `n g R` is a reference, so an integer needs two tokens of lookahead.
    fn integer_or_reference(&mut self, number: i64, start: usize) -> Result<Object> {
        let generation = match self.peek(0) {
            Some(Ok(Token {
                kind: TokenKind::Integer(generation),
                ..
            })) => *generation,
            _ => return Ok(Object::Integer(number)),
        };
        if !matches!(self.peek(1), Some(Ok(Token { kind: TokenKind::Keyword(word), .. })) if word == b"R")
        {
            return Ok(Object::Integer(number));
        }
        self.next_token()?;
        self.next_token()?;

        match (u32::try_from(number), u16::try_from(generation)) {
            (Ok(number), Ok(generation)) => {
                Ok(Object::Reference(ObjectId::new(number, generation)))
            }
            _ => Err(Error::new(ErrorKind::InvalidReference, start)),
        }
    }

    fn peek(&mut self, distance: usize) -> Option<&Result<Token>> {
        while self.lookahead.len() <= distance {
            let next = self.lexer.next_token().transpose()?;
            let is_error = next.is_err();
            self.lookahead.push_back(next);
            if is_error {
                break;
            }
        }
        self.lookahead.get(distance)
    }

    fn next_token(&mut self) -> Result<Option<Token>> {
        let token = match self.lookahead.pop_front() {
            Some(token) => Some(token?),
            None => self.lexer.next_token()?,
        };
        if let Some(token) = &token {
            self.position = token.end;
        }
        Ok(token)
    }
}

/// Parses the first object in `input`.
pub fn parse_object(input: &[u8], limits: Limits) -> Result<ParsedObject> {
    let mut parser = Parser::new(input, limits);
    match parser.next_object()? {
        Some(object) => Ok(ParsedObject {
            object,
            end: parser.position(),
            diagnostics: parser.take_diagnostics(),
        }),
        None => Err(Error::new(ErrorKind::UnexpectedEnd, input.len())),
    }
}

fn keyword(word: &[u8], start: usize) -> Result<Object> {
    match word {
        b"true" => Ok(Object::Boolean(true)),
        b"false" => Ok(Object::Boolean(false)),
        b"null" => Ok(Object::Null),
        _ => Err(unexpected_token(start)),
    }
}

fn unexpected_token(start: usize) -> Error {
    Error::new(ErrorKind::UnexpectedToken, start)
}

fn limit_exceeded(limit: Limit, start: usize) -> Error {
    Error::new(ErrorKind::LimitExceeded(limit), start)
}
