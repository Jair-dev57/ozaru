use ozaru_engine::base::Limits;
use ozaru_engine::syntax::{
    Dictionary, Name, Object, ObjectId, PdfString, StringFormat, parse_indirect_object,
    parse_object,
};
use ozaru_engine::writer::{write_indirect, write_object};

fn written(object: &Object) -> Vec<u8> {
    let mut out = Vec::new();
    write_object(&mut out, object);
    out
}

fn read_back(object: &Object) -> Object {
    let bytes = written(object);
    let parsed = parse_object(&bytes, Limits::default()).expect("written objects parse");
    assert_eq!(
        parsed.end,
        bytes.len(),
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(parsed.diagnostics.is_empty());
    parsed.object
}

fn dictionary(entries: &[(&str, Object)]) -> Dictionary {
    let mut dictionary = Dictionary::new();
    for (key, value) in entries {
        dictionary.insert(Name::from(*key), value.clone());
    }
    dictionary
}

#[test]
fn simple_objects_are_written_as_pdf_syntax() {
    assert_eq!(written(&Object::Null), b"null");
    assert_eq!(written(&Object::Boolean(true)), b"true");
    assert_eq!(written(&Object::Boolean(false)), b"false");
    assert_eq!(written(&Object::Integer(-42)), b"-42");
    assert_eq!(written(&Object::Real(0.5)), b"0.5");
    assert_eq!(written(&Object::Reference(ObjectId::new(12, 3))), b"12 3 R");
    assert_eq!(written(&Object::Name(Name::from("Type"))), b"/Type");
}

#[test]
fn reals_have_a_period_and_no_exponent() {
    assert_eq!(written(&Object::Real(3.0)), b"3.0");
    assert_eq!(written(&Object::Real(-12.0)), b"-12.0");
    assert_eq!(written(&Object::Real(0.0000001)), b"0.0000001");
    assert_eq!(written(&Object::Real(1e20)), b"100000000000000000000.0");
    assert_eq!(written(&Object::Real(f64::NAN)), b"0.0");
    assert_eq!(written(&Object::Real(f64::INFINITY)), b"0.0");
    assert_eq!(written(&Object::Real(-0.0)), b"0.0");
}

#[test]
fn reals_read_back_as_the_same_number() {
    for value in [0.1, -0.25, 612.0, 1.0 / 3.0, 123456.789, 1e-12, -7e15] {
        assert_eq!(read_back(&Object::Real(value)), Object::Real(value));
    }
}

#[test]
fn literal_strings_escape_parentheses_backslashes_and_carriage_returns() {
    let string = PdfString::new(&b"a(b)c\\d\re\nf"[..], StringFormat::Literal);

    assert_eq!(
        written(&Object::String(string.clone())),
        b"(a\\(b\\)c\\\\d\\re\nf)"
    );
    assert_eq!(
        read_back(&Object::String(string.clone())),
        Object::String(string)
    );
}

#[test]
fn hexadecimal_strings_stay_hexadecimal() {
    let string = PdfString::new(vec![0x00, 0xAB, 0xFF], StringFormat::Hexadecimal);

    assert_eq!(written(&Object::String(string.clone())), b"<00ABFF>");
    assert_eq!(
        read_back(&Object::String(string.clone())),
        Object::String(string)
    );
}

#[test]
fn every_byte_reads_back_in_both_string_forms() {
    let bytes: Vec<u8> = (0..=255).collect();
    for format in [StringFormat::Literal, StringFormat::Hexadecimal] {
        let string = Object::String(PdfString::new(bytes.clone(), format));
        assert_eq!(read_back(&string), string);
    }
}

#[test]
fn names_escape_delimiters_whitespace_and_hash() {
    let name = Name::new(b"A B/C#D(\x00\xFF");

    assert_eq!(
        written(&Object::Name(name.clone())),
        b"/A#20B#2FC#23D#28#00#FF"
    );
    assert_eq!(read_back(&Object::Name(name.clone())), Object::Name(name));
}

#[test]
fn nested_arrays_and_dictionaries_read_back() {
    let object = Object::Dictionary(dictionary(&[
        ("Type", Object::Name(Name::from("Page"))),
        ("Parent", Object::Reference(ObjectId::new(2, 0))),
        (
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(595.28),
                Object::Real(841.89),
            ]),
        ),
        (
            "Resources",
            Object::Dictionary(dictionary(&[(
                "Font",
                Object::Dictionary(dictionary(&[(
                    "F1",
                    Object::Reference(ObjectId::new(7, 0)),
                )])),
            )])),
        ),
        ("Empty", Object::Array(Vec::new())),
        ("Nothing", Object::Dictionary(Dictionary::new())),
        (
            "Flags",
            Object::Array(vec![Object::Boolean(true), Object::Null]),
        ),
    ]));

    assert_eq!(read_back(&object), object);
}

#[test]
fn indirect_objects_read_back_with_their_number() {
    let object = Object::Array(vec![Object::Integer(1), Object::Name(Name::from("X"))]);
    let mut out = b"%PDF-1.7\n".to_vec();

    let offset = write_indirect(&mut out, ObjectId::new(5, 2), &object, None);

    assert_eq!(offset, 9);
    let parsed = parse_indirect_object(&out, offset, Limits::default()).unwrap();
    assert_eq!(parsed.object.id, ObjectId::new(5, 2));
    assert_eq!(parsed.object.object, object);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn streams_get_their_data_and_the_right_length() {
    let mut stream_dictionary = dictionary(&[
        ("Length", Object::Reference(ObjectId::new(9, 0))),
        ("Filter", Object::Name(Name::from("FlateDecode"))),
    ]);
    stream_dictionary.insert(Name::from("Subtype"), Object::Name(Name::from("Form")));
    let data = b"binary\nendstream\x00\xFF data";
    let mut out = Vec::new();

    write_indirect(
        &mut out,
        ObjectId::new(3, 0),
        &Object::Dictionary(stream_dictionary),
        Some(data),
    );

    let parsed = parse_indirect_object(&out, 0, Limits::default()).unwrap();
    assert!(parsed.diagnostics.is_empty());
    let stream = parsed.object.object.as_stream().expect("a stream");
    assert_eq!(stream.raw_data(&out), Some(&data[..]));
    let written = stream.dictionary();
    assert_eq!(
        written.get("Length"),
        Some(&Object::Integer(data.len() as i64))
    );
    assert_eq!(
        written.get("Filter"),
        Some(&Object::Name(Name::from("FlateDecode")))
    );
    assert_eq!(
        written.get("Subtype"),
        Some(&Object::Name(Name::from("Form")))
    );
}
