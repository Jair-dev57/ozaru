#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::ObjectId;
use ozaru_engine::xref::Resolver;

// Opens the input as a whole file and resolves every object its cross-reference
// data lists, plus a few it doesn't. Each is an object or an error, and every
// error and diagnostic is inside the input.
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
    // Encrypted files are decrypted with the empty password or "user", and each
    // stream is an error or decoded data within the limit.
    resolver.unlock(b"user");
    let numbers: Vec<u32> = resolver
        .table()
        .iter()
        .map(|(number, _)| number)
        .take(200)
        .collect();
    for number in numbers {
        if let Ok(decoded) = resolver.stream_data(ObjectId::new(number, 0)) {
            assert!(decoded.data.len() <= limits.max_decoded_length);
        }
    }
    assert!(
        resolver
            .diagnostics()
            .iter()
            .all(|d| d.offset() <= data.len())
    );
});
