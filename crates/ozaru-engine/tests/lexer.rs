use ozaru_engine::base::{ErrorKind, Limit, Limits};
use ozaru_engine::syntax::{Lexer, Name, PdfString, StringFormat, Token, TokenKind};

fn tokens_with(input: &[u8], limits: Limits) -> Result<Vec<TokenKind<'_>>, (ErrorKind, usize)> {
    let mut lexer = Lexer::new(input, limits);
    let mut kinds = Vec::new();
    loop {
        match lexer.next_token() {
            Ok(Some(token)) => kinds.push(token.kind),
            Ok(None) => return Ok(kinds),
            Err(error) => return Err((error.kind(), error.offset())),
        }
    }
}

fn tokens(input: &[u8]) -> Vec<TokenKind<'_>> {
    tokens_with(input, Limits::default()).expect("input should be valid")
}

fn error(input: &[u8]) -> (ErrorKind, usize) {
    tokens_with(input, Limits::default()).expect_err("input should be invalid")
}

fn literal(bytes: &[u8]) -> TokenKind<'_> {
    TokenKind::String(PdfString::new(bytes, StringFormat::Literal))
}

fn hexadecimal(bytes: &[u8]) -> TokenKind<'_> {
    TokenKind::String(PdfString::new(bytes, StringFormat::Hexadecimal))
}

fn name(bytes: &[u8]) -> TokenKind<'_> {
    TokenKind::Name(Name::new(bytes))
}

fn keyword(bytes: &[u8]) -> TokenKind<'_> {
    TokenKind::Keyword(bytes)
}

// Whitespace and comments

#[test]
fn empty_input_has_no_tokens() {
    assert_eq!(tokens(b""), []);
    assert_eq!(tokens(b" \t\r\n\x0c\0"), []);
}

#[test]
fn all_six_whitespace_characters_separate_tokens() {
    assert_eq!(
        tokens(b"1\x002\t3\n4\x0c5\r6 7"),
        (1..=7).map(TokenKind::Integer).collect::<Vec<_>>()
    );
}

#[test]
fn comments_are_skipped_until_the_end_of_the_line() {
    assert_eq!(
        tokens(b"1 % a comment\n2"),
        [TokenKind::Integer(1), TokenKind::Integer(2)]
    );
    assert_eq!(
        tokens(b"1 %comment\r2"),
        [TokenKind::Integer(1), TokenKind::Integer(2)]
    );
    assert_eq!(tokens(b"%PDF-1.7"), []);
}

#[test]
fn a_comment_ends_a_token() {
    assert_eq!(tokens(b"true%comment"), [keyword(b"true")]);
}

#[test]
fn a_percent_sign_inside_a_string_is_not_a_comment() {
    assert_eq!(tokens(b"(50%)"), [literal(b"50%")]);
}

// Token positions

#[test]
fn tokens_know_where_they_start_and_end() {
    let mut lexer = Lexer::new(b"  /Type  12", Limits::default());

    assert_eq!(
        lexer.next_token().unwrap(),
        Some(Token {
            kind: name(b"Type"),
            start: 2,
            end: 7
        })
    );
    assert_eq!(
        lexer.next_token().unwrap(),
        Some(Token {
            kind: TokenKind::Integer(12),
            start: 9,
            end: 11
        })
    );
    assert_eq!(lexer.next_token().unwrap(), None);
    assert_eq!(lexer.position(), 11);
}

// Numbers

#[test]
fn integers() {
    assert_eq!(
        tokens(b"123 43445 +17 -98 0"),
        [123, 43445, 17, -98, 0].map(TokenKind::Integer)
    );
}

#[test]
fn reals_in_every_form_the_standard_allows() {
    assert_eq!(
        tokens(b"34.5 -3.62 +123.6 4. -.002 0.0 .5"),
        [34.5, -3.62, 123.6, 4.0, -0.002, 0.0, 0.5].map(TokenKind::Real)
    );
}

#[test]
fn the_largest_and_smallest_integers_are_read() {
    assert_eq!(
        tokens(b"9223372036854775807 -9223372036854775808"),
        [TokenKind::Integer(i64::MAX), TokenKind::Integer(i64::MIN)]
    );
}

#[test]
fn integers_around_18_digits_are_read() {
    // 18 digits take the fast path, 19 the general one.
    assert_eq!(
        tokens(b"999999999999999999 -999999999999999999 1000000000000000000 +007 -0"),
        [
            TokenKind::Integer(999_999_999_999_999_999),
            TokenKind::Integer(-999_999_999_999_999_999),
            TokenKind::Integer(1_000_000_000_000_000_000),
            TokenKind::Integer(7),
            TokenKind::Integer(0),
        ]
    );
}

#[test]
fn integers_that_dont_fit_are_an_error() {
    assert_eq!(
        error(b"9223372036854775808"),
        (ErrorKind::NumberOutOfRange, 0)
    );
}

#[test]
fn invalid_numbers_are_an_error_at_their_start() {
    for input in [
        &b"  1.2.3"[..],
        b"  1e5",
        b"  --5",
        b"  -",
        b"  .",
        b"  +",
        b"  12abc",
    ] {
        assert_eq!(error(input), (ErrorKind::InvalidNumber, 2), "{input:?}");
    }
}

#[test]
fn numbers_end_at_delimiters() {
    assert_eq!(
        tokens(b"[1 2.5]"),
        [
            TokenKind::ArrayStart,
            TokenKind::Integer(1),
            TokenKind::Real(2.5),
            TokenKind::ArrayEnd
        ]
    );
}

// Keywords

#[test]
fn words_that_are_not_numbers_are_keywords() {
    assert_eq!(
        tokens(b"true false null R obj Tj T* ' \""),
        [
            keyword(b"true"),
            keyword(b"false"),
            keyword(b"null"),
            keyword(b"R"),
            keyword(b"obj"),
            keyword(b"Tj"),
            keyword(b"T*"),
            keyword(b"'"),
            keyword(b"\"")
        ]
    );
}

#[test]
fn a_reference_is_three_tokens() {
    assert_eq!(
        tokens(b"12 0 R"),
        [TokenKind::Integer(12), TokenKind::Integer(0), keyword(b"R")]
    );
}

// Literal strings

#[test]
fn literal_strings() {
    assert_eq!(
        tokens(b"(This is a string)"),
        [literal(b"This is a string")]
    );
    assert_eq!(tokens(b"()"), [literal(b"")]);
}

#[test]
fn balanced_parentheses_need_no_escape() {
    assert_eq!(
        tokens(b"(Strings may contain (balanced) parentheses ( ) and special characters (*!&}^% and so on).)"),
        [literal(b"Strings may contain (balanced) parentheses ( ) and special characters (*!&}^% and so on).")]
    );
}

#[test]
fn escape_sequences() {
    assert_eq!(
        tokens(br"(\n\r\t\b\f\(\)\\)"),
        [literal(b"\n\r\t\x08\x0c()\\")]
    );
}

#[test]
fn escaped_parentheses_dont_count_for_balance() {
    assert_eq!(tokens(br"(a\)b)"), [literal(b"a)b")]);
    assert_eq!(tokens(br"(\()"), [literal(b"(")]);
}

#[test]
fn octal_escapes_with_one_two_or_three_digits() {
    assert_eq!(tokens(br"(\0053)"), [literal(b"\x053")]);
    assert_eq!(tokens(br"(\053)"), [literal(b"+")]);
    assert_eq!(tokens(br"(\53)"), [literal(b"+")]);
    assert_eq!(tokens(br"(\5)"), [literal(b"\x05")]);
    assert_eq!(tokens(br"(caf\351)"), [literal(b"caf\xe9")]);
}

#[test]
fn octal_escapes_ignore_overflow() {
    assert_eq!(tokens(br"(\777)"), [literal(b"\xff")]);
}

#[test]
fn backslash_before_an_end_of_line_joins_the_lines() {
    assert_eq!(
        tokens(b"(These \\\ntwo strings \\\r\nare the same.)"),
        [literal(b"These two strings are the same.")]
    );
    assert_eq!(tokens(b"(a\\\rb)"), [literal(b"ab")]);
}

#[test]
fn backslash_before_an_unknown_character_is_ignored() {
    assert_eq!(tokens(br"(\q\%)"), [literal(b"q%")]);
}

#[test]
fn every_end_of_line_inside_a_string_becomes_a_line_feed() {
    assert_eq!(tokens(b"(a\rb\r\nc\nd)"), [literal(b"a\nb\nc\nd")]);
}

#[test]
fn unterminated_literal_strings_are_an_error() {
    assert_eq!(error(b"(abc"), (ErrorKind::UnexpectedEnd, 4));
    assert_eq!(error(b"(a(b)"), (ErrorKind::UnexpectedEnd, 5));
    assert_eq!(error(b"(abc\\"), (ErrorKind::UnexpectedEnd, 5));
}

// Hexadecimal strings

#[test]
fn hexadecimal_strings() {
    assert_eq!(
        tokens(b"<4E6F762073686D6F7A206B6120706F702E>"),
        [hexadecimal(b"Nov shmoz ka pop.")]
    );
    assert_eq!(tokens(b"<>"), [hexadecimal(b"")]);
}

#[test]
fn hexadecimal_digits_can_be_in_either_case() {
    assert_eq!(tokens(b"<aBcD>"), [hexadecimal(b"\xab\xcd")]);
}

#[test]
fn whitespace_inside_hexadecimal_strings_is_ignored() {
    assert_eq!(tokens(b"<48 65\n6C\t6C 6F>"), [hexadecimal(b"Hello")]);
}

#[test]
fn an_odd_number_of_digits_is_padded_with_zero() {
    assert_eq!(tokens(b"<901FA>"), [hexadecimal(b"\x90\x1f\xa0")]);
    assert_eq!(tokens(b"<901FA3>"), [hexadecimal(b"\x90\x1f\xa3")]);
}

#[test]
fn non_hexadecimal_characters_are_an_error_where_they_appear() {
    assert_eq!(error(b"<48G5>"), (ErrorKind::InvalidHexString, 3));
}

#[test]
fn unterminated_hexadecimal_strings_are_an_error() {
    assert_eq!(error(b"<4865"), (ErrorKind::UnexpectedEnd, 5));
}

// Names

#[test]
fn names_from_the_standard() {
    assert_eq!(
        tokens(b"/Name1 /ASomewhatLongerName /A;Name_With-Various***Characters? /1.2 /$$ /@pattern /.notdef"),
        [
            name(b"Name1"),
            name(b"ASomewhatLongerName"),
            name(b"A;Name_With-Various***Characters?"),
            name(b"1.2"),
            name(b"$$"),
            name(b"@pattern"),
            name(b".notdef")
        ]
    );
}

#[test]
fn hex_escapes_in_names_are_decoded() {
    assert_eq!(
        tokens(b"/Lime#20Green /paired#28#29parentheses /The_Key_of_F#23_Minor /A#42"),
        [
            name(b"Lime Green"),
            name(b"paired()parentheses"),
            name(b"The_Key_of_F#_Minor"),
            name(b"AB")
        ]
    );
}

#[test]
fn a_hash_without_two_hex_digits_is_kept() {
    assert_eq!(tokens(b"/A#G1 /B#"), [name(b"A#G1"), name(b"B#")]);
}

#[test]
fn a_slash_alone_is_an_empty_name() {
    assert_eq!(tokens(b"/ /"), [name(b""), name(b"")]);
}

#[test]
fn names_end_at_delimiters() {
    assert_eq!(tokens(b"/Type/Page"), [name(b"Type"), name(b"Page")]);
    assert_eq!(tokens(b"/A(b)"), [name(b"A"), literal(b"b")]);
}

// Delimiters

#[test]
fn array_dictionary_and_procedure_delimiters() {
    assert_eq!(
        tokens(b"[ ] << >> { }"),
        [
            TokenKind::ArrayStart,
            TokenKind::ArrayEnd,
            TokenKind::DictionaryStart,
            TokenKind::DictionaryEnd,
            TokenKind::ProcedureStart,
            TokenKind::ProcedureEnd
        ]
    );
}

#[test]
fn a_dictionary_without_spaces() {
    assert_eq!(
        tokens(b"<</Type/Page/Count 3>>"),
        [
            TokenKind::DictionaryStart,
            name(b"Type"),
            name(b"Page"),
            name(b"Count"),
            TokenKind::Integer(3),
            TokenKind::DictionaryEnd
        ]
    );
}

#[test]
fn a_dictionary_start_is_not_a_hexadecimal_string() {
    assert_eq!(
        tokens(b"<<>>"),
        [TokenKind::DictionaryStart, TokenKind::DictionaryEnd]
    );
}

#[test]
fn a_lone_closing_parenthesis_or_angle_bracket_is_an_error() {
    assert_eq!(error(b"  )"), (ErrorKind::UnexpectedByte(b')'), 2));
    assert_eq!(error(b"1 >"), (ErrorKind::UnexpectedByte(b'>'), 2));
}

// Limits

#[test]
fn strings_longer_than_the_limit_are_an_error() {
    let limits = Limits {
        max_string_length: 3,
        ..Limits::default()
    };

    assert_eq!(tokens_with(b"(abc)", limits), Ok(vec![literal(b"abc")]));
    assert_eq!(
        tokens_with(b"  (abcd)", limits),
        Err((ErrorKind::LimitExceeded(Limit::StringLength), 2))
    );
    assert_eq!(
        tokens_with(b"<61626364>", limits),
        Err((ErrorKind::LimitExceeded(Limit::StringLength), 0))
    );
    assert_eq!(
        tokens_with(b"<6162636>", limits),
        Err((ErrorKind::LimitExceeded(Limit::StringLength), 0))
    );
}

#[test]
fn names_longer_than_the_limit_are_an_error() {
    let limits = Limits {
        max_name_length: 3,
        ..Limits::default()
    };

    assert_eq!(tokens_with(b"/abc", limits), Ok(vec![name(b"abc")]));
    assert_eq!(
        tokens_with(b"/abcd", limits),
        Err((ErrorKind::LimitExceeded(Limit::NameLength), 0))
    );
}

#[test]
fn name_limit_counts_decoded_bytes() {
    let limits = Limits {
        max_name_length: 1,
        ..Limits::default()
    };

    assert_eq!(tokens_with(b"/#41", limits), Ok(vec![name(b"A")]));
}
