#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::filters::{CcittParameters, decode_ccitt};

// Decodes the input as fax data, with parameters from its first four bytes:
// the mode, whether lines are aligned on bytes, black is 1, and the columns
// and rows. It never panics, and its rows have the size of the columns.
fuzz_target!(|data: &[u8]| {
    let [mode, flags, columns, rows, rest @ ..] = data else {
        return;
    };
    let parameters = CcittParameters {
        k: i64::from(*mode % 3) - 1,
        encoded_byte_align: flags & 1 != 0,
        black_is_1: flags & 2 != 0,
        columns: usize::from(*columns) * 8 + usize::from(flags >> 2),
        rows: usize::from(*rows),
    };
    let limits = Limits {
        max_decoded_length: 1 << 20,
        ..Limits::default()
    };
    if let Ok(decoded) = decode_ccitt(rest, &parameters, 0, limits) {
        let row = parameters.columns.div_ceil(8);
        assert_eq!(decoded.data.len() % row, 0);
        if parameters.rows > 0 {
            assert_eq!(decoded.data.len(), parameters.rows * row);
        }
    }
});
