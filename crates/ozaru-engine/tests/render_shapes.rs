//! The rasterizer against cairo: the coverage of every pixel of the shapes in
//! `tests/corpus/shapes.tsv`, made by `shapes.py`.

use std::path::{Path as FilePath, PathBuf};

use ozaru_engine::content::Matrix;
use ozaru_engine::render::{
    FillRule, IRect, LineCap, LineJoin, Mask, Path, StrokeStyle, fill_contours, flatten,
    stroke_contours,
};

const SIZE: usize = 200;

fn folder() -> PathBuf {
    FilePath::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

fn path_of(text: &str) -> Path {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let number = |i: usize| tokens[i].parse::<f64>().unwrap();
    let mut path = Path::new();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "M" => {
                path.move_to(number(i + 1), number(i + 2));
                i += 3;
            }
            "L" => {
                path.line_to(number(i + 1), number(i + 2));
                i += 3;
            }
            "C" => {
                path.curve_to(
                    number(i + 1),
                    number(i + 2),
                    number(i + 3),
                    number(i + 4),
                    number(i + 5),
                    number(i + 6),
                );
                i += 7;
            }
            "Z" => {
                path.close();
                i += 1;
            }
            other => panic!("{other}"),
        }
    }
    path
}

fn style_of(text: &str) -> StrokeStyle {
    let mut style = StrokeStyle::default();
    for item in text.split_whitespace() {
        let (key, value) = item.split_once('=').unwrap();
        match key {
            "w" => style.width = value.parse().unwrap(),
            "cap" => {
                style.cap = match value {
                    "round" => LineCap::Round,
                    "square" => LineCap::Square,
                    _ => LineCap::Butt,
                }
            }
            "join" => {
                style.join = match value {
                    "round" => LineJoin::Round,
                    "bevel" => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                }
            }
            "miter" => style.miter_limit = value.parse().unwrap(),
            "dash" if value != "-" => {
                style.dash = value.split(',').map(|d| d.parse().unwrap()).collect();
            }
            "phase" => style.dash_phase = value.parse().unwrap(),
            _ => {}
        }
    }
    style
}

fn render(operation: &str, style: &str, path: &Path) -> Option<Mask> {
    let page = IRect::new(0, 0, SIZE as i32, SIZE as i32);
    let contours = flatten(path, &Matrix::IDENTITY, 0.02).unwrap();
    match operation {
        "stroke" => {
            let outline =
                stroke_contours(&contours, &style_of(style), &Matrix::IDENTITY, 0.05).unwrap();
            fill_contours(&outline, FillRule::NonZero, &page)
        }
        "evenodd" => fill_contours(&contours, FillRule::EvenOdd, &page),
        _ => fill_contours(&contours, FillRule::NonZero, &page),
    }
}

#[test]
fn every_shape_covers_the_pixels_cairo_says() {
    let text = std::fs::read_to_string(folder().join("shapes.tsv")).unwrap();
    let mut count = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        let (name, operation, style, path) = (columns[0], columns[1], columns[2], columns[3]);
        let expected = std::fs::read(folder().join("shapes").join(format!("{name}.raw"))).unwrap();
        let mask = render(operation, style, &path_of(path));
        let (mut largest, mut total) = (0u8, 0u64);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let ours = mask.as_ref().map_or(0, |m| m.at(x as i32, y as i32));
                let d = ours.abs_diff(expected[y * SIZE + x]);
                largest = largest.max(d);
                total += u64::from(d);
            }
        }
        let mean = total as f64 / (SIZE * SIZE) as f64;
        eprintln!("{name}: largest {largest}, mean {mean:.4}");
        assert!(largest <= 80 && mean < 0.12, "{name}: {largest} {mean}");
        count += 1;
    }
    assert!(count >= 20);
}
