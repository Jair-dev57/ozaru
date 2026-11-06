#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::filters::decode_flate;
use ozaru_engine::syntax::parse_object;

// The first byte picks the predictor parameters, TIFF or PNG, the rest is the
// Flate data.
// Decoded data must stay under the limit. Otherwise it's an error.
fuzz_target!(|data: &[u8]| {
    let Some((&choice, data)) = data.split_first() else {
        return;
    };
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    let parameters = format!(
        "<< /Predictor {} /Colors {} /BitsPerComponent {} /Columns {} >>",
        [2, 10, 11, 12, 13, 14, 15][usize::from(choice) % 7],
        1 + (choice >> 3) % 4,
        [1, 2, 4, 8, 16][usize::from(choice >> 5) % 5],
        1 + choice % 7,
    );
    let parameters = parse_object(parameters.as_bytes(), limits).unwrap().object;

    for parameters in [None, parameters.as_dictionary()] {
        if let Ok(decoded) = decode_flate(data, parameters, 0, limits) {
            assert!(decoded.data.len() <= limits.max_decoded_length);
        }
    }
});
