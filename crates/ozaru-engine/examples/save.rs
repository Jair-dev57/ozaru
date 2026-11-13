//! Reads a PDF and saves it again, as the engine writes it:
//!
//! ```text
//! cargo run --example save -- [--password PASSWORD] [--rotate DEGREES] in.pdf out.pdf
//! ```
//!
//! With `--rotate`, every page is turned clockwise by that many degrees, a
//! multiple of 90. An encrypted file is saved decrypted.

use ozaru_engine::writer::Editor;
use ozaru_engine::{Document, OpenOptions};

fn main() {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let password = take_option(&mut arguments, "--password");
    let rotate = take_option(&mut arguments, "--rotate").map(|degrees| {
        degrees
            .parse::<i64>()
            .unwrap_or_else(|_| exit("--rotate needs a number of degrees"))
    });
    let [input, output] = &arguments[..] else {
        exit("usage: save [--password PASSWORD] [--rotate DEGREES] in.pdf out.pdf");
    };

    let file = std::fs::read(input).unwrap_or_else(|e| exit(&format!("{input}: {e}")));
    let options = OpenOptions {
        password: password.as_deref().map(str::as_bytes),
        ..OpenOptions::default()
    };
    let mut document =
        Document::open_with(&file, options).unwrap_or_else(|e| exit(&format!("{input}: {e}")));
    let mut editor = Editor::new(&mut document).unwrap_or_else(|e| exit(&format!("{input}: {e}")));
    if let Some(degrees) = rotate {
        for index in 0..editor.page_count() {
            editor
                .rotate_page(index, degrees)
                .unwrap_or_else(|e| exit(&e.to_string()));
        }
    }
    let saved = editor
        .save()
        .unwrap_or_else(|e| exit(&format!("{input}: {e}")));
    std::fs::write(output, saved).unwrap_or_else(|e| exit(&format!("{output}: {e}")));
}

fn take_option(arguments: &mut Vec<String>, name: &str) -> Option<String> {
    let index = arguments.iter().position(|argument| argument == name)?;
    if index + 1 >= arguments.len() {
        exit(&format!("{name} needs a value"));
    }
    let value = arguments.remove(index + 1);
    arguments.remove(index);
    Some(value)
}

fn exit(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
