#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::filters::{InflateStatus, deflate, deflate_zlib, inflate};

// The first byte picks the level. What is compressed must inflate to the same
// bytes at every level, and any bytes, taken as Deflate data, inflate without
// panicking and within the limit.
fuzz_target!(|data: &[u8]| {
    let Some((&choice, data)) = data.split_first() else {
        return;
    };
    let level = choice % 11;
    let (back, status) = inflate(&deflate(data, level), false, usize::MAX);
    assert_eq!(status, InflateStatus::Done);
    assert!(back == data);
    let (back, status) = inflate(&deflate_zlib(data, level), true, usize::MAX);
    assert_eq!(status, InflateStatus::Done);
    assert!(back == data);

    let (out, _) = inflate(data, choice & 1 == 0, 1 << 20);
    assert!(out.len() <= 1 << 20);
});
