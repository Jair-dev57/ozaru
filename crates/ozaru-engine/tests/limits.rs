use ozaru_engine::base::{Limit, Limits};

#[test]
fn default_limits_are_set() {
    let limits = Limits::default();

    assert_eq!(limits.max_depth, 256);
    assert_eq!(limits.max_string_length, 64 * 1024 * 1024);
    assert_eq!(limits.max_name_length, 64 * 1024);
    assert_eq!(limits.max_array_length, 1_000_000);
    assert_eq!(limits.max_dictionary_length, 100_000);
}

#[test]
fn limits_can_be_changed() {
    let limits = Limits {
        max_depth: 8,
        ..Limits::default()
    };

    assert_eq!(limits.max_depth, 8);
}

#[test]
fn limits_have_readable_names() {
    let names: Vec<String> = [
        Limit::Depth,
        Limit::StringLength,
        Limit::NameLength,
        Limit::ArrayLength,
        Limit::DictionaryLength,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    assert_eq!(
        names,
        [
            "nesting depth",
            "string length",
            "name length",
            "array length",
            "dictionary length"
        ]
    );
}
