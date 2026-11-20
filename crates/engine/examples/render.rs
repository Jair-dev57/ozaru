//! Draws a page of a PDF as a PNG file.
//!
//!     cargo run --release --example render -- file.pdf page out.png [scale]

use ozaru_engine::Document;
use ozaru_engine::render::{RenderOptions, render_page};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a PDF file");
    let page: usize = args
        .next()
        .expect("a page number")
        .parse()
        .expect("a number");
    let out = args.next().expect("an output file");
    let scale: f64 = args.next().map_or(1.0, |s| s.parse().expect("a scale"));
    let data = std::fs::read(path).expect("readable file");
    let mut document = Document::open(&data).expect("a PDF");
    let limits = document.limits();
    let page = document.page(page).expect("a page").clone();
    let options = RenderOptions {
        scale,
        ..RenderOptions::default()
    };
    let start = std::time::Instant::now();
    let pixmap = render_page(document.resolver(), &page, &options, limits).expect("a page");
    eprintln!("{:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    std::fs::write(out, pixmap.to_png([255, 255, 255])).expect("writable file");
}
