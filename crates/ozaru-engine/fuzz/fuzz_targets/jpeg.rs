#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::filters::decode_jpeg;

// Decodes the input as the JPEG data of an image, with the color transform
// its first byte says. It never panics, every error is inside the limits, and
// the image has the size it says.
fuzz_target!(|data: &[u8]| {
    let Some((&flag, data)) = data.split_first() else {
        return;
    };
    let transform = match flag % 3 {
        0 => None,
        1 => Some(false),
        _ => Some(true),
    };
    let limits = Limits {
        max_decoded_length: 1 << 18,
        ..Limits::default()
    };
    if let Ok(image) = decode_jpeg(data, transform, 0, limits) {
        assert_eq!(
            image.data.len(),
            image.width * image.height * image.components
        );
    }
});
