//! Saving with a password: AES-256, user and owner passwords, permissions.

use ozaru_engine::base::ErrorKind;
use ozaru_engine::text::document_text;
use ozaru_engine::writer::{Editor, Encryption, RANDOM_BYTES};
use ozaru_engine::{Document, OpenOptions};

mod common;
use common::one_page;

fn random(start: u8) -> Vec<u8> {
    (0..RANDOM_BYTES)
        .map(|i| start.wrapping_add(i as u8).wrapping_mul(7))
        .collect()
}

fn encrypted(user: &[u8], owner: &[u8], permissions: u32, metadata: bool) -> Vec<u8> {
    let data = one_page("BT /F1 12 Tf 72 700 Td (Secret text) Tj ET", "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let encryption = Encryption {
        user_password: user,
        owner_password: owner,
        permissions,
        encrypt_metadata: metadata,
    };
    editor.save_encrypted(&encryption, &random(1)).unwrap()
}

fn text_with(file: &[u8], password: Option<&[u8]>) -> Option<String> {
    let options = OpenOptions {
        password,
        ..OpenOptions::default()
    };
    let mut document = Document::open_with(file, options).ok()?;
    if document.needs_password() {
        return None;
    }
    let pages = document_text(&mut document);
    Some(
        pages
            .into_iter()
            .map(|page| page.text)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[test]
fn the_user_and_owner_passwords_open_the_file_and_others_dont() {
    let file = encrypted(b"user", b"owner", 4, true);
    assert!(file.starts_with(b"%PDF-2.0"));
    assert!(!file.windows(11).any(|w| w == b"Secret text"));
    assert!(
        text_with(&file, Some(b"user"))
            .unwrap()
            .contains("Secret text")
    );
    assert!(
        text_with(&file, Some(b"owner"))
            .unwrap()
            .contains("Secret text")
    );
    assert!(text_with(&file, Some(b"nope")).is_none());
    assert!(text_with(&file, None).is_none());
}

#[test]
fn an_empty_user_password_lets_anyone_read() {
    let file = encrypted(b"", b"owner", 0, true);
    assert!(text_with(&file, None).unwrap().contains("Secret text"));
}

#[test]
fn the_permissions_are_saved() {
    let file = encrypted(b"", b"owner", 4 | 16, false);
    let document = Document::open(&file).unwrap();
    assert!(document.is_encrypted());
    assert_eq!(document.permissions().unwrap() & 0xFFF, 4 | 16 | 0xC0);
}

#[test]
fn too_few_random_bytes_is_an_error() {
    let data = one_page("", "");
    let mut document = Document::open(&data).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let encryption = Encryption {
        user_password: b"",
        owner_password: b"",
        permissions: 0,
        encrypt_metadata: true,
    };
    let error = editor.save_encrypted(&encryption, &[1; 10]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::NotEnoughRandom);
}

#[test]
fn an_encrypted_file_can_be_saved_again_with_other_passwords() {
    let file = encrypted(b"one", b"two", 4, true);
    let options = OpenOptions {
        password: Some(b"one"),
        ..OpenOptions::default()
    };
    let mut document = Document::open_with(&file, options).unwrap();
    let mut editor = Editor::new(&mut document).unwrap();
    let encryption = Encryption {
        user_password: b"three",
        owner_password: b"four",
        permissions: 0,
        encrypt_metadata: true,
    };
    let again = editor.save_encrypted(&encryption, &random(9)).unwrap();
    assert!(text_with(&again, Some(b"one")).is_none());
    assert!(
        text_with(&again, Some(b"three"))
            .unwrap()
            .contains("Secret text")
    );
}
