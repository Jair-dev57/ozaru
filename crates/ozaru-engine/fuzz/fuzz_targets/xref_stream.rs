#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::xref::parse_xref_stream;

// A cross-reference stream must end inside the input. Otherwise it's an error.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    match parse_xref_stream(data, 0, limits) {
        Ok(parsed) => assert!(parsed.end <= data.len()),
        Err(error) => assert!(error.offset() <= data.len()),
    }
});
