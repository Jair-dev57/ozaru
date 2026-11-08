#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::{Diagnostics, Limits};
use ozaru_engine::fonts::{CidCMap, ToUnicode, parse_cmap};

// Reads the input as a ToUnicode CMap and as a CID CMap, and looks up codes in
// both. Ranges are never expanded, so even a huge one takes no memory, and
// splitting a string into codes uses all of its bytes.
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

    let cmap = CidCMap::from_data(parse_cmap(data, Limits::default()), None, None);
    let codes = cmap.codes(data);
    assert_eq!(codes.iter().map(|code| usize::from(code.length)).sum::<usize>(), data.len());
    for code in codes.into_iter().take(1000) {
        let _ = cmap.cid(code);
    }
});
