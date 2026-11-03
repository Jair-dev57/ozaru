#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::xref::{XrefEntry, rebuild_xref};

// Every rebuilt entry in use points inside the input. Otherwise it's an error.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    if let Ok(rebuilt) = rebuild_xref(data, limits) {
        for (_, entry) in rebuilt.table.iter() {
            if let XrefEntry::InUse { offset, .. } = entry {
                assert!(offset < data.len());
            }
        }
    }
});
