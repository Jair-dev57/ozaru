//! The rasterizer's strokes: widths, caps, joins, dashes and matrices.

use ozaru_engine::content::Matrix;
use ozaru_engine::render::{
    FillRule, IRect, LineCap, LineJoin, Mask, Path, StrokeStyle, fill_contours, flatten,
    stroke_contours,
};

const PAGE: IRect = IRect {
    x0: 0,
    y0: 0,
    x1: 200,
    y1: 200,
};

fn stroke(path: &Path, style: &StrokeStyle) -> Option<Mask> {
    stroke_with(path, style, &Matrix::IDENTITY)
}

fn stroke_with(path: &Path, style: &StrokeStyle, matrix: &Matrix) -> Option<Mask> {
    let contours = flatten(path, &Matrix::IDENTITY, 0.02).unwrap();
    let outline = stroke_contours(&contours, style, matrix, 0.05).unwrap();
    fill_contours(&outline, FillRule::NonZero, &PAGE)
}

fn area(mask: &Mask) -> f64 {
    mask.data.iter().map(|&c| f64::from(c)).sum::<f64>() / 255.0
}

fn line(from: (f64, f64), to: (f64, f64)) -> Path {
    let mut path = Path::new();
    path.move_to(from.0, from.1);
    path.line_to(to.0, to.1);
    path
}

fn style(width: f64, cap: LineCap, join: LineJoin) -> StrokeStyle {
    StrokeStyle {
        width,
        cap,
        join,
        ..StrokeStyle::default()
    }
}

#[test]
fn a_line_is_a_rectangle_as_wide_as_the_width() {
    let mask = stroke(
        &line((10.0, 50.0), (60.0, 50.0)),
        &style(4.0, LineCap::Butt, LineJoin::Miter),
    )
    .unwrap();
    assert_eq!(mask.rect, IRect::new(10, 48, 60, 52));
    assert!(mask.data.iter().all(|&c| c == 255));
    // A slanted one has its length times its width.
    let mask = stroke(
        &line((20.0, 20.0), (80.0, 100.0)),
        &style(3.0, LineCap::Butt, LineJoin::Miter),
    )
    .unwrap();
    assert!((area(&mask) - 100.0 * 3.0).abs() < 1.0);
}

#[test]
fn caps_add_nothing_half_a_width_or_a_semicircle() {
    let path = line((50.0, 50.0), (100.0, 50.0));
    let butt = area(&stroke(&path, &style(10.0, LineCap::Butt, LineJoin::Miter)).unwrap());
    let square = area(&stroke(&path, &style(10.0, LineCap::Square, LineJoin::Miter)).unwrap());
    let round = area(&stroke(&path, &style(10.0, LineCap::Round, LineJoin::Miter)).unwrap());
    assert!((butt - 500.0).abs() < 0.5);
    assert!((square - 600.0).abs() < 0.5);
    let circle = std::f64::consts::PI * 25.0;
    assert!((round - (500.0 + circle)).abs() < 1.0, "{round}");
}

#[test]
fn joins_fill_the_corner_as_they_should() {
    let mut corner = Path::new();
    corner.move_to(20.0, 20.0);
    corner.line_to(100.0, 20.0);
    corner.line_to(100.0, 100.0);
    // The two lines are 80 long, 10 wide: 1600 for both, and the corner.
    let miter = area(&stroke(&corner, &style(10.0, LineCap::Butt, LineJoin::Miter)).unwrap());
    let bevel = area(&stroke(&corner, &style(10.0, LineCap::Butt, LineJoin::Bevel)).unwrap());
    let round = area(&stroke(&corner, &style(10.0, LineCap::Butt, LineJoin::Round)).unwrap());
    // The lines overlap in a 5 by 5 square, 25 less in all; the outside of the
    // corner is a 5 by 5 square more with a miter, half of it with a bevel,
    // and a quarter of a circle with a round join.
    assert!((miter - 1600.0).abs() < 0.5, "{miter}");
    assert!((bevel - 1587.5).abs() < 0.5, "{bevel}");
    let quarter = std::f64::consts::PI * 25.0 / 4.0;
    assert!((round - (1575.0 + quarter)).abs() < 0.5, "{round}");
}

#[test]
fn a_miter_that_is_too_long_becomes_a_bevel() {
    // A very sharp turn.
    let mut sharp = Path::new();
    sharp.move_to(20.0, 20.0);
    sharp.line_to(120.0, 30.0);
    sharp.line_to(20.0, 40.0);
    let mut limited = style(6.0, LineCap::Butt, LineJoin::Miter);
    limited.miter_limit = 2.0;
    let beveled = stroke(&sharp, &limited).unwrap();
    limited.miter_limit = 100.0;
    let mitered = stroke(&sharp, &limited).unwrap();
    assert!(mitered.rect.x1 > 140);
    assert!(beveled.rect.x1 < 130);
}

#[test]
fn a_closed_path_is_a_ring_and_an_open_one_has_ends() {
    let mut square = Path::new();
    square.rectangle(50.0, 50.0, 60.0, 60.0);
    let ring = stroke(&square, &style(10.0, LineCap::Butt, LineJoin::Miter)).unwrap();
    // 70 by 70 outside, 50 by 50 inside.
    assert!((area(&ring) - (4900.0 - 2500.0)).abs() < 1.0);
    assert_eq!(ring.at(80, 80), 0);
    assert_eq!(ring.at(80, 50), 255);
    // The same lines not closed leave the last corner open.
    let mut open = Path::new();
    open.move_to(50.0, 50.0);
    open.line_to(110.0, 50.0);
    open.line_to(110.0, 110.0);
    open.line_to(50.0, 110.0);
    open.line_to(50.0, 50.0);
    let open_area = area(&stroke(&open, &style(10.0, LineCap::Butt, LineJoin::Miter)).unwrap());
    assert!(open_area < area(&ring) && open_area > area(&ring) - 40.0);
}

#[test]
fn dashes_cut_the_line_into_pieces() {
    let path = line((10.0, 20.0), (110.0, 20.0));
    let mut dashed = style(4.0, LineCap::Butt, LineJoin::Miter);
    dashed.dash = vec![10.0, 5.0];
    let mask = stroke(&path, &dashed).unwrap();
    // 100 long: dashes at 0, 15, 30, 45, 60, 75 and 90 (the last cut short).
    assert!((area(&mask) - (6.0 * 10.0 + 10.0) * 4.0).abs() < 1.0);
    assert_eq!(mask.at(12, 20), 255);
    assert_eq!(mask.at(22, 20), 0);
    assert_eq!(mask.at(27, 20), 255);
    // The phase moves the pattern.
    dashed.dash_phase = 10.0;
    let mask = stroke(&path, &dashed).unwrap();
    assert_eq!(mask.at(12, 20), 0);
    assert_eq!(mask.at(22, 20), 255);
    // An odd number of lengths repeats with dashes and gaps swapped.
    dashed.dash = vec![10.0];
    dashed.dash_phase = 0.0;
    let mask = stroke(&path, &dashed).unwrap();
    assert_eq!(mask.at(15, 20), 255);
    assert_eq!(mask.at(25, 20), 0);
    assert_eq!(mask.at(35, 20), 255);
    // A pattern that can't be used is a solid line.
    dashed.dash = vec![0.0, 0.0];
    assert_eq!(stroke(&path, &dashed).unwrap().at(60, 20), 255);
}

#[test]
fn a_point_is_a_dot_with_round_caps_a_square_with_square_ones() {
    let mut point = Path::new();
    point.move_to(50.0, 50.0);
    point.close();
    let round = stroke(&point, &style(10.0, LineCap::Round, LineJoin::Miter)).unwrap();
    assert!((area(&round) - std::f64::consts::PI * 25.0).abs() < 1.0);
    let square = stroke(&point, &style(10.0, LineCap::Square, LineJoin::Miter)).unwrap();
    assert!((area(&square) - 100.0).abs() < 0.5);
    assert!(stroke(&point, &style(10.0, LineCap::Butt, LineJoin::Miter)).is_none());
    // Dashes of no length are dots too.
    let mut dots = style(6.0, LineCap::Round, LineJoin::Miter);
    dots.dash = vec![0.0, 20.0];
    let mask = stroke(&line((10.0, 100.0), (110.0, 100.0)), &dots).unwrap();
    assert_eq!(mask.at(10, 100), 255);
    assert_eq!(mask.at(30, 100), 255);
    assert_eq!(mask.at(20, 100), 0);
}

#[test]
fn the_width_is_in_user_space_so_the_matrix_stretches_it() {
    // A horizontal line, 2 wide, in a space twice as high as it is wide.
    let matrix = Matrix::new(1.0, 0.0, 0.0, 2.0, 0.0, 0.0);
    let mask = stroke_with(
        &line((10.0, 20.0), (60.0, 20.0)),
        &style(2.0, LineCap::Butt, LineJoin::Miter),
        &matrix,
    );
    // The line isn't stroked in device space: stroke_contours was given the
    // contours as user space, and moves the outline with the matrix.
    let mask = mask.unwrap();
    assert_eq!(mask.rect.y1 - mask.rect.y0, 4);
}

#[test]
fn no_width_or_a_bad_one_paints_nothing() {
    let path = line((10.0, 10.0), (50.0, 50.0));
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(stroke(&path, &style(width, LineCap::Butt, LineJoin::Miter)).is_none());
    }
}

#[test]
fn a_turn_back_on_itself_has_a_round_end_with_a_round_join() {
    let mut there_and_back = Path::new();
    there_and_back.move_to(50.0, 50.0);
    there_and_back.line_to(100.0, 50.0);
    there_and_back.line_to(50.0, 50.0);
    let mask = stroke(
        &there_and_back,
        &style(10.0, LineCap::Butt, LineJoin::Round),
    )
    .unwrap();
    // The half circle goes out past the turning point.
    assert_eq!(mask.at(103, 50), 255);
    assert_eq!(mask.at(106, 50), 0);
}
