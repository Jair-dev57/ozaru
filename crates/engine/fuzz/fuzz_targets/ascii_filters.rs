#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::filters::{decode_ascii_hex, decode_ascii85};

// Each decoder returns data within the size limit, or an error at the data's
// offset.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    for decode in [decode_ascii_hex, decode_ascii85] {
        match decode(data, 0, limits) {
            Ok(decoded) => assert!(decoded.data.len() <= limits.max_decoded_length),
            Err(error) => assert_eq!(error.offset(), 0),
        }
    }
});
