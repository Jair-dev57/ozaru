//! Measures the engine on the files of the test corpus, from the lexer up to
//! opening a whole document and running the content of its pages:
//!
//! ```text
//! cargo bench -p ozaru-engine
//! cargo bench -p ozaru-engine -- lexer     # only the measurements named so
//! ```
//!
//! Each measurement runs for about a second after a warm-up, and prints the
//! fastest run: the one least disturbed by the rest of the machine, so the most
//! repeatable. It uses only the standard library, so it adds no
//! dependency. Compare numbers from the same machine only.

use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use ozaru_engine::Document;
use ozaru_engine::base::Limits;
use ozaru_engine::content::Interpreter;
use ozaru_engine::syntax::{Lexer, ObjectId, parse_object};
use ozaru_engine::text::document_text;
use ozaru_engine::xref::Resolver;

fn read(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

// The fastest of many runs of `work`, after a warm-up.
fn measure(mut work: impl FnMut()) -> Duration {
    let warm_up = Instant::now();
    while warm_up.elapsed() < Duration::from_millis(100) {
        work();
    }
    let mut times = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1000) || times.len() < 10 {
        let run = Instant::now();
        work();
        times.push(run.elapsed());
    }
    times.into_iter().min().expect("at least 10 runs")
}

fn report(name: &str, filter: &Option<String>, unit: Option<(&str, f64)>, work: impl FnMut()) {
    if filter.as_ref().is_some_and(|f| !name.contains(f.as_str())) {
        return;
    }
    let time = measure(work);
    let per_unit = unit
        .map(|(unit, count)| format!("  {:>8.3} µs per {unit}", time.as_secs_f64() * 1e6 / count))
        .unwrap_or_default();
    println!("{name:52} {:>10.1} µs{per_unit}", time.as_secs_f64() * 1e6);
}

fn main() {
    // `cargo bench` passes `--bench`; any other argument filters by name.
    let filter = std::env::args().skip(1).find(|a| !a.starts_with("--"));
    let limits = Limits::default();

    let many = read("corpus/many-pages.pdf");
    let tokens = {
        let mut lexer = Lexer::new(&many, limits);
        let mut count = 0;
        while let Ok(Some(_)) = lexer.next_token() {
            count += 1;
        }
        count
    };
    report(
        "lexer: many-pages.pdf",
        &filter,
        Some(("token", tokens as f64)),
        || {
            let mut lexer = Lexer::new(&many, limits);
            while let Ok(Some(token)) = lexer.next_token() {
                black_box(token);
            }
        },
    );

    let page = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595.2756 841.8898] \
        /Resources << /Font << /F1 3 0 R >> /ProcSet [/PDF /Text] >> /Contents 9 0 R >>";
    report("parser: a page dictionary", &filter, None, || {
        black_box(parse_object(page, limits).unwrap());
    });

    for name in [
        "corpus/many-pages.pdf",
        "corpus/balanced-tree-compressed.pdf",
    ] {
        let file = read(name);
        let ids: Vec<ObjectId> = Resolver::open(&file, limits)
            .unwrap()
            .table()
            .iter()
            .map(|(number, _)| ObjectId::new(number, 0))
            .collect();
        let label = format!(
            "resolver: every object of {}",
            name.trim_start_matches("corpus/")
        );
        report(&label, &filter, Some(("object", ids.len() as f64)), || {
            let mut resolver = Resolver::open(&file, limits).unwrap();
            for &id in &ids {
                let _ = black_box(resolver.resolve(id));
            }
        });
    }

    for name in [
        "../../../docs/assets/report.pdf",
        "corpus/many-pages.pdf",
        "corpus/balanced-tree.pdf",
        "corpus/balanced-tree-compressed.pdf",
        "files/report-rc4-128.pdf",
    ] {
        let file = read(name);
        let pages = Document::open(&file).unwrap().page_count();
        let short = name.rsplit('/').next().unwrap();
        report(
            &format!("document: {short}"),
            &filter,
            Some(("page", pages as f64)),
            || {
                black_box(Document::open(&file).unwrap().page_count());
            },
        );
    }

    // Running every page, with the fonts read once, as extracting the text of
    // a document will.
    for name in [
        "../../../docs/assets/report.pdf",
        "corpus/many-pages.pdf",
        "corpus/fonts-reportlab-ttf.pdf",
        "corpus/fonts-cairo-cjk.pdf",
    ] {
        let file = read(name);
        let run = |file: &[u8]| -> usize {
            let mut document = Document::open(file).unwrap();
            let pages = document.pages().to_vec();
            let mut interpreter = Interpreter::new(limits);
            pages
                .iter()
                .map(|page| interpreter.run_page(document.resolver(), page).glyphs.len())
                .sum()
        };
        let glyphs = run(&file);
        let short = name.rsplit('/').next().unwrap();
        report(
            &format!("content: {short}"),
            &filter,
            Some(("glyph", glyphs as f64)),
            || {
                black_box(run(&file));
            },
        );
    }

    // The text of every page, as extract_text gets it.
    for name in [
        "../../../docs/assets/report.pdf",
        "corpus/many-pages.pdf",
        "corpus/fonts-cairo-cjk.pdf",
    ] {
        let file = read(name);
        let pages = Document::open(&file).unwrap().page_count();
        let short = name.rsplit('/').next().unwrap();
        report(
            &format!("text: {short}"),
            &filter,
            Some(("page", pages as f64)),
            || {
                let mut document = Document::open(&file).unwrap();
                black_box(document_text(&mut document));
            },
        );
    }
}
