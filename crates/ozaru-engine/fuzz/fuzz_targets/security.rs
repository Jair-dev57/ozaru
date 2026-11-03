#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::security::StandardSecurity;
use ozaru_engine::syntax::parse_object;

// Reads the input as an /Encrypt dictionary and tries the empty password and the
// input itself. It never panics, and a key always has the advertised length.
fuzz_target!(|data: &[u8]| {
    let Ok(parsed) = parse_object(data, Limits::default()) else {
        return;
    };
    let Some(encrypt) = parsed.object.as_dictionary() else {
        return;
    };
    if let Ok(security) = StandardSecurity::new(encrypt, data, 0) {
        for password in [&b""[..], data] {
            if let Some(key) = security.authenticate(password) {
                assert_eq!(key.as_bytes().len(), security.key_length());
            }
        }
    }
});
