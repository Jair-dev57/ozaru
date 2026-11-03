#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::xref::{find_startxref, load_xref, parse_xref_table};

// The startxref offset must be inside the input, a table must end inside it, and
// a loaded chain has at least one section. Otherwise it's an error.
fuzz_target!(|data: &[u8]| {
    match find_startxref(data, Limits::default()) {
        Ok(found) => assert!(found.offset < data.len()),
        Err(error) => assert!(error.offset() <= data.len()),
    }
    match load_xref(data, Limits::default()) {
        Ok(loaded) => assert!(loaded.sections > 0),
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
