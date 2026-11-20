use ozaru_engine::syntax::{Dictionary, Name, Object, Stream};

fn length_dictionary(length: i64) -> Dictionary {
    let mut dictionary = Dictionary::new();
    dictionary.insert(Name::from("Length"), Object::Integer(length));
    dictionary
}

#[test]
fn streams_keep_their_dictionary_and_data_range() {
    let stream = Stream::new(length_dictionary(5), 20..25);

    assert_eq!(stream.dictionary().get("Length"), Some(&Object::Integer(5)));
    assert_eq!(stream.data_range(), 20..25);
}

#[test]
fn raw_data_is_taken_from_the_file() {
    let file = b"1 0 obj << /Length 5 >> stream\nHello\nendstream endobj";
    let start = file.windows(5).position(|w| w == b"Hello").unwrap();
    let stream = Stream::new(length_dictionary(5), start..start + 5);

    assert_eq!(stream.raw_data(file), Some(&b"Hello"[..]));
}

#[test]
fn raw_data_is_none_when_the_range_does_not_fit_the_file() {
    let stream = Stream::new(Dictionary::new(), 10..20);

    assert_eq!(stream.raw_data(b"short"), None);
}

#[test]
fn an_empty_stream_has_no_data() {
    let stream = Stream::new(length_dictionary(0), 7..7);

    assert_eq!(stream.raw_data(b"0123456789"), Some(&b""[..]));
}

#[test]
fn streams_are_objects() {
    let stream = Stream::new(Dictionary::new(), 0..3);
    let object = Object::Stream(stream.clone());

    assert_eq!(object.as_stream(), Some(&stream));
    assert_eq!(object.as_dictionary(), None);
}
