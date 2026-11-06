#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::security::StandardSecurity;
use ozaru_engine::syntax::parse_object;

// Reads the input as an /Encrypt dictionary and tries the empty password and the
// input itself. It never panics, a key always has the advertised length, and
// decryption never makes data longer.
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
                let id = ozaru_engine::syntax::ObjectId::new(1, 0);
                let mut diagnostics = ozaru_engine::base::Diagnostics::new(100);
                if let Ok(plain) = security.decrypt_stream_data(&key, id, data, 0, &mut diagnostics)
                {
                    assert!(plain.len() <= data.len());
                }
                let mut object = parsed.object.clone();
                let _ = security.decrypt_object(&key, id, &mut object, 0, &mut diagnostics);
            }
        }
    }
});
