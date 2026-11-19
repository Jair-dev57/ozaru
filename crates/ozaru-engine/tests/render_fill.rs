//! The rasterizer's fills: the coverage of every pixel is the area of it
//! inside the shape.

use ozaru_engine::content::Matrix;
use ozaru_engine::render::{FillRule, IRect, Mask, Path, fill_contours, flatten};

const PAGE: IRect = IRect {
    x0: 0,
    y0: 0,
    x1: 100,
    y1: 100,
};

fn fill(path: &Path, rule: FillRule) -> Option<Mask> {
    fill_with(path, &Matrix::IDENTITY, rule, &PAGE)
}

fn fill_with(path: &Path, matrix: &Matrix, rule: FillRule, limit: &IRect) -> Option<Mask> {
    let contours = flatten(path, matrix, 0.05).unwrap();
    fill_contours(&contours, rule, limit)
}

// The area the mask covers, in pixels.
fn area(mask: &Mask) -> f64 {
    mask.data.iter().map(|&c| f64::from(c)).sum::<f64>() / 255.0
}

#[test]
fn a_rectangle_on_pixel_edges_covers_its_pixels_fully() {
    let mut path = Path::new();
    path.rectangle(10.0, 20.0, 5.0, 3.0);
    let mask = fill(&path, FillRule::NonZero).unwrap();
    assert_eq!(mask.rect, IRect::new(10, 20, 15, 23));
    assert!(mask.data.iter().all(|&c| c == 255));
}

#[test]
fn the_edges_of_a_rectangle_cover_the_part_of_the_pixel_inside_it() {
    let mut path = Path::new();
    path.rectangle(10.25, 20.5, 4.5, 2.0);
    let mask = fill(&path, FillRule::NonZero).unwrap();
    // Columns 10 and 14 are three quarters covered, rows 20 and 22 half.
    assert_eq!(mask.at(10, 21), 191);
    assert_eq!(mask.at(14, 21), 191);
    assert_eq!(mask.at(12, 20), 128);
    assert_eq!(mask.at(12, 22), 128);
    assert_eq!(mask.at(10, 20), 96);
    assert_eq!(mask.at(12, 21), 255);
    assert!((area(&mask) - 9.0).abs() < 0.05);
}

#[test]
fn triangles_and_circles_have_the_area_they_should() {
    let mut triangle = Path::new();
    triangle.move_to(10.3, 10.1);
    triangle.line_to(60.7, 20.9);
    triangle.line_to(30.2, 70.4);
    triangle.close();
    let mask = fill(&triangle, FillRule::NonZero).unwrap();
    let expected = 0.5 * ((60.7 - 10.3) * (70.4 - 10.1) - (30.2 - 10.3) * (20.9 - 10.1));
    assert!(
        (area(&mask) - expected).abs() < 0.5,
        "{} {expected}",
        area(&mask)
    );

    // A circle of four curves, which is a bit more than 0.01% off its area.
    let (cx, cy, r) = (50.3, 49.6, 30.0);
    let k = 0.552_284_749_8 * r;
    let mut circle = Path::new();
    circle.move_to(cx + r, cy);
    circle.curve_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    circle.curve_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    circle.curve_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    circle.curve_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    circle.close();
    let mask = fill(&circle, FillRule::NonZero).unwrap();
    let expected = std::f64::consts::PI * r * r;
    assert!((area(&mask) - expected).abs() / expected < 0.003);
    // The center is covered and the corner of its box isn't.
    assert_eq!(mask.at(50, 50), 255);
    assert_eq!(mask.at(21, 20), 0);
}

#[test]
fn the_fill_rules_differ_where_shapes_overlap_in_the_same_direction() {
    let mut path = Path::new();
    path.rectangle(10.0, 10.0, 30.0, 30.0);
    path.rectangle(20.0, 20.0, 30.0, 30.0);
    let nonzero = fill(&path, FillRule::NonZero).unwrap();
    let even_odd = fill(&path, FillRule::EvenOdd).unwrap();
    // The overlap, 20 to 40, is inside by one rule and a hole by the other.
    assert_eq!(nonzero.at(30, 30), 255);
    assert_eq!(even_odd.at(30, 30), 0);
    assert_eq!(nonzero.at(15, 15), 255);
    assert_eq!(even_odd.at(15, 15), 255);
    assert!((area(&nonzero) - 1400.0).abs() < 0.5);
    assert!((area(&even_odd) - 1000.0).abs() < 0.5);
}

#[test]
fn a_shape_wound_the_other_way_inside_another_is_a_hole_with_both_rules() {
    let mut path = Path::new();
    path.rectangle(10.0, 10.0, 40.0, 40.0);
    // Counterclockwise.
    path.move_to(20.0, 20.0);
    path.line_to(20.0, 40.0);
    path.line_to(40.0, 40.0);
    path.line_to(40.0, 20.0);
    path.close();
    for rule in [FillRule::NonZero, FillRule::EvenOdd] {
        let mask = fill(&path, rule).unwrap();
        assert_eq!(mask.at(30, 30), 0, "{rule:?}");
        assert_eq!(mask.at(12, 12), 255, "{rule:?}");
    }
}

#[test]
fn shapes_beyond_the_limit_are_cut_to_it() {
    let mut path = Path::new();
    path.rectangle(-50.0, -50.0, 1000.0, 1000.0);
    let mask = fill(&path, FillRule::NonZero).unwrap();
    assert_eq!(mask.rect, PAGE);
    assert!(mask.data.iter().all(|&c| c == 255));
    // A triangle with a corner far away still fills what is inside.
    let mut wedge = Path::new();
    wedge.move_to(50.0, 50.0);
    wedge.line_to(5000.0, 50.0);
    wedge.line_to(5000.0, 5000.0);
    wedge.close();
    let mask = fill(&wedge, FillRule::NonZero).unwrap();
    assert_eq!(mask.at(99, 80), 255);
    assert_eq!(mask.at(60, 55), 255);
    assert_eq!(mask.at(60, 80), 0);
    // On the diagonal, half.
    assert_eq!(mask.at(99, 99), 128);
    // Nothing inside the limit, no mask.
    let mut away = Path::new();
    away.rectangle(200.0, 200.0, 10.0, 10.0);
    assert!(fill(&away, FillRule::NonZero).is_none());
    // An empty path.
    assert!(fill(&Path::new(), FillRule::NonZero).is_none());
}

#[test]
fn the_matrix_moves_and_scales_the_path() {
    let mut path = Path::new();
    path.rectangle(0.0, 0.0, 1.0, 1.0);
    let matrix = Matrix::new(10.0, 0.0, 0.0, 20.0, 5.0, 6.0);
    let mask = fill_with(&path, &matrix, FillRule::NonZero, &PAGE).unwrap();
    assert_eq!(mask.rect, IRect::new(5, 6, 15, 26));
    assert!((area(&mask) - 200.0).abs() < 0.01);
}

#[test]
fn a_path_that_is_not_finite_or_is_too_large_is_refused() {
    let mut path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(f64::NAN, 5.0);
    path.line_to(5.0, 5.0);
    assert!(flatten(&path, &Matrix::IDENTITY, 0.1).is_none());
    // Curves are cut into few lines when they are flat and into many when
    // they are not.
    let mut flat = Path::new();
    flat.move_to(0.0, 0.0);
    flat.curve_to(1.0, 0.0, 2.0, 0.0, 3.0, 0.0);
    let mut bent = Path::new();
    bent.move_to(0.0, 0.0);
    bent.curve_to(0.0, 100.0, 100.0, 100.0, 100.0, 0.0);
    let lines = |path: &Path| {
        flatten(path, &Matrix::IDENTITY, 0.1).unwrap()[0]
            .points
            .len()
    };
    assert!(lines(&flat) <= 2);
    assert!(lines(&bent) > 20 && lines(&bent) < 513);
}
