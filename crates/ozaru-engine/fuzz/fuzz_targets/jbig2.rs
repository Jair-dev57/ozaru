#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::filters::decode_jbig2;

// Decodes the input as the JBIG2 data of an image, the first part, up to
// the byte given by its first byte, as its globals. It never panics, every
// error is inside the limits, and the page has the size it says.
fuzz_target!(|data: &[u8]| {
    let Some((&split, data)) = data.split_first() else {
        return;
    };
    let split = usize::from(split).min(data.len());
    let (globals, page) = data.split_at(split);
    let limits = Limits {
        max_decoded_length: 1 << 16,
        ..Limits::default()
    };
    let globals = (!globals.is_empty()).then_some(globals);
    if let Ok(page) = decode_jbig2(page, globals, 0, limits) {
        assert_eq!(page.data.len(), page.width.div_ceil(8) * page.height);
    }
});
