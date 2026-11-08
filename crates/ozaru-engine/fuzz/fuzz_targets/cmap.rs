#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::{Diagnostics, Limits};
use ozaru_engine::fonts::ToUnicode;

// Reads the input as a ToUnicode CMap and looks up codes in it. Ranges are never
// expanded, so even a huge one takes no memory.
fuzz_target!(|data: &[u8]| {
    let mut diagnostics = Diagnostics::new(10);
    let to_unicode = ToUnicode::parse(data, Limits::default(), 0, &mut diagnostics);
    for code in [0, 1, 0x20, 0x41, 0xFF, 0x100, 0xFFFF, 0x1_0000, u32::MAX] {
        let _ = to_unicode.get(code);
    }
    for (low, high) in to_unicode.codespace() {
        assert!(low.length == high.length && low.value <= high.value);
        assert!((1..=4).contains(&low.length));
    }
});
