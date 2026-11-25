//! Prints the page count of each PDF given, or why it can't be read:
//!
//! ```text
//! cargo run --example page_count -- [--password PASSWORD] report.pdf ...
//! ```
//!
//! Each line is the path, a tab, and the count or `error: ...`. Encrypted files
//! are counted locked, as their pages can usually be counted without the
//! password. If that fails and a password is given, they are opened with it.

use ozaru_engine::{Document, OpenOptions};

fn main() {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let password = match arguments.iter().position(|a| a == "--password") {
        Some(index) if index + 1 < arguments.len() => {
            let password = arguments.remove(index + 1);
            arguments.remove(index);
            Some(password)
        }
        _ => None,
    };
    for path in arguments {
        let result = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|file| count(&file, password.as_deref()));
        match result {
            Ok(count) => println!("{path}\t{count}"),
            Err(error) => println!("{path}\terror: {error}"),
        }
    }
}

fn count(file: &[u8], password: Option<&str>) -> Result<usize, String> {
    let locked = Document::open(file).map(|document| document.page_count());
    match (locked, password) {
        (Err(_), Some(password)) => {
            let options = OpenOptions {
                password: Some(password.as_bytes()),
                ..OpenOptions::default()
            };
            Document::open_with(file, options)
                .map(|document| document.page_count())
                .map_err(|e| e.to_string())
        }
        (result, _) => result.map_err(|e| e.to_string()),
    }
}
