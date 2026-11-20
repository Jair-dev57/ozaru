use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::syntax::{
    Dictionary, Name, Object, ObjectId, Parser, PdfString, StringFormat, parse_object,
};

fn parse(input: &[u8]) -> Object {
    parse_object(input, Limits::default())
        .expect("input should be valid")
        .object
}

fn parse_error_with(input: &[u8], limits: Limits) -> (ErrorKind, usize) {
    let error = parse_object(input, limits).expect_err("input should be invalid");
    (error.kind(), error.offset())
}

fn parse_error(input: &[u8]) -> (ErrorKind, usize) {
    parse_error_with(input, Limits::default())
}

fn all_objects(input: &[u8]) -> Vec<Object> {
    let mut parser = Parser::new(input, Limits::default());
    let mut objects = Vec::new();
    while let Some(object) = parser.next_object().expect("input should be valid") {
        objects.push(object);
    }
    objects
}

fn name(value: &str) -> Object {
    Object::Name(Name::from(value))
}

fn literal(value: &str) -> Object {
    Object::String(PdfString::new(value, StringFormat::Literal))
}

fn reference(number: u32, generation: u16) -> Object {
    Object::Reference(ObjectId::new(number, generation))
}

fn dictionary(entries: Vec<(&str, Object)>) -> Object {
    let mut dictionary = Dictionary::new();
    for (key, value) in entries {
        dictionary.insert(Name::from(key), value);
    }
    Object::Dictionary(dictionary)
}

// Simple objects

#[test]
fn booleans_and_null() {
    assert_eq!(parse(b"true"), Object::Boolean(true));
    assert_eq!(parse(b"false"), Object::Boolean(false));
    assert_eq!(parse(b"null"), Object::Null);
}

#[test]
fn numbers_strings_and_names() {
    assert_eq!(parse(b"-17"), Object::Integer(-17));
    assert_eq!(parse(b"3.5"), Object::Real(3.5));
    assert_eq!(parse(b"(Hello)"), literal("Hello"));
    assert_eq!(
        parse(b"<48656C6C6F>"),
        Object::String(PdfString::new("Hello", StringFormat::Hexadecimal))
    );
    assert_eq!(parse(b"/Type"), name("Type"));
}

#[test]
fn unknown_keywords_are_an_error() {
    assert_eq!(parse_error(b"  obj"), (ErrorKind::UnexpectedToken, 2));
    assert_eq!(parse_error(b"Tj"), (ErrorKind::UnexpectedToken, 0));
    assert_eq!(parse_error(b"True"), (ErrorKind::UnexpectedToken, 0));
}

// Arrays

// 3.14 comes from the example in the standard, it isn't meant to be pi.
#[allow(clippy::approx_constant)]
#[test]
fn arrays() {
    assert_eq!(
        parse(b"[549 3.14 false (Ralph) /SomeName]"),
        Object::Array(vec![
            Object::Integer(549),
            Object::Real(3.14),
            Object::Boolean(false),
            literal("Ralph"),
            name("SomeName"),
        ])
    );
    assert_eq!(parse(b"[]"), Object::Array(vec![]));
}

#[test]
fn nested_arrays() {
    assert_eq!(
        parse(b"[[1] [[2]]]"),
        Object::Array(vec![
            Object::Array(vec![Object::Integer(1)]),
            Object::Array(vec![Object::Array(vec![Object::Integer(2)])]),
        ])
    );
}

#[test]
fn unterminated_arrays_are_an_error_at_the_end_of_the_input() {
    assert_eq!(parse_error(b"[1 2"), (ErrorKind::UnexpectedEnd, 4));
    assert_eq!(parse_error(b"[[1]"), (ErrorKind::UnexpectedEnd, 4));
}

#[test]
fn a_closing_bracket_without_an_opening_one_is_an_error() {
    assert_eq!(parse_error(b"  ]"), (ErrorKind::UnexpectedToken, 2));
    assert_eq!(parse_error(b"[1 >>"), (ErrorKind::UnexpectedToken, 3));
}

// Dictionaries

#[test]
fn the_dictionary_example_from_the_standard() {
    let input = b"<< /Type /Example
        /Subtype /DictionaryExample
        /Version 0.01
        /IntegerItem 12
        /StringItem (a string)
        /Subdictionary << /Item1 0.4
                          /Item2 true
                          /LastItem (not !)
                          /VeryLastItem (OK)
                       >>
    >>";

    assert_eq!(
        parse(input),
        dictionary(vec![
            ("Type", name("Example")),
            ("Subtype", name("DictionaryExample")),
            ("Version", Object::Real(0.01)),
            ("IntegerItem", Object::Integer(12)),
            ("StringItem", literal("a string")),
            (
                "Subdictionary",
                dictionary(vec![
                    ("Item1", Object::Real(0.4)),
                    ("Item2", Object::Boolean(true)),
                    ("LastItem", literal("not !")),
                    ("VeryLastItem", literal("OK")),
                ])
            ),
        ])
    );
}

#[test]
fn empty_dictionary() {
    assert_eq!(parse(b"<<>>"), Object::Dictionary(Dictionary::new()));
}

#[test]
fn dictionaries_without_spaces() {
    assert_eq!(
        parse(b"<</Type/Page/Count 3/Kids[]>>"),
        dictionary(vec![
            ("Type", name("Page")),
            ("Count", Object::Integer(3)),
            ("Kids", Object::Array(vec![])),
        ])
    );
}

#[test]
fn a_repeated_key_keeps_the_last_value() {
    assert_eq!(
        parse(b"<< /A 1 /B 2 /A 3 >>"),
        dictionary(vec![("A", Object::Integer(3)), ("B", Object::Integer(2))])
    );
}

#[test]
fn a_repeated_key_is_reported_where_it_appears_again() {
    let parsed = parse_object(b"<< /A 1 /B 2 /A 3 >>", Limits::default()).unwrap();
    let diagnostics: Vec<_> = parsed
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect();

    assert_eq!(diagnostics, [(DiagnosticKind::DuplicateDictionaryKey, 13)]);
}

#[test]
fn repeated_keys_in_nested_dictionaries_are_reported() {
    let parsed = parse_object(
        b"[<< /A 1 /A 2 >> << /B << /C 1 /C 2 >> >>]",
        Limits::default(),
    )
    .unwrap();
    let offsets: Vec<_> = parsed.diagnostics.iter().map(|d| d.offset()).collect();

    assert_eq!(offsets, [9, 31]);
}

#[test]
fn the_same_key_in_different_dictionaries_is_not_repeated() {
    let parsed = parse_object(b"[<< /A 1 >> << /A 2 >>]", Limits::default()).unwrap();

    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn valid_objects_have_no_diagnostics() {
    let parsed = parse_object(b"<< /Type /Page /Kids [4 0 R] >>", Limits::default()).unwrap();

    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn the_parser_collects_diagnostics_across_objects() {
    let mut parser = Parser::new(b"<< /A 1 /A 2 >> << /B 1 /B 2 >>", Limits::default());
    while parser.next_object().unwrap().is_some() {}

    assert_eq!(parser.diagnostics().len(), 2);
    assert_eq!(parser.take_diagnostics().len(), 2);
    assert!(parser.diagnostics().is_empty());
}

#[test]
fn diagnostics_beyond_the_limit_are_only_counted() {
    let limits = Limits {
        max_diagnostics: 1,
        ..Limits::default()
    };
    let parsed = parse_object(b"<< /A 1 /A 2 /A 3 /A 4 >>", limits).unwrap();

    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.diagnostics.dropped(), 2);
}

#[test]
fn dictionary_keys_must_be_names() {
    assert_eq!(
        parse_error(b"<< 1 2 >>"),
        (ErrorKind::InvalidDictionaryKey, 3)
    );
    assert_eq!(
        parse_error(b"<< /A 1 (B) 2 >>"),
        (ErrorKind::InvalidDictionaryKey, 8)
    );
    assert_eq!(
        parse_error(b"<< [/A] 1 >>"),
        (ErrorKind::InvalidDictionaryKey, 3)
    );
}

#[test]
fn a_key_without_a_value_is_an_error() {
    assert_eq!(
        parse_error(b"<< /A 1 /B >>"),
        (ErrorKind::MissingDictionaryValue, 11)
    );
}

#[test]
fn unterminated_dictionaries_are_an_error() {
    assert_eq!(parse_error(b"<< /A 1"), (ErrorKind::UnexpectedEnd, 7));
    assert_eq!(
        parse_error(b"<< /A [1] ]"),
        (ErrorKind::UnexpectedToken, 10)
    );
}

// References

#[test]
fn references() {
    assert_eq!(parse(b"12 0 R"), reference(12, 0));
    assert_eq!(parse(b"7 65535 R"), reference(7, 65535));
}

#[test]
fn references_inside_arrays_and_dictionaries() {
    assert_eq!(
        parse(b"[4 0 R 5 0 R 6]"),
        Object::Array(vec![reference(4, 0), reference(5, 0), Object::Integer(6)])
    );
    assert_eq!(
        parse(b"<< /Pages 2 0 R /Count 1 >>"),
        dictionary(vec![
            ("Pages", reference(2, 0)),
            ("Count", Object::Integer(1))
        ])
    );
}

#[test]
fn integers_that_are_not_references_stay_integers() {
    assert_eq!(
        parse(b"[1 2 3]"),
        Object::Array(vec![
            Object::Integer(1),
            Object::Integer(2),
            Object::Integer(3)
        ])
    );
    assert_eq!(
        parse(b"[1 2 /R]"),
        Object::Array(vec![Object::Integer(1), Object::Integer(2), name("R")])
    );
}

#[test]
fn a_reference_needs_two_integers() {
    assert_eq!(parse_error(b"[1 2.0 R]"), (ErrorKind::UnexpectedToken, 7));
    assert_eq!(parse_error(b"[1 R]"), (ErrorKind::UnexpectedToken, 3));
}

#[test]
fn references_out_of_range_are_an_error() {
    assert_eq!(parse_error(b"  -1 0 R"), (ErrorKind::InvalidReference, 2));
    assert_eq!(
        parse_error(b"4294967296 0 R"),
        (ErrorKind::InvalidReference, 0)
    );
    assert_eq!(parse_error(b"1 65536 R"), (ErrorKind::InvalidReference, 0));
    assert_eq!(parse_error(b"1 -1 R"), (ErrorKind::InvalidReference, 0));
}

// Several objects

#[test]
fn objects_are_read_one_after_another() {
    assert_eq!(
        all_objects(b"1 2 3 0 R (a) % comment\n /B"),
        [
            Object::Integer(1),
            Object::Integer(2),
            reference(3, 0),
            literal("a"),
            name("B"),
        ]
    );
}

#[test]
fn parse_object_returns_where_the_object_ends() {
    let parsed = parse_object(b"  [1 2]  (rest)", Limits::default()).unwrap();

    assert_eq!(
        parsed.object,
        Object::Array(vec![Object::Integer(1), Object::Integer(2)])
    );
    assert_eq!(parsed.end, 7);
}

#[test]
fn the_position_does_not_include_tokens_read_ahead() {
    let mut parser = Parser::new(b"5 6 (x)", Limits::default());

    assert_eq!(parser.next_object().unwrap(), Some(Object::Integer(5)));
    assert_eq!(parser.position(), 1);
    assert_eq!(parser.next_object().unwrap(), Some(Object::Integer(6)));
    assert_eq!(parser.position(), 3);
}

#[test]
fn an_error_after_an_object_is_reported_on_the_next_call() {
    let mut parser = Parser::new(b"5 )", Limits::default());

    assert_eq!(parser.next_object().unwrap(), Some(Object::Integer(5)));
    let error = parser.next_object().unwrap_err();
    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::UnexpectedByte(b')'), 2)
    );
}

#[test]
fn empty_input_has_no_object() {
    assert_eq!(all_objects(b"  % only a comment"), []);
    assert_eq!(parse_error(b"   "), (ErrorKind::UnexpectedEnd, 3));
}

#[test]
fn procedures_are_not_objects() {
    assert_eq!(parse_error(b"{ 1 }"), (ErrorKind::UnexpectedToken, 0));
}

// Limits

#[test]
fn nesting_deeper_than_the_limit_is_an_error() {
    let limits = Limits {
        max_depth: 2,
        ..Limits::default()
    };

    assert!(parse_object(b"[[1]]", limits).is_ok());
    assert_eq!(
        parse_error_with(b"[[[1]]]", limits),
        (ErrorKind::LimitExceeded(Limit::Depth), 2)
    );
    assert_eq!(
        parse_error_with(b"[<< /A [1] >>]", limits),
        (ErrorKind::LimitExceeded(Limit::Depth), 7)
    );
}

#[test]
fn very_deep_nesting_is_an_error_not_a_crash() {
    let deep = "[".repeat(10_000);

    assert_eq!(
        parse_error(deep.as_bytes()),
        (ErrorKind::LimitExceeded(Limit::Depth), 256)
    );
}

#[test]
fn the_depth_limit_cant_be_raised_past_the_safe_maximum() {
    let limits = Limits {
        max_depth: usize::MAX,
        ..Limits::default()
    };
    let input = "[".repeat(100_000);

    assert_eq!(
        parse_error_with(input.as_bytes(), limits),
        (ErrorKind::LimitExceeded(Limit::Depth), Limits::MAX_DEPTH)
    );
}

#[test]
fn objects_at_the_safe_maximum_depth_can_be_used_and_dropped() {
    let limits = Limits {
        max_depth: usize::MAX,
        ..Limits::default()
    };
    let depth = Limits::MAX_DEPTH;
    let input = format!("{}{}", "[".repeat(depth), "]".repeat(depth));

    let object = parse_object(input.as_bytes(), limits).unwrap().object;
    let copy = object.clone();

    assert_eq!(object, copy);
    assert!(!format!("{object:?}").is_empty());
}

#[test]
fn arrays_longer_than_the_limit_are_an_error() {
    let limits = Limits {
        max_array_length: 2,
        ..Limits::default()
    };

    assert!(parse_object(b"[1 2]", limits).is_ok());
    assert_eq!(
        parse_error_with(b"[1 2 3]", limits),
        (ErrorKind::LimitExceeded(Limit::ArrayLength), 5)
    );
}

#[test]
fn dictionaries_longer_than_the_limit_are_an_error() {
    let limits = Limits {
        max_dictionary_length: 1,
        ..Limits::default()
    };

    assert!(parse_object(b"<< /A 1 /A 2 >>", limits).is_ok());
    assert_eq!(
        parse_error_with(b"<< /A 1 /B 2 >>", limits),
        (ErrorKind::LimitExceeded(Limit::DictionaryLength), 11)
    );
}
