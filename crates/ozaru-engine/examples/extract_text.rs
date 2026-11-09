//! Prints the text of each PDF given, as the engine extracts it:
//!
//! ```text
//! cargo run --example extract_text -- [--password PASSWORD] [--out FOLDER] report.pdf ...
//! ```
//!
//! Pages are separated by a form feed. With `--out`, the text of each file goes
//! to `FOLDER/<path with / as __>.txt` instead, for comparisons. A file that
//! can't be opened prints `error: ...`. Encrypted files are unlocked with the
//! password, if given.

use std::ffi::OsString;
use std::path::PathBuf;

use ozaru_engine::text::document_text;
use ozaru_engine::{Document, OpenOptions};

fn main() {
    // Passwords may not be UTF-8, like the Latin-1 ones of revision 4.
    let mut arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut option = |name: &str| match arguments.iter().position(|a| *a == *name) {
        Some(index) if index + 1 < arguments.len() => {
            let value = arguments.remove(index + 1);
            arguments.remove(index);
            Some(value)
        }
        _ => None,
    };
    let password = option("--password").map(|p| p.as_encoded_bytes().to_vec());
    let out = option("--out").map(PathBuf::from);
    for path in arguments {
        let path = path.to_string_lossy().into_owned();
        let text = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|file| extract(&file, password.as_deref()));
        match (&out, text) {
            (Some(folder), Ok(text)) => {
                let name = format!("{}.txt", path.replace('/', "__"));
                std::fs::write(folder.join(name), text).unwrap();
            }
            (None, Ok(text)) => println!("{text}"),
            (_, Err(error)) => eprintln!("{path}\terror: {error}"),
        }
    }
}

fn extract(file: &[u8], password: Option<&[u8]>) -> Result<String, String> {
    // Files whose page tree is encrypted only open with the password.
    let mut document = Document::open(file)
        .or_else(|error| match password {
            Some(password) => Document::open_with(
                file,
                OpenOptions {
                    password: Some(password),
                    ..OpenOptions::default()
                },
            ),
            None => Err(error),
        })
        .map_err(|e| e.to_string())?;
    if document.needs_password() {
        let unlocked = password.is_some_and(|password| document.unlock(password));
        if !unlocked {
            return Err("the file needs a password".to_string());
        }
    }
    let pages: Vec<String> = document_text(&mut document)
        .into_iter()
        .map(|page| page.text)
        .collect();
    Ok(pages.join("\u{c}"))
}
