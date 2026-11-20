#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::parse_object;
use ozaru_engine::writer::write_object;

// Any object the parser reads must be written as syntax that parses again, to
// an object written the same way: what a writer gives a reader is what it meant.
fuzz_target!(|data: &[u8]| {
    let Ok(parsed) = parse_object(data, Limits::default()) else {
        return;
    };
    let mut first = Vec::new();
    write_object(&mut first, &parsed.object);
    let again = parse_object(&first, Limits::default()).expect("written objects parse");
    assert_eq!(again.end, first.len());
    let mut second = Vec::new();
    write_object(&mut second, &again.object);
    assert_eq!(first, second);
});
