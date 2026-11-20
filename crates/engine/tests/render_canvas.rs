//! Pixmaps and clips: colors blended through coverage.

use ozaru_engine::content::Matrix;
use ozaru_engine::render::{Clip, FillRule, IRect, Mask, Path, Pixmap, fill_contours, flatten};

fn shape(x: f64, y: f64, w: f64, h: f64, limit: &IRect) -> Mask {
    let mut path = Path::new();
    path.rectangle(x, y, w, h);
    let contours = flatten(&path, &Matrix::IDENTITY, 0.1).unwrap();
    fill_contours(&contours, FillRule::NonZero, limit).unwrap()
}

#[test]
fn a_color_is_blended_by_coverage_and_opacity() {
    let mut pixmap = Pixmap::new(20, 20, 1 << 20).unwrap();
    pixmap.clear([255, 255, 255]);
    let clip = Clip::new(pixmap.rect());
    pixmap.fill(
        &shape(2.0, 2.0, 5.5, 4.0, &pixmap.rect()),
        &clip,
        [255, 0, 0],
        255,
    );
    assert_eq!(pixmap.pixel(3, 3), Some([255, 0, 0, 255]));
    // The half pixel at the right of it is a mix of red and white.
    assert_eq!(pixmap.pixel(7, 3), Some([255, 127, 127, 255]));
    assert_eq!(pixmap.pixel(10, 3), Some([255, 255, 255, 255]));
    // Half opacity.
    pixmap.fill(
        &shape(12.0, 2.0, 4.0, 4.0, &pixmap.rect()),
        &clip,
        [0, 0, 255],
        128,
    );
    assert_eq!(pixmap.pixel(13, 3), Some([127, 127, 255, 255]));
    assert_eq!(pixmap.pixel(30, 3), None);
}

#[test]
fn a_transparent_pixmap_keeps_premultiplied_colors() {
    let mut pixmap = Pixmap::new(4, 4, 1 << 10).unwrap();
    let clip = Clip::new(pixmap.rect());
    pixmap.fill(
        &shape(0.0, 0.0, 4.0, 4.0, &pixmap.rect()),
        &clip,
        [200, 100, 0],
        128,
    );
    // Premultiplied by about half.
    let [r, g, b, a] = pixmap.pixel(1, 1).unwrap();
    assert_eq!((r, g, b, a), (100, 50, 0, 128));
    // As a PNG over white it is the mix.
    let png = pixmap.to_png([255, 255, 255]);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn a_clip_limits_where_colors_go() {
    let mut pixmap = Pixmap::new(20, 20, 1 << 20).unwrap();
    pixmap.clear([255, 255, 255]);
    let page = pixmap.rect();
    let clip = Clip::new(page).intersect(Some(&shape(5.0, 5.0, 6.5, 6.0, &page)));
    assert_eq!(clip.rect, IRect::new(5, 5, 12, 11));
    pixmap.fill(&shape(0.0, 0.0, 20.0, 20.0, &page), &clip, [0, 0, 0], 255);
    assert_eq!(pixmap.pixel(4, 8), Some([255, 255, 255, 255]));
    assert_eq!(pixmap.pixel(6, 8), Some([0, 0, 0, 255]));
    // The edge of the clip is anti-aliased too.
    assert_eq!(pixmap.pixel(11, 8), Some([127, 127, 127, 255]));
    assert_eq!(pixmap.pixel(12, 8), Some([255, 255, 255, 255]));
    // Clips cut by clips keep what both have: half of half.
    let both = clip.intersect(Some(&shape(5.0, 5.0, 6.5, 6.5, &page)));
    assert_eq!(both.at(11, 10), 64);
    assert_eq!(both.at(6, 6), 255);
}

#[test]
fn a_clip_that_covers_nothing_clips_everything() {
    let mut pixmap = Pixmap::new(10, 10, 1 << 20).unwrap();
    pixmap.clear([255, 255, 255]);
    let page = pixmap.rect();
    let nothing = Clip::new(page).intersect(None);
    pixmap.fill(
        &shape(0.0, 0.0, 10.0, 10.0, &page),
        &nothing,
        [0, 0, 0],
        255,
    );
    assert_eq!(pixmap.pixel(5, 5), Some([255, 255, 255, 255]));
    // A shape away from the clip, too.
    let away = Clip::new(page).intersect(Some(&shape(0.0, 0.0, 3.0, 3.0, &page)));
    let other = Clip::new(IRect::new(5, 5, 10, 10));
    assert!(
        away.intersect(Some(&shape(5.0, 5.0, 4.0, 4.0, &page)))
            .rect
            .is_empty()
    );
    assert!(other.at(2, 2) == 0 && other.at(6, 6) == 255);
}

#[test]
fn a_pixmap_larger_than_the_limit_isn_t_made() {
    assert!(Pixmap::new(1000, 1000, 1 << 20).is_none());
    assert!(Pixmap::new(usize::MAX, 2, usize::MAX).is_none());
    assert!(Pixmap::new(1000, 1000, 4_000_000).is_some());
}
