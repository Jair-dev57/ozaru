#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::xref::{find_startxref, parse_xref_table};

// The startxref offset must be inside the input, and a table must end inside it.
// Otherwise it's an error.
fuzz_target!(|data: &[u8]| {
    match find_startxref(data, Limits::default()) {
        Ok(found) => assert!(found.offset < data.len()),
        Err(error) => assert!(error.offset() <= data.len()),
    }
    match parse_xref_table(data, 0, Limits::default()) {
        Ok(parsed) => {
            assert!(parsed.end <= data.len());
            assert!(parsed.table.iter().count() == parsed.table.len());
        }
        Err(error) => assert!(error.offset() <= data.len()),
    }
});
