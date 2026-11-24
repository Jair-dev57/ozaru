#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::text::{bidi_class, bidi_levels, paragraph_level};

// Any text, taken as a paragraph: the algorithm gives a level for each character,
// none deeper than the standard allows, and never panics.
fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let count = text.chars().count();
    for paragraph in [None, Some(0), Some(1)] {
        let levels = bidi_levels(&text, paragraph);
        assert_eq!(levels.len(), count);
        assert!(levels.iter().all(|&level| level <= 126));
    }
    assert!(paragraph_level(&text) <= 1);
    for c in text.chars().take(64) {
        let _ = bidi_class(c);
    }
});
