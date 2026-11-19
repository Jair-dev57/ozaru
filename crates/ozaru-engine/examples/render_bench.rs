//! Times the rasterizer on a page-sized workload: big shapes, many small ones
//! and long strokes.
//!
//!     cargo run --release --example render_bench

use std::time::Instant;

use ozaru_engine::content::Matrix;
use ozaru_engine::render::{
    Clip, FillRule, LineCap, LineJoin, Path, Pixmap, StrokeStyle, fill_contours, flatten,
    stroke_contours,
};

fn main() {
    let mut pixmap = Pixmap::new(1240, 1754, 1 << 28).expect("a pixmap");
    pixmap.clear([255, 255, 255]);
    let clip = Clip::new(pixmap.rect());
    let page = pixmap.rect();

    let start = Instant::now();
    let mut circle = Path::new();
    let (cx, cy, r) = (620.0, 877.0, 600.0);
    let k = 0.552_284_749_8 * r;
    circle.move_to(cx + r, cy);
    circle.curve_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    circle.curve_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    circle.curve_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    circle.curve_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    circle.close();
    let contours = flatten(&circle, &Matrix::IDENTITY, 0.1).unwrap();
    let mask = fill_contours(&contours, FillRule::NonZero, &page).unwrap();
    pixmap.fill(&mask, &clip, [200, 30, 30], 255);
    println!("big circle: {:.2} ms", start.elapsed().as_secs_f64() * 1e3);

    let start = Instant::now();
    for i in 0..5000 {
        let mut path = Path::new();
        path.rectangle(
            f64::from(i % 70) * 17.3,
            f64::from(i / 70) * 23.1,
            12.5,
            9.5,
        );
        let contours = flatten(&path, &Matrix::IDENTITY, 0.1).unwrap();
        if let Some(mask) = fill_contours(&contours, FillRule::NonZero, &page) {
            pixmap.fill(&mask, &clip, [30, 30, 200], 255);
        }
    }
    println!(
        "5000 small rectangles: {:.2} ms",
        start.elapsed().as_secs_f64() * 1e3
    );

    let start = Instant::now();
    let mut wave = Path::new();
    wave.move_to(0.0, 100.0);
    for i in 0..2000 {
        let x = f64::from(i) * 0.6;
        wave.line_to(x, 900.0 + 300.0 * (f64::from(i) * 0.05).sin());
    }
    let style = StrokeStyle {
        width: 3.0,
        cap: LineCap::Round,
        join: LineJoin::Round,
        dash: vec![9.0, 4.0],
        ..StrokeStyle::default()
    };
    let contours = flatten(&wave, &Matrix::IDENTITY, 0.1).unwrap();
    let outline = stroke_contours(&contours, &style, &Matrix::IDENTITY, 0.1).unwrap();
    let mask = fill_contours(&outline, FillRule::NonZero, &page).unwrap();
    pixmap.fill(&mask, &clip, [0, 0, 0], 255);
    println!(
        "dashed stroke of 2000 lines: {:.2} ms",
        start.elapsed().as_secs_f64() * 1e3
    );
}
