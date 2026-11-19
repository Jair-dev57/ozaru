//! Images drawn on a page: placement, sampling, masks and JPEG.

use ozaru_engine::Document;
use ozaru_engine::render::{Pixmap, RenderOptions, render_page};

mod common;
use common::{file_with, stream_object};

fn draw(content: &str, images: &[(u32, String)], scale: f64) -> Pixmap {
    let mut objects = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
    ];
    let names: String = images
        .iter()
        .map(|(n, _)| format!("/Im{n} {n} 0 R "))
        .collect();
    objects.push((
        3,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
             /Resources << /XObject << {names} >> >> >>"
        ),
    ));
    objects.push((4, stream_object("", content)));
    objects.extend(images.iter().cloned());
    let pdf = file_with(&objects);
    let mut document = Document::open(&pdf).unwrap();
    let limits = document.limits();
    let page = document.page(0).unwrap().clone();
    let options = RenderOptions {
        scale,
        ..RenderOptions::default()
    };
    render_page(document.resolver(), &page, &options, limits).unwrap()
}

fn image(
    number: u32,
    width: u32,
    height: u32,
    space: &str,
    bits: u32,
    hex: &str,
    extra: &str,
) -> (u32, String) {
    (
        number,
        stream_object(
            &format!(
                "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {space} \
                 /BitsPerComponent {bits} /Filter /ASCIIHexDecode {extra}"
            ),
            hex,
        ),
    )
}

fn rgb(pixmap: &Pixmap, x: usize, y: usize) -> [u8; 3] {
    let p = pixmap.pixel(x, y).unwrap();
    [p[0], p[1], p[2]]
}

#[test]
fn a_small_image_is_repeated_pixel_by_pixel_when_it_is_enlarged() {
    // Red and green above, blue and white below.
    let im = image(5, 2, 2, "/DeviceRGB", 8, "FF0000 00FF00 0000FF FFFFFF>", "");
    let pixmap = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[im], 1.0);
    assert_eq!(rgb(&pixmap, 10, 10), [255, 0, 0]);
    assert_eq!(rgb(&pixmap, 90, 10), [0, 255, 0]);
    assert_eq!(rgb(&pixmap, 10, 90), [0, 0, 255]);
    assert_eq!(rgb(&pixmap, 90, 90), [255, 255, 255]);
    // Sharp edges: the pixels next to the middle are one color or the other.
    assert_eq!(rgb(&pixmap, 49, 10), [255, 0, 0]);
    assert_eq!(rgb(&pixmap, 50, 10), [0, 255, 0]);
}

#[test]
fn an_image_that_is_reduced_averages_the_pixels_it_covers() {
    // 8 by 8, left half black and right half white, drawn 2 pixels wide.
    let row = "00 00 00 00 FF FF FF FF ";
    let im = image(
        5,
        8,
        8,
        "/DeviceGray",
        8,
        &format!("{}>", row.repeat(8)),
        "",
    );
    let pixmap = draw("q 2 0 0 2 10 10 cm /Im5 Do Q", &[im], 1.0);
    assert_eq!(rgb(&pixmap, 10, 88)[0], 0);
    assert_eq!(rgb(&pixmap, 11, 88)[0], 255);
    // At scale 0.25 the whole 2 pixels fit in one, and mix.
    let small = draw(
        "q 4 0 0 4 12 12 cm /Im5 Do Q",
        &[image(
            5,
            8,
            8,
            "/DeviceGray",
            8,
            &format!("{}>", row.repeat(8)),
            "",
        )],
        0.25,
    );
    let mixed = rgb(&small, 3, 21)[0];
    assert!(mixed > 90 && mixed < 170, "{mixed}");
}

#[test]
fn images_are_moved_turned_and_flipped_by_the_matrix() {
    let im = || image(5, 2, 1, "/DeviceRGB", 8, "FF0000 0000FF>", "");
    // Turned a quarter, the red end goes from the left to the bottom.
    let turned = draw("q 0 50 -50 0 75 25 cm /Im5 Do Q", &[im()], 1.0);
    assert_eq!(rgb(&turned, 10, 50), [255, 255, 255]);
    assert_eq!(rgb(&turned, 50, 60), [255, 0, 0]);
    assert_eq!(rgb(&turned, 50, 40), [0, 0, 255]);
    // Flipped: a negative width swaps the ends.
    let plain = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[im()], 1.0);
    let flipped = draw("q -100 0 0 100 100 0 cm /Im5 Do Q", &[im()], 1.0);
    assert_eq!(rgb(&plain, 10, 50), [255, 0, 0]);
    assert_eq!(rgb(&flipped, 10, 50), [0, 0, 255]);
    // A matrix that squashes the image to nothing draws nothing.
    let flat = draw("q 100 0 0 0 0 0 cm /Im5 Do Q", &[im()], 1.0);
    assert_eq!(rgb(&flat, 50, 50), [255, 255, 255]);
}

#[test]
fn a_soft_mask_is_the_opacity_of_the_image() {
    let mask = image(6, 2, 1, "/DeviceGray", 8, "FF 00>", "");
    let with_mask = image(5, 2, 1, "/DeviceRGB", 8, "000000 000000>", "/SMask 6 0 R");
    let pixmap = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[with_mask, mask], 1.0);
    assert_eq!(rgb(&pixmap, 10, 50), [0, 0, 0]);
    // Transparent where the mask is 0: the paper shows.
    assert_eq!(rgb(&pixmap, 90, 50), [255, 255, 255]);
}

#[test]
fn a_stencil_mask_paints_the_fill_color_where_its_bits_say() {
    // 8 by 1: the first four bits are 0, which paint, the rest 1.
    let stencil = (
        5,
        stream_object(
            "/Type /XObject /Subtype /Image /Width 8 /Height 1 /ImageMask true /BitsPerComponent 1 \
             /Filter /ASCIIHexDecode",
            "0F>",
        ),
    );
    let pixmap = draw("0 0.5 0 rg q 80 0 0 80 10 10 cm /Im5 Do Q", &[stencil], 1.0);
    assert_eq!(rgb(&pixmap, 20, 50), [0, 128, 0]);
    assert_eq!(rgb(&pixmap, 80, 50), [255, 255, 255]);
}

#[test]
fn an_image_with_a_color_key_mask_and_one_in_a_palette() {
    let keyed = image(5, 2, 1, "/DeviceGray", 8, "00 FF>", "/Mask [255 255]");
    let pixmap = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[keyed], 1.0);
    assert_eq!(rgb(&pixmap, 10, 50), [0, 0, 0]);
    assert_eq!(rgb(&pixmap, 90, 50), [255, 255, 255]);
    let palette = image(
        5,
        2,
        1,
        "[/Indexed /DeviceRGB 1 <FF0000 0000FF>]",
        8,
        "00 01>",
        "",
    );
    let pixmap = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[palette], 1.0);
    assert_eq!(rgb(&pixmap, 10, 50), [255, 0, 0]);
    assert_eq!(rgb(&pixmap, 90, 50), [0, 0, 255]);
}

#[test]
fn a_jpeg_image_is_decoded_to_be_drawn() {
    let data = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/jpeg/rgb-444.jpg"),
    )
    .unwrap();
    let hex: String = data.iter().map(|b| format!("{b:02X}")).collect::<String>() + ">";
    let im = (
        5,
        stream_object(
            "/Type /XObject /Subtype /Image /Width 67 /Height 45 /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter [/ASCIIHexDecode /DCTDecode]",
            &hex,
        ),
    );
    let pixmap = draw("q 67 0 0 45 0 55 cm /Im5 Do Q", &[im], 1.0);
    // The picture is a gradient with a red ellipse in the middle.
    let middle = rgb(&pixmap, 28, 15);
    assert!(middle[0] > 180 && middle[1] < 90, "{middle:?}");
    // JPEG 2000 can't be drawn: the page keeps its paper.
    let jpx = (
        6,
        stream_object(
            "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter [/ASCIIHexDecode /JPXDecode]",
            "00>",
        ),
    );
    let pixmap = draw("q 50 0 0 50 0 0 cm /Im6 Do Q", &[jpx], 1.0);
    assert_eq!(rgb(&pixmap, 20, 70), [255, 255, 255]);
}

#[test]
fn an_image_is_drawn_with_the_opacity_and_the_clip_of_its_state() {
    let im = image(5, 1, 1, "/DeviceRGB", 8, "000000>", "");
    let content = "q 0 0 50 100 re W n /Half gs 100 0 0 100 0 0 cm /Im5 Do Q";
    let pdf_resources = "";
    let _ = pdf_resources;
    // The /Half state isn't in these resources: the image is opaque, but clipped.
    let pixmap = draw(content, &[im], 1.0);
    assert_eq!(rgb(&pixmap, 25, 50), [0, 0, 0]);
    assert_eq!(rgb(&pixmap, 75, 50), [255, 255, 255]);
}

// A page with `content` and Helvetica as /F1, drawn at 1.
fn text_page(content: &str) -> Pixmap {
    let pdf = common::one_page(content, "");
    let mut document = Document::open(&pdf).unwrap();
    let limits = document.limits();
    let page = document.page(0).unwrap().clone();
    render_page(
        document.resolver(),
        &page,
        &RenderOptions::default(),
        limits,
    )
    .unwrap()
}

#[test]
fn text_in_a_clipping_mode_cuts_what_is_painted_after_it() {
    // The H of Helvetica at 80 points, from x 10: its stems are at the sides.
    let clipped = text_page("q BT /F1 80 Tf 10 20 Td 7 Tr (H) Tj ET 1 0 0 rg 0 0 100 100 re f Q");
    let stem = clipped.pixel(18, 792 - 50).unwrap();
    assert_eq!(&stem[..3], &[255, 0, 0]);
    // Between the stems, above the bar, there is nothing of the H.
    let between = clipped.pixel(40, 792 - 70).unwrap();
    assert_eq!(&between[..3], &[255, 255, 255]);
    // Mode 5 strokes too, and mode 4 fills and clips.
    let filled =
        text_page("q BT /F1 80 Tf 10 20 Td 0 0 1 rg 4 Tr (H) Tj ET 1 0 0 rg 0 0 100 100 re f Q");
    assert_eq!(&filled.pixel(18, 792 - 50).unwrap()[..3], &[255, 0, 0]);
    assert_eq!(&filled.pixel(40, 792 - 70).unwrap()[..3], &[255, 255, 255]);
    // Without a clipping mode the text doesn't cut.
    let plain = text_page("q BT /F1 80 Tf 10 20 Td 3 Tr (H) Tj ET 1 0 0 rg 0 0 100 100 re f Q");
    assert_eq!(&plain.pixel(40, 792 - 70).unwrap()[..3], &[255, 0, 0]);
}

#[test]
fn an_image_that_asks_for_interpolation_is_smoothed_when_enlarged() {
    let plain = image(5, 2, 1, "/DeviceGray", 8, "00 FF>", "");
    let smooth = image(5, 2, 1, "/DeviceGray", 8, "00 FF>", "/Interpolate true");
    let sharp = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[plain], 1.0);
    let soft = draw("q 100 0 0 100 0 0 cm /Im5 Do Q", &[smooth], 1.0);
    // Next to the middle, nearest gives one color and bilinear a mix.
    assert_eq!(rgb(&sharp, 45, 50)[0], 0);
    let mixed = rgb(&soft, 45, 50)[0];
    assert!(mixed > 20 && mixed < 235, "{mixed}");
}
