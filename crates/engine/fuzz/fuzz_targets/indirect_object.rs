#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::{Object, parse_indirect_object};

// An indirect object, with or without a stream, must end inside the input, and a
// stream's data must be a valid range of it. Otherwise it's an error.
fuzz_target!(|data: &[u8]| {
    match parse_indirect_object(data, 0, Limits::default()) {
        Ok(parsed) => {
            assert!(parsed.end > 0 && parsed.end <= data.len());
            if let Object::Stream(stream) = &parsed.object.object {
                let range = stream.data_range();
                assert!(range.start <= range.end && range.end <= parsed.end);
                assert!(stream.raw_data(data).is_some());
            }
        }
        Err(error) => assert!(error.offset() <= data.len()),
    }
});
