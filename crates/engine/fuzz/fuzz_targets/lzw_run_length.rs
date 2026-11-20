#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::filters::{decode_lzw, decode_run_length};
use ozaru_engine::syntax::parse_object;

// The first byte picks EarlyChange and whether to use a PNG predictor, the rest is
// the data. Decoded data stays under the limit, or it's an error at its offset.
fuzz_target!(|data: &[u8]| {
    let Some((&choice, data)) = data.split_first() else {
        return;
    };
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    let parameters = format!(
        "<< /EarlyChange {} {} >>",
        choice & 1,
        if choice & 2 == 0 {
            ""
        } else {
            "/Predictor 12 /Columns 4"
        }
    );
    let parameters = parse_object(parameters.as_bytes(), limits).unwrap().object;

    for result in [
        decode_lzw(data, parameters.as_dictionary(), 0, limits),
        decode_run_length(data, 0, limits),
    ] {
        match result {
            Ok(decoded) => assert!(decoded.data.len() <= limits.max_decoded_length),
            Err(error) => assert_eq!(error.offset(), 0),
        }
    }
});
