use ozaru_engine::base::{DiagnosticKind, Limits};
use ozaru_engine::content::{ContentParser, Operator};
use ozaru_engine::syntax::{Name, Object, PdfString, StringFormat};

// The operations of `content`, as their keywords and operands, and the
// diagnostics.
fn parse(content: &[u8]) -> (Vec<(String, Vec<Object>)>, Vec<DiagnosticKind>) {
    parse_with(content, Limits::default())
}

fn parse_with(content: &[u8], limits: Limits) -> (Vec<(String, Vec<Object>)>, Vec<DiagnosticKind>) {
    let mut parser = ContentParser::new(content, limits);
    let mut operations = Vec::new();
    while let Some(operation) = parser.next_operation() {
        let keyword = String::from_utf8_lossy(operation.operator.keyword()).into_owned();
        operations.push((keyword, operation.operands.to_vec()));
    }
    let diagnostics = parser.diagnostics().iter().map(|d| d.kind()).collect();
    (operations, diagnostics)
}

fn keywords(content: &[u8]) -> Vec<String> {
    parse(content)
        .0
        .into_iter()
        .map(|(keyword, _)| keyword)
        .collect()
}

fn literal(text: &str) -> Object {
    Object::String(PdfString::new(text.as_bytes(), StringFormat::Literal))
}

fn name(text: &str) -> Object {
    Object::Name(Name::new(text))
}

#[test]
fn operators_take_the_operands_before_them() {
    let (operations, diagnostics) = parse(b"BT /F1 12 Tf 72 712.5 Td (Hello) Tj ET");
    assert!(diagnostics.is_empty());
    assert_eq!(
        operations,
        vec![
            ("BT".to_string(), vec![]),
            ("Tf".to_string(), vec![name("F1"), Object::Integer(12)]),
            (
                "Td".to_string(),
                vec![Object::Integer(72), Object::Real(712.5)]
            ),
            ("Tj".to_string(), vec![literal("Hello")]),
            ("ET".to_string(), vec![]),
        ]
    );
}

#[test]
fn every_standard_operator_is_known() {
    let all = "w J j M d ri i gs q Q cm m l c v y h re S s f F f* B B* b b* n W W* \
               BT ET Tc Tw Tz TL Tf Tr Ts Td TD Tm T* Tj TJ ' \" d0 d1 CS cs SC SCN \
               sc scn G g RG rg K k sh Do MP DP BMC BDC EMC BX EX";
    let mut parser = ContentParser::new(all.as_bytes(), Limits::default());
    let mut count = 0;
    while let Some(operation) = parser.next_operation() {
        assert!(
            !matches!(operation.operator, Operator::Unknown(_)),
            "{:?}",
            operation.operator
        );
        // The keyword gives back the operator.
        assert_eq!(
            Operator::from_keyword(operation.operator.keyword()),
            operation.operator
        );
        count += 1;
    }
    // Every operator of Annex A but `BI`, `ID` and `EI`, read as one inline image.
    assert_eq!(count, 70);
}

#[test]
fn unknown_operators_are_kept_as_written() {
    let mut parser = ContentParser::new(b"1 2 foo", Limits::default());
    let operation = parser.next_operation().unwrap();
    assert_eq!(operation.operator, Operator::Unknown(b"foo"));
    assert_eq!(operation.operands, [Object::Integer(1), Object::Integer(2)]);
    assert_eq!(operation.offset, 4);
}

#[test]
fn arrays_and_dictionaries_are_operands() {
    let (operations, diagnostics) =
        parse(b"[(A) -120 (B) [1 2]] TJ /Span << /ActualText (fi) /N [true null] >> BDC");
    assert!(diagnostics.is_empty());
    assert_eq!(operations[0].0, "TJ");
    assert_eq!(
        operations[0].1,
        vec![Object::Array(vec![
            literal("A"),
            Object::Integer(-120),
            literal("B"),
            Object::Array(vec![Object::Integer(1), Object::Integer(2)]),
        ])]
    );
    assert_eq!(operations[1].0, "BDC");
    let properties = operations[1].1[1].as_dictionary().unwrap();
    assert_eq!(properties.get("ActualText"), Some(&literal("fi")));
    assert_eq!(
        properties.get("N"),
        Some(&Object::Array(vec![Object::Boolean(true), Object::Null]))
    );
}

#[test]
fn quotes_are_operators() {
    let (operations, _) = parse(b"(a) ' 1 2 (b) \"");
    assert_eq!(operations[0], ("'".to_string(), vec![literal("a")]));
    assert_eq!(
        operations[1],
        (
            "\"".to_string(),
            vec![Object::Integer(1), Object::Integer(2), literal("b")]
        )
    );
}

#[test]
fn comments_are_skipped() {
    assert_eq!(keywords(b"q % save\n1 0 0 1 0 0 cm%x\nQ"), ["q", "cm", "Q"]);
}

#[test]
fn trailing_operands_are_dropped() {
    let (operations, diagnostics) = parse(b"q 1 2");
    assert_eq!(operations, vec![("q".to_string(), vec![])]);
    assert_eq!(diagnostics, [DiagnosticKind::MissingOperator]);
}

#[test]
fn broken_tokens_are_skipped() {
    // A `)` alone, a bad number and a bad hex string.
    let (operations, diagnostics) = parse(b"q ) 1.2.3 <zz> 5 w Q");
    assert_eq!(keywords(b"q ) 1.2.3 5 w Q"), ["q", "w", "Q"]);
    assert!(diagnostics.contains(&DiagnosticKind::DamagedContent));
    assert_eq!(operations.first().unwrap().0, "q");
    assert_eq!(operations.last().unwrap().0, "Q");
    let width = operations.iter().find(|(k, _)| k == "w").unwrap();
    assert_eq!(width.1.last(), Some(&Object::Integer(5)));
}

#[test]
fn unbalanced_closers_are_skipped() {
    let (operations, diagnostics) = parse(b"] >> } 1 w");
    assert_eq!(
        operations,
        vec![("w".to_string(), vec![Object::Integer(1)])]
    );
    assert_eq!(diagnostics, [DiagnosticKind::DamagedContent; 3]);
}

#[test]
fn an_operator_inside_an_array_breaks_it() {
    // A `]` is missing: the array is dropped and `TJ` is read on its own.
    let (operations, diagnostics) = parse(b"BT [(a) 10 (b) TJ ET");
    assert_eq!(keywords(b"BT [(a) 10 (b) TJ ET"), ["BT", "TJ", "ET"]);
    assert_eq!(operations[1].1, vec![]);
    assert_eq!(diagnostics, [DiagnosticKind::DamagedContent]);
}

#[test]
fn content_cut_short_loses_the_last_operation() {
    let (operations, diagnostics) = parse(b"BT /F1 12 Tf [(Hel");
    assert_eq!(keywords(b"BT /F1 12 Tf [(Hel"), ["BT", "Tf"]);
    assert_eq!(operations.len(), 2);
    assert!(diagnostics.contains(&DiagnosticKind::DamagedContent));

    let (operations, _) = parse(b"BT /F1 12 Tf [(Hello) 5");
    assert_eq!(operations.len(), 2);
}

#[test]
fn dictionary_keys_must_be_names() {
    let (operations, diagnostics) = parse(b"/P << 1 2 /K 3 >> BDC");
    let properties = operations[0].1[1].as_dictionary().unwrap();
    assert_eq!(properties.get("K"), Some(&Object::Integer(3)));
    assert!(diagnostics.contains(&DiagnosticKind::DamagedContent));
}

#[test]
fn deep_nesting_is_skipped() {
    let mut content = b"q ".to_vec();
    content.extend(std::iter::repeat_n(b'[', 100_000));
    content.extend(b" 1 w Q");
    let (operations, diagnostics) = parse(&content);
    assert_eq!(
        operations
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["q", "w", "Q"]
    );
    assert!(diagnostics.contains(&DiagnosticKind::DamagedContent));
}

#[test]
fn operand_count_is_limited() {
    let limits = Limits {
        max_array_length: 3,
        ..Limits::default()
    };
    let (operations, diagnostics) = parse_with(b"1 2 3 4 5 re", limits);
    assert_eq!(operations[0].1.len(), 3);
    assert_eq!(diagnostics, [DiagnosticKind::DamagedContent]);
}

#[test]
fn inline_images_are_one_operation() {
    let content = b"q BI /W 2 /H 2 /BPC 8 /CS /G /F [/AHx] ID 00FF\nFF00> EI Q";
    let mut parser = ContentParser::new(content, Limits::default());
    assert_eq!(parser.next_operation().unwrap().operator, Operator::Save);
    let image = parser.next_operation().unwrap();
    assert_eq!(image.operator, Operator::InlineImage);
    let inline = image.inline_image.unwrap();
    assert_eq!(inline.dictionary.get("W"), Some(&Object::Integer(2)));
    assert_eq!(
        inline.dictionary.get("F"),
        Some(&Object::Array(vec![name("AHx")]))
    );
    assert_eq!(inline.data, b"00FF\nFF00>");
    assert_eq!(parser.next_operation().unwrap().operator, Operator::Restore);
    assert!(parser.next_operation().is_none());
    assert!(parser.diagnostics().is_empty());
}

#[test]
fn binary_inline_image_data_can_hold_ei() {
    // The first ` EI ` is followed by binary data, so it is part of the image.
    let mut content = b"BI /W 4 /H 1 /BPC 8 /CS /G ID ".to_vec();
    content.extend(b"\x01 EI \x80\x81\x82\x83 x");
    content.extend(b"\nEI\nQ");
    let mut parser = ContentParser::new(&content, Limits::default());
    let image = parser.next_operation().unwrap();
    assert_eq!(
        image.inline_image.unwrap().data,
        b"\x01 EI \x80\x81\x82\x83 x"
    );
    assert_eq!(parser.next_operation().unwrap().operator, Operator::Restore);
}

#[test]
fn inline_images_can_give_their_length() {
    // PDF 2.0's `/L`: the data holds ` EI ` followed by text.
    let content = b"BI /W 6 /H 1 /BPC 8 /CS /G /L 6 ID a EI b EI Q";
    let mut parser = ContentParser::new(content, Limits::default());
    let image = parser.next_operation().unwrap();
    assert_eq!(image.inline_image.unwrap().data, b"a EI b");
    assert_eq!(parser.next_operation().unwrap().operator, Operator::Restore);
}

#[test]
fn inline_images_without_ei_run_to_the_end() {
    let content = b"BI /W 1 /H 1 ID \x01\x02";
    let mut parser = ContentParser::new(content, Limits::default());
    let image = parser.next_operation().unwrap();
    assert_eq!(image.inline_image.unwrap().data, b"\x01\x02");
    assert!(parser.next_operation().is_none());
    let kinds: Vec<_> = parser.diagnostics().iter().map(|d| d.kind()).collect();
    assert_eq!(kinds, [DiagnosticKind::UnterminatedInlineImage]);
}

#[test]
fn inline_images_without_id_have_no_data() {
    let content = b"BI /W 1 /H 1 Q";
    let mut parser = ContentParser::new(content, Limits::default());
    let image = parser.next_operation().unwrap();
    assert_eq!(image.operator, Operator::InlineImage);
    assert_eq!(image.inline_image.unwrap().data, b"");
    // The operator that interrupted it is read on its own.
    assert_eq!(parser.next_operation().unwrap().operator, Operator::Restore);
}

#[test]
fn random_bytes_never_panic() {
    // A simple generator, so the test needs no dependency.
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let alphabet = b"()<>[]{}/% \n\r\t0123456789.-+abcdefBIDEQqTJjfw#\\\x00\xFF";
    for _ in 0..20_000 {
        let length = (state % 64) as usize;
        let mut input = Vec::with_capacity(length);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            input.push(alphabet[(state % alphabet.len() as u64) as usize]);
        }
        let mut parser = ContentParser::new(&input, Limits::default());
        let mut count = 0;
        while let Some(operation) = parser.next_operation() {
            assert!(operation.offset < input.len());
            count += 1;
            assert!(count <= input.len());
        }
        assert!(
            parser
                .diagnostics()
                .iter()
                .all(|d| d.offset() <= input.len())
        );
    }
}
