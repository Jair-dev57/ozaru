#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::document::{Xmp, parse_xmp_date};

// Reads the input as an XMP packet and as a date, and every property of the
// packet. It never panics, and dates are within their ranges.
fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Some(date) = parse_xmp_date(&text) {
        assert!((1..=12).contains(&date.month) && (1..=31).contains(&date.day));
        assert!(date.hour < 24 && date.minute < 60 && date.second < 60);
    }
    if let Some(xmp) = Xmp::parse(&text) {
        let _ = (xmp.title(), xmp.author(), xmp.subject(), xmp.keywords());
        let _ = (xmp.creator(), xmp.producer());
        let _ = (xmp.creation_date(), xmp.modification_date());
        assert_eq!(xmp.xml(), text);
    }
});
