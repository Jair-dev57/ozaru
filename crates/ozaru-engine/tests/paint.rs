//! What the interpreter keeps for drawing: paths, colors, lines, clips and the
//! order of everything painted.

use ozaru_engine::Document;
use ozaru_engine::base::Limits;
use ozaru_engine::content::{Interpreter, PageContent, PaintItem};
use ozaru_engine::render::{FillRule, LineCap, LineJoin, Segment};

mod common;
use common::{file_with, stream_object};

// A page with `content`, a Separation and an Indexed color space, an
// ExtGState and a Type 3 font.
fn run(content: &str) -> PageContent {
    let pdf = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << \
             /ColorSpace << /Ink [/Separation /Gold /DeviceCMYK 6 0 R] \
             /Pal [/Indexed /DeviceRGB 1 <FF0000 0000FF>] /Gray /DeviceGray >> \
             /ExtGState << /Half << /ca 0.5 /CA 0.25 /LW 3 /LC 1 /LJ 2 /ML 4 /D [[3 1] 2] >> >> \
             /Font << /T3 5 0 R >> >> >>"
                .to_string(),
        ),
        (4, stream_object("", content)),
        (
            5,
            "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] \
             /CharProcs << /a 7 0 R /b 8 0 R >> /Encoding << /Type /Encoding /Differences [97 /a /b] >> \
             /FirstChar 97 /LastChar 98 /Widths [1000 1000] /Resources << >> >>"
                .to_string(),
        ),
        (
            6,
            "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 0.2 0.8 0] /N 1 >>".to_string(),
        ),
        (7, stream_object("", "1000 0 d0 1 0 0 rg 0 0 500 500 re f")),
        (8, stream_object("", "1000 0 0 0 500 500 d1 0 1 0 rg 0 0 500 500 re f")),
    ]);
    let mut document = Document::open(&pdf).unwrap();
    let page = document.page(0).unwrap().clone();
    Interpreter::painting(Limits::default()).run_page(document.resolver(), &page)
}

#[test]
fn paths_are_kept_with_the_rule_and_the_way_they_are_painted() {
    let content = run(
        "0 0 m 10 0 l 10 10 l h f  20 20 5 5 re f*  0 0 m 30 30 l S  \
                       1 1 m 5 5 l b  2 2 m 6 6 l B*  7 7 m 8 8 l s  9 9 m n",
    );
    let kinds: Vec<(Option<FillRule>, bool)> =
        content.paths.iter().map(|p| (p.fill, p.stroke)).collect();
    assert_eq!(
        kinds,
        [
            (Some(FillRule::NonZero), false),
            (Some(FillRule::EvenOdd), false),
            (None, true),
            (Some(FillRule::NonZero), true),
            (Some(FillRule::EvenOdd), true),
            (None, true),
        ]
    );
    assert_eq!(content.order.len(), 6);
    assert!(matches!(content.order[0], PaintItem::Path(0)));
    // The first path: a triangle, closed.
    assert_eq!(content.paths[0].path.segments.last(), Some(&Segment::Close));
    assert_eq!(content.paths[1].path.segments.len(), 5);
}

#[test]
fn curves_of_the_three_kinds_and_a_line_without_a_current_point() {
    let content = run(
        "10 10 m 20 20 30 30 40 40 c 50 50 60 60 v 70 70 80 80 y S  5 5 l 9 9 l S  \
                       1 2 3 4 5 6 c S",
    );
    let first = &content.paths[0].path.segments;
    assert_eq!(first[1], Segment::Curve(20.0, 20.0, 30.0, 30.0, 40.0, 40.0));
    // `v` starts from the current point and `y` ends where its control point is.
    assert_eq!(first[2], Segment::Curve(40.0, 40.0, 50.0, 50.0, 60.0, 60.0));
    assert_eq!(first[3], Segment::Curve(70.0, 70.0, 80.0, 80.0, 80.0, 80.0));
    // Lines and curves without a `m` start a subpath.
    assert_eq!(content.paths[1].path.segments[0], Segment::Move(5.0, 5.0));
    assert_eq!(content.paths[2].path.segments[0], Segment::Move(1.0, 2.0));
}

#[test]
fn colors_of_every_kind_are_turned_into_rgb() {
    let content = run(
        "0.5 g 0 0 1 1 re f  1 0 0 rg 0 0 1 1 re f  0 0 0 0.5 k 0 0 1 1 re f \
         /Ink cs 1 scn 0 0 1 1 re f  /Pal cs 1 sc 0 0 1 1 re f  /Gray cs 0.2 sc 0 0 1 1 re f \
         0 0 1 RG 0 0 1 1 re S  /Pal CS 0 SC 0 0 1 1 re S  0.5 G  0 0 1 1 re S  1 0 0 0 K 0 0 1 1 re S \
         /Ink CS 0.5 SCN 0 0 1 1 re S  /Missing cs  0 0 1 1 re f",
    );
    let fills: Vec<[u8; 3]> = content
        .paths
        .iter()
        .filter(|p| p.fill.is_some())
        .map(|p| p.paint.fill_color)
        .collect();
    assert_eq!(fills[0], [128, 128, 128]);
    assert_eq!(fills[1], [255, 0, 0]);
    // CMYK with only black at 0.5.
    assert_eq!(fills[2], [128, 128, 128]);
    // The ink at full tint gives the alternate color: (0, 0.2, 0.8, 0).
    assert_eq!(fills[3], [255, 204, 51]);
    assert_eq!(fills[4], [0, 0, 255]);
    assert_eq!(fills[5], [51, 51, 51]);
    let strokes: Vec<[u8; 3]> = content
        .paths
        .iter()
        .filter(|p| p.stroke)
        .map(|p| p.paint.stroke_color)
        .collect();
    assert_eq!(strokes[0], [0, 0, 255]);
    assert_eq!(strokes[1], [255, 0, 0]);
    assert_eq!(strokes[2], [128, 128, 128]);
    assert_eq!(strokes[3], [0, 255, 255]);
    assert_eq!(strokes[4], [255, 230, 153]);
    // A color space that doesn't exist leaves the color as it was.
    assert_eq!(fills[6], [51, 51, 51]);
}

#[test]
fn colors_without_a_space_are_read_by_their_count() {
    let content = run(
        "0.2 0.4 0.6 sc 0 0 1 1 re f  0.5 sc 0 0 1 1 re f  1 0 0 0 sc 0 0 1 1 re f  sc 0 0 1 1 re f",
    );
    let fills: Vec<[u8; 3]> = content.paths.iter().map(|p| p.paint.fill_color).collect();
    assert_eq!(fills[0], [51, 102, 153]);
    assert_eq!(fills[1], [128, 128, 128]);
    assert_eq!(fills[2], [0, 255, 255]);
    assert_eq!(fills[3], [0, 0, 0]);
}

#[test]
fn line_style_and_opacity_come_from_operators_and_graphics_states() {
    let content = run(
        "5 w 1 J 2 j 3 M [4 2] 1 d 0 0 1 1 re S  q /Half gs 0 0 1 1 re S Q  0 0 1 1 re S  \
                       -1 w 0 J 9 j 0.5 M 0 0 1 1 re S",
    );
    let line = &content.paths[0].paint.line;
    assert_eq!(
        (line.width, line.cap, line.join, line.miter_limit),
        (5.0, LineCap::Round, LineJoin::Bevel, 3.0)
    );
    assert_eq!((line.dash.clone(), line.dash_phase), (vec![4.0, 2.0], 1.0));
    let half = &content.paths[1].paint;
    assert_eq!((half.fill_alpha, half.stroke_alpha), (0.5, 0.25));
    assert_eq!(
        (
            half.line.width,
            half.line.cap,
            half.line.join,
            half.line.miter_limit
        ),
        (3.0, LineCap::Round, LineJoin::Bevel, 4.0)
    );
    assert_eq!(
        (half.line.dash.clone(), half.line.dash_phase),
        (vec![3.0, 1.0], 2.0)
    );
    // Q brings back what q saved.
    assert_eq!(content.paths[2].paint.stroke_alpha, 1.0);
    // Bad values are ignored or clamped.
    let last = &content.paths[3].paint.line;
    assert_eq!(last.width, 0.0);
    assert_eq!(last.miter_limit, 1.0);
}

#[test]
fn a_clip_takes_effect_after_the_path_that_makes_it() {
    let content =
        run("0 0 50 50 re W n  0 0 10 10 re f  q 20 20 30 30 re W* n 0 0 5 5 re f Q  0 0 1 1 re f");
    assert!(content.paths[0].paint.clip.is_some());
    let inner = content.paths[1].paint.clip.as_ref().unwrap();
    assert_eq!(inner.rule, FillRule::EvenOdd);
    // The clip of the clip is its parent.
    assert!(inner.parent.is_some());
    assert!(
        content.paths[2]
            .paint
            .clip
            .as_ref()
            .unwrap()
            .parent
            .is_none()
    );
    // A path painted with its own clip uses the clip before it.
    let own = run("0 0 5 5 re W f");
    assert!(own.paths[0].paint.clip.is_none());
}

#[test]
fn type_3_glyphs_are_run_as_content_with_their_colors() {
    let content = run("0 0 1 rg BT /T3 10 Tf 20 20 Td (ab) Tj ET");
    // The first glyph has colors of its own, the second takes the text's.
    assert_eq!(content.paths.len(), 2);
    assert_eq!(content.paths[0].paint.fill_color, [255, 0, 0]);
    assert_eq!(content.paths[1].paint.fill_color, [0, 0, 255]);
    // Each is the glyph's, scaled by the font size and moved to its place.
    let m = content.paths[0].matrix;
    assert!((m.a - 0.01).abs() < 1e-9 && (m.e - 20.0).abs() < 1e-9);
    let m = content.paths[1].matrix;
    assert!((m.e - 30.0).abs() < 1e-9);
    // The glyphs come in the order of painting too.
    assert_eq!(content.order.len(), 4);
}

#[test]
fn an_interpreter_that_does_not_paint_keeps_no_paths() {
    let pdf = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 20 20] /Contents 4 0 R >>".to_string(),
        ),
        (4, stream_object("", "1 0 0 rg 0 0 1 1 re f 0 0 m 1 1 l S")),
    ]);
    let mut document = Document::open(&pdf).unwrap();
    let page = document.page(0).unwrap().clone();
    let content = Interpreter::new(Limits::default()).run_page(document.resolver(), &page);
    assert!(content.paths.is_empty() && content.order.is_empty());
}

#[test]
fn blend_modes_soft_masks_and_groups_are_kept() {
    use ozaru_engine::render::BlendMode;
    let pdf = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources << \
             /XObject << /G 5 0 R /P 6 0 R >> /ExtGState << /Mul << /BM /Multiply >> \
             /Arr << /BM [/Nonsense /Screen] >> /Half << /ca 0.5 >> \
             /Lum << /SMask << /S /Luminosity /G 5 0 R /BC [1] >> >> /None << /SMask /None >> >> >> >>"
                .to_string(),
        ),
        (4, stream_object("", "q /Mul gs 0 0 1 1 re f Q q /Arr gs 0 0 1 1 re f Q \
                               q /Lum gs 0 0 1 1 re f /None gs 0 0 1 1 re f Q \
                               q /Half gs /G Do Q  /G Do  q /Mul gs /P Do Q")),
        (
            5,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Group << /S /Transparency >>",
                "1 0 0 rg 0 0 5 5 re f",
            ),
        ),
        (6, stream_object("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 0 5 5 re f")),
    ]);
    let mut document = Document::open(&pdf).unwrap();
    let page = document.page(0).unwrap().clone();
    let content = Interpreter::painting(Limits::default()).run_page(document.resolver(), &page);
    assert_eq!(content.paths[0].paint.blend, BlendMode::Multiply);
    // The first name that is a mode.
    assert_eq!(content.paths[1].paint.blend, BlendMode::Screen);
    let mask = content.paths[2].paint.soft_mask.as_ref().unwrap();
    assert!(mask.luminosity && mask.backdrop == [255, 255, 255]);
    assert!(content.paths[3].paint.soft_mask.is_none());
    // Only the form with a group, painted with opacity, is a group; the one
    // painted plainly and the one without a group aren't.
    let groups = content
        .order
        .iter()
        .filter(|i| matches!(i, PaintItem::BeginGroup(_)))
        .count();
    let ends = content
        .order
        .iter()
        .filter(|i| matches!(i, PaintItem::EndGroup))
        .count();
    assert_eq!((groups, ends, content.groups.len()), (1, 1, 1));
    assert_eq!(content.groups[0].paint.fill_alpha, 0.5);
    // The form of a soft mask runs on its own.
    let own = Interpreter::painting(Limits::default()).run_form(
        document.resolver(),
        mask.form,
        &mask.matrix,
    );
    assert_eq!(own.paths.len(), 1);
}
