#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::writer::{Editor, Encryption, RANDOM_BYTES};
use ozaru_engine::{Document, OpenOptions};

// Opens the input as a PDF and saves it encrypted, with the first bytes of the
// input as passwords. It never panics.
fuzz_target!(|data: &[u8]| {
    let options = OpenOptions {
        password: None,
        ..OpenOptions::default()
    };
    let Ok(mut document) = Document::open_with(data, options) else {
        return;
    };
    if document.needs_password() {
        return;
    }
    let Ok(mut editor) = Editor::new(&mut document) else {
        return;
    };
    let password = &data[..data.len().min(200)];
    let encryption = Encryption {
        user_password: password,
        owner_password: &password[password.len() / 2..],
        permissions: u32::from(data.first().copied().unwrap_or(0)) << 2,
        encrypt_metadata: data.len() % 2 == 0,
    };
    let _ = editor.save_encrypted(&encryption, &[7; RANDOM_BYTES]);
});
