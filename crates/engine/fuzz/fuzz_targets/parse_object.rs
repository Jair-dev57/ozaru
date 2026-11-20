#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::Parser;

// The parser must keep moving forward and stay inside the input, or return an
// error. Each object is also cloned and compared, which visits all of it.
fuzz_target!(|data: &[u8]| {
    let mut parser = Parser::new(data, Limits::default());
    let mut previous_position = 0;
    loop {
        match parser.next_object() {
            Ok(Some(object)) => {
                assert!(parser.position() > previous_position);
                assert!(parser.position() <= data.len());
                previous_position = parser.position();
                assert_eq!(object.clone(), object);
            }
            Ok(None) => break,
            Err(error) => {
                assert!(error.offset() <= data.len());
                break;
            }
        }
    }
});
