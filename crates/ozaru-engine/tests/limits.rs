use ozaru_engine::base::{Limit, Limits};

#[test]
fn default_limits_are_set() {
    let limits = Limits::default();

    assert_eq!(limits.max_depth, 256);
    assert_eq!(limits.max_string_length, 64 * 1024 * 1024);
    assert_eq!(limits.max_name_length, 64 * 1024);
    assert_eq!(limits.max_array_length, 1_000_000);
    assert_eq!(limits.max_dictionary_length, 100_000);
    assert_eq!(limits.max_decoded_length, 256 * 1024 * 1024);
    assert_eq!(limits.max_xref_sections, 1_000);
    assert_eq!(limits.max_resolve_depth, 32);
    assert_eq!(limits.max_diagnostics, 1_000);
}

#[test]
fn the_depth_is_capped_at_the_safe_maximum() {
    let deep = Limits {
        max_depth: usize::MAX,
        ..Limits::default()
    };
    let shallow = Limits {
        max_depth: 8,
        ..Limits::default()
    };

    assert_eq!(deep.depth(), Limits::MAX_DEPTH);
    assert_eq!(shallow.depth(), 8);
    assert!(Limits::default().max_depth <= Limits::MAX_DEPTH);
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
        Limit::DecodedLength,
        Limit::XrefSections,
        Limit::ResolveDepth,
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
            "dictionary length",
            "decoded stream length",
            "cross-reference sections",
            "reference resolution depth",
        ]
    );
}
