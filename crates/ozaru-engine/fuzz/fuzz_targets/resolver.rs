#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::ObjectId;
use ozaru_engine::xref::Resolver;

// Opens the input as a whole file and resolves every object its cross-reference
// data lists, plus a few it doesn't. Each is an object or an error.
fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    let Ok(mut resolver) = Resolver::open(data, limits) else {
        return;
    };
    let numbers: Vec<u32> = resolver
        .table()
        .iter()
        .map(|(number, _)| number)
        .take(1000)
        .collect();
    for number in numbers.into_iter().chain(0..4) {
        for generation in [0, 1] {
            if let Err(error) = resolver.resolve(ObjectId::new(number, generation)) {
                assert!(error.offset() <= data.len());
            }
        }
    }
});
