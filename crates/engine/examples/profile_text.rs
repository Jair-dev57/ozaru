use ozaru_engine::Document;
use ozaru_engine::content::Interpreter;
use ozaru_engine::text::{characters, lines, page_text, read_columns};
use std::time::{Duration, Instant};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let data = std::fs::read(path).unwrap();
    for _ in 0..3 {
        let t = Instant::now();
        let mut document = Document::open(&data).unwrap();
        let open = t.elapsed();
        let limits = document.limits();
        let pages: Vec<_> = document.pages().to_vec();
        let mut interpreter = Interpreter::new(limits);
        let (mut run, mut chars, mut lns, mut cols, mut text) = (
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        );
        let mut n_chars = 0;
        for page in &pages {
            let t = Instant::now();
            let content = interpreter.run_page(document.resolver(), page);
            run += t.elapsed();
            let t = Instant::now();
            let cs = characters(&content);
            chars += t.elapsed();
            n_chars += cs.len();
            let t = Instant::now();
            let ls = lines(cs);
            lns += t.elapsed();
            let t = Instant::now();
            let ls = read_columns(ls);
            cols += t.elapsed();
            let t = Instant::now();
            let _ = ls;
            let s = page_text(&content, &page.crop_box());
            text += t.elapsed();
            let _ = s;
        }
        println!(
            "open {:.1} | interpret {:.1} | characters {:.1} | lines {:.1} | columns {:.1} | page_text(all) {:.1} ms; {} chars",
            open.as_secs_f64() * 1e3,
            run.as_secs_f64() * 1e3,
            chars.as_secs_f64() * 1e3,
            lns.as_secs_f64() * 1e3,
            cols.as_secs_f64() * 1e3,
            text.as_secs_f64() * 1e3,
            n_chars
        );
    }
}
