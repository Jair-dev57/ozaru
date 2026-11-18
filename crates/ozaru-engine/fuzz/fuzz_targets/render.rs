#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::content::Matrix;
use ozaru_engine::render::{
    Clip, FillRule, IRect, LineCap, LineJoin, Path, Pixmap, StrokeStyle, fill_contours, flatten,
    stroke_contours,
};

// Reads the input as numbers: a matrix, a line style and the segments of a
// path, then fills, strokes and clips with it. It never panics and takes no
// more than the limits of the pixmap and the points allow.
fuzz_target!(|data: &[u8]| {
    let mut numbers = data.chunks_exact(2).map(|c| {
        let v = i16::from_le_bytes([c[0], c[1]]);
        f64::from(v) / 16.0
    });
    let mut next = || numbers.next().unwrap_or(0.0);
    let matrix = Matrix::new(next() / 8.0, next() / 8.0, next() / 8.0, next() / 8.0, next(), next());
    let flags = next().abs() as u32;
    let style = StrokeStyle {
        width: next().abs(),
        cap: [LineCap::Butt, LineCap::Round, LineCap::Square][(flags % 3) as usize],
        join: [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel][((flags / 3) % 3) as usize],
        miter_limit: next().abs(),
        dash: if flags % 2 == 0 {
            vec![next().abs(), next().abs(), next().abs()]
        } else {
            Vec::new()
        },
        dash_phase: next(),
    };
    let mut path = Path::new();
    for _ in 0..64 {
        match next().abs() as u32 % 5 {
            0 => path.move_to(next(), next()),
            1 | 2 => path.line_to(next(), next()),
            3 => path.curve_to(next(), next(), next(), next(), next(), next()),
            _ => path.close(),
        }
    }
    let mut pixmap = Pixmap::new(256, 256, 1 << 20).unwrap();
    pixmap.clear([255, 255, 255]);
    let page = IRect::new(0, 0, 256, 256);
    let mut clip = Clip::new(page);
    if let Some(contours) = flatten(&path, &matrix, 0.1) {
        for rule in [FillRule::NonZero, FillRule::EvenOdd] {
            let mask = fill_contours(&contours, rule, &clip.rect);
            if let Some(mask) = &mask {
                pixmap.fill(mask, &clip, [10, 20, 30], 200);
            }
            clip = clip.intersect(mask.as_ref());
        }
    }
    if let Some(contours) = flatten(&path, &Matrix::IDENTITY, 0.1)
        && let Some(outline) = stroke_contours(&contours, &style, &matrix, 0.1)
        && let Some(mask) = fill_contours(&outline, FillRule::NonZero, &page)
    {
        pixmap.fill(&mask, &Clip::new(page), [0, 0, 0], 255);
    }
});
