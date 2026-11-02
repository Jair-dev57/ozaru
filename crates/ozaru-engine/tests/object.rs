use ozaru_engine::syntax::{Dictionary, Name, Object, ObjectId, PdfString, StringFormat};

#[test]
fn accessors_return_the_value_of_the_matching_variant() {
    assert!(Object::Null.is_null());
    assert_eq!(Object::Boolean(true).as_bool(), Some(true));
    assert_eq!(Object::Integer(-17).as_integer(), Some(-17));
    assert_eq!(
        Object::Name(Name::from("Type")).as_name(),
        Some(&Name::from("Type"))
    );
    assert_eq!(
        Object::Reference(ObjectId::new(5, 0)).as_reference(),
        Some(ObjectId::new(5, 0))
    );
    assert_eq!(
        Object::Array(vec![Object::Null]).as_array(),
        Some(&[Object::Null][..])
    );
}

#[test]
fn accessors_return_none_for_other_variants() {
    let integer = Object::Integer(3);

    assert!(!integer.is_null());
    assert_eq!(integer.as_bool(), None);
    assert_eq!(integer.as_name(), None);
    assert_eq!(integer.as_string(), None);
    assert_eq!(integer.as_array(), None);
    assert_eq!(integer.as_dictionary(), None);
    assert_eq!(integer.as_reference(), None);
    assert_eq!(integer.as_stream(), None);
    assert_eq!(Object::Real(1.5).as_integer(), None);
}

#[test]
fn integers_and_reals_are_both_numbers() {
    assert_eq!(Object::Integer(3).as_number(), Some(3.0));
    assert_eq!(Object::Real(-0.5).as_number(), Some(-0.5));
    assert_eq!(Object::Boolean(true).as_number(), None);
}

#[test]
fn strings_keep_their_bytes_and_format() {
    let string = PdfString::new(b"caf\xe9".to_vec(), StringFormat::Hexadecimal);

    assert_eq!(string.as_bytes(), b"caf\xe9");
    assert_eq!(string.format(), StringFormat::Hexadecimal);
    assert_eq!(string.into_bytes(), b"caf\xe9".to_vec());
}

#[test]
fn strings_with_the_same_bytes_but_different_formats_are_different() {
    assert_ne!(
        PdfString::new("A", StringFormat::Literal),
        PdfString::new("A", StringFormat::Hexadecimal)
    );
}

#[test]
fn names_display_as_they_are_written_in_a_pdf() {
    assert_eq!(Name::from("Type").to_string(), "/Type");
    assert_eq!(Name::new(b"A B#".to_vec()).to_string(), "/A#20B#23");
    assert_eq!(Name::new(b"\xe9".to_vec()).to_string(), "/#E9");
    assert_eq!(Name::from("").to_string(), "/");
}

#[test]
fn object_ids_display_as_references() {
    assert_eq!(ObjectId::new(12, 3).to_string(), "12 3 R");
}

#[test]
fn dictionaries_find_values_by_key() {
    let mut dictionary = Dictionary::new();
    dictionary.insert(Name::from("Type"), Object::Name(Name::from("Page")));

    assert_eq!(
        dictionary.get("Type"),
        Some(&Object::Name(Name::from("Page")))
    );
    assert_eq!(
        dictionary.get(b"Type"),
        Some(&Object::Name(Name::from("Page")))
    );
    assert!(dictionary.contains_key("Type"));
    assert_eq!(dictionary.get("Count"), None);
}

#[test]
fn dictionaries_keep_insertion_order() {
    let mut dictionary = Dictionary::new();
    for key in ["Type", "Count", "Kids"] {
        dictionary.insert(Name::from(key), Object::Null);
    }

    let keys: Vec<String> = dictionary
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(keys, ["/Type", "/Count", "/Kids"]);
}

#[test]
fn inserting_an_existing_key_replaces_it_in_place() {
    let mut dictionary = Dictionary::new();
    dictionary.insert(Name::from("A"), Object::Integer(1));
    dictionary.insert(Name::from("B"), Object::Integer(2));

    let old = dictionary.insert(Name::from("A"), Object::Integer(3));

    assert_eq!(old, Some(Object::Integer(1)));
    assert_eq!(dictionary.len(), 2);
    assert_eq!(
        dictionary.iter().next(),
        Some((&Name::from("A"), &Object::Integer(3)))
    );
}

#[test]
fn new_dictionaries_are_empty() {
    assert!(Dictionary::new().is_empty());
    assert_eq!(Dictionary::new().len(), 0);
}
