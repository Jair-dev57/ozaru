#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::Lexer;

// The lexer must return tokens in order, inside the input, or an error.
fuzz_target!(|data: &[u8]| {
    let mut lexer = Lexer::new(data, Limits::default());
    let mut previous_end = 0;
    loop {
        match lexer.next_token() {
            Ok(Some(token)) => {
                assert!(previous_end <= token.start && token.start < token.end);
                assert!(token.end <= data.len());
                previous_end = token.end;
            }
            Ok(None) => break,
            Err(error) => {
                assert!(error.offset() <= data.len());
                break;
            }
        }
    }
});
