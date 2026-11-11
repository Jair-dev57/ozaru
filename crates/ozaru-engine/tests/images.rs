use ozaru_engine::Document;
use ozaru_engine::base::{ErrorKind, Limits};
use ozaru_engine::content::{ImageSource, Interpreter, PageContent};
use ozaru_engine::filters::ImageFilter;
use ozaru_engine::images::{ImageFile, ImageFormat, document_images, image_file, read_image};
use ozaru_engine::syntax::Object;

mod common;
use common::{file_with, stream_object};

// A page whose content is `content`, with /Im1, a 2 by 1 RGB image of 8 bits,
// /Jpeg, an image whose data is JPEG, /Form, a form that paints /Im1, and a
// color space /CS0.
fn file(content: &str) -> Vec<u8> {
    file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /XObject << /Im1 5 0 R /Jpeg 6 0 R /Form 7 0 R >> \
             /ColorSpace << /CS0 [/Indexed /DeviceRGB 1 <FF000000FF00>] >> >> /Contents 4 0 R >>"
                .to_string(),
        ),
        (4, stream_object("", content)),
        (
            5,
            stream_object(
                "/Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceRGB \
                 /BitsPerComponent 8 /Filter /ASCIIHexDecode",
                "FF000000FF00>",
            ),
        ),
        (
            6,
            stream_object(
                "/Type /XObject /Subtype /Image /Width 4 /Height 3 /ColorSpace /DeviceGray \
                 /BitsPerComponent 8 /Filter [/ASCIIHexDecode /DCTDecode] \
                 /DecodeParms [null << /ColorTransform 0 >>]",
                "FFD8FFD9>",
            ),
        ),
        (
            7,
            stream_object(
                "/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Matrix [2 0 0 2 0 0] \
                 /Resources << /XObject << /Im1 5 0 R >> >>",
                "/Im1 Do",
            ),
        ),
    ])
}

fn run(file: &[u8]) -> (Document<'_>, PageContent) {
    let mut document = Document::open(file).unwrap();
    let page = document.page(0).unwrap().clone();
    let content = Interpreter::new(Limits::default()).run_page(document.resolver(), &page);
    (document, content)
}

fn corners(content: &PageContent, index: usize) -> [f64; 4] {
    let b = content.images[index].bounding_box();
    [b.left, b.bottom, b.right, b.top]
}

#[test]
fn an_image_fills_the_unit_square_of_the_current_matrix() {
    let pdf = file("q 200 0 0 100 50 600 cm /Im1 Do Q");
    let (_, content) = run(&pdf);
    assert_eq!(content.images.len(), 1);
    assert_eq!(corners(&content, 0), [50.0, 600.0, 250.0, 700.0]);
    assert!(matches!(
        content.images[0].source,
        ImageSource::XObject { .. }
    ));
    assert!(content.diagnostics.is_empty());
}

#[test]
fn an_image_in_a_form_is_placed_with_the_form_matrix() {
    let pdf = file("q 1 0 0 1 100 100 cm /Form Do Q");
    let (_, content) = run(&pdf);
    assert_eq!(corners(&content, 0), [100.0, 100.0, 102.0, 102.0]);
}

#[test]
fn inline_images_are_placed_too() {
    let pdf = file("q 10 0 0 10 0 0 cm BI /W 1 /H 1 /CS /G /BPC 8 /F /AHx ID 80> EI Q /Im1 Do");
    let (_, content) = run(&pdf);
    assert_eq!(content.images.len(), 2);
    assert_eq!(corners(&content, 0), [0.0, 0.0, 10.0, 10.0]);
    assert!(matches!(
        content.images[0].source,
        ImageSource::Inline { .. }
    ));
}

#[test]
fn an_image_is_described_and_its_data_decoded() {
    let pdf = file("/Im1 Do");
    let (mut document, content) = run(&pdf);
    let image = read_image(
        document.resolver(),
        &content.images[0].source,
        Limits::default(),
    )
    .unwrap();
    let d = &image.description;
    assert_eq!((d.width, d.height, d.bits_per_component), (2, 1, 8));
    assert_eq!(d.color_space, Some(Object::Name("DeviceRGB".into())));
    assert!(!d.image_mask);
    assert_eq!(image.data, [0xFF, 0, 0, 0, 0xFF, 0]);
    assert_eq!(image.encoding, None);
}

#[test]
fn an_image_filter_at_the_end_is_left_with_its_parameters() {
    let pdf = file("/Jpeg Do");
    let (mut document, content) = run(&pdf);
    let image = read_image(
        document.resolver(),
        &content.images[0].source,
        Limits::default(),
    )
    .unwrap();
    let encoding = image.encoding.unwrap();
    assert_eq!(encoding.filter, ImageFilter::Dct);
    assert_eq!(
        encoding.parameters.unwrap().get("ColorTransform"),
        Some(&Object::Integer(0))
    );
    // The data is the JPEG data, with the filters before it decoded.
    assert_eq!(image.data, [0xFF, 0xD8, 0xFF, 0xD9]);
}

#[test]
fn inline_abbreviations_and_named_color_spaces_are_written_out() {
    let pdf = file("BI /W 2 /H 1 /CS /CS0 /BPC 1 /F /AHx ID 40> EI");
    let (mut document, content) = run(&pdf);
    let image = read_image(
        document.resolver(),
        &content.images[0].source,
        Limits::default(),
    )
    .unwrap();
    let d = &image.description;
    assert_eq!((d.width, d.height, d.bits_per_component), (2, 1, 1));
    let Some(Object::Array(space)) = &d.color_space else {
        panic!("{:?}", d.color_space);
    };
    assert_eq!(space[0], Object::Name("Indexed".into()));
    assert_eq!(image.data, [0x40]);
}

#[test]
fn a_stencil_mask_has_one_bit_and_no_color_space() {
    let pdf = file("BI /W 8 /H 1 /IM true /F /AHx ID F0> EI");
    let (mut document, content) = run(&pdf);
    let image = read_image(
        document.resolver(),
        &content.images[0].source,
        Limits::default(),
    )
    .unwrap();
    assert!(image.description.image_mask);
    assert_eq!(image.description.bits_per_component, 1);
    assert_eq!(image.description.color_space, None);
}

#[test]
fn an_image_without_a_usable_size_is_an_error() {
    for dictionary in [
        "/W 0 /H 1 /CS /G /BPC 8",
        "/H 1 /CS /G /BPC 8",
        "/W 1 /H 1 /CS /G /BPC 3",
        "/W 1 /H 1 /BPC 8",
    ] {
        let pdf = file(&format!("BI {dictionary} ID \x00 EI"));
        let (mut document, content) = run(&pdf);
        let error = read_image(
            document.resolver(),
            &content.images[0].source,
            Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidImage, "{dictionary}");
    }
}

// The chunks of a PNG file, after its signature: type and data.
fn png_chunks(png: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let mut chunks = Vec::new();
    let mut at = 8;
    while at < png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let kind = String::from_utf8(png[at + 4..at + 8].to_vec()).unwrap();
        chunks.push((kind, png[at + 8..at + 8 + length].to_vec()));
        at += 12 + length;
    }
    chunks
}

fn file_of(content: &str) -> Option<ImageFile> {
    let pdf = file(content);
    let (mut document, page) = run(&pdf);
    let image = read_image(
        document.resolver(),
        &page.images[0].source,
        Limits::default(),
    )
    .unwrap();
    image_file(&image)
}

#[test]
fn rgb_pixels_become_a_png_file() {
    let png = file_of("/Im1 Do").unwrap();
    assert_eq!(png.format, ImageFormat::Png);
    let chunks = png_chunks(&png.data);
    let kinds: Vec<&str> = chunks.iter().map(|(kind, _)| kind.as_str()).collect();
    assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
    // 2 by 1, 8 bits, RGB.
    assert_eq!(chunks[0].1, [0, 0, 0, 2, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
    let rows = miniz_oxide::inflate::decompress_to_vec_zlib(&chunks[1].1).unwrap();
    assert_eq!(rows, [0, 0xFF, 0, 0, 0, 0xFF, 0]);
}

#[test]
fn gray_pixels_become_a_png_file_and_missing_ones_are_black() {
    // A 1 by 2 gray image with one byte of data.
    let png = file_of("BI /W 1 /H 2 /CS /G /BPC 8 /F /AHx ID 80> EI").unwrap();
    let chunks = png_chunks(&png.data);
    assert_eq!(chunks[0].1[9], 0);
    let rows = miniz_oxide::inflate::decompress_to_vec_zlib(&chunks[1].1).unwrap();
    assert_eq!(rows, [0, 0x80, 0, 0]);
}

#[test]
fn a_jpeg_image_is_given_as_the_pdf_has_it() {
    let jpeg = file_of("/Jpeg Do").unwrap();
    assert_eq!(jpeg.format, ImageFormat::Jpeg);
    assert_eq!(jpeg.format.extension(), "jpg");
    assert_eq!(jpeg.data, [0xFF, 0xD8, 0xFF, 0xD9]);
}

#[test]
fn images_whose_colors_arent_supported_yet_have_no_file() {
    assert_eq!(
        file_of("BI /W 2 /H 1 /CS /CS0 /BPC 1 /F /AHx ID 40> EI"),
        None
    );
    assert_eq!(file_of("BI /W 8 /H 1 /IM true /F /AHx ID F0> EI"), None);
    assert_eq!(
        file_of("BI /W 1 /H 1 /CS /G /BPC 8 /D [1 0] /F /AHx ID 80> EI"),
        None
    );
}

#[test]
fn every_image_of_a_document_is_found_in_its_place() {
    let pdf = file("q 200 0 0 100 50 600 cm /Im1 Do Q q 10 0 0 10 0 0 cm /Im1 Do /Jpeg Do Q");
    let mut document = Document::open(&pdf).unwrap();
    let pages = document_images(&mut document);
    assert_eq!(pages.len(), 1);
    let images = &pages[0].images;
    assert_eq!(images.len(), 3);
    let b = images[0].bbox;
    assert_eq!(
        [b.left, b.bottom, b.right, b.top],
        [50.0, 600.0, 250.0, 700.0]
    );
    // The same image, read once, is in both places.
    assert_eq!(images[0].id, images[1].id);
    assert_eq!(images[0].file, images[1].file);
    assert_eq!(images[2].file.as_ref().unwrap().format, ImageFormat::Jpeg);
    assert_eq!(images[2].description.as_ref().unwrap().width, 4);
    assert_eq!(pages[0].problem, None);
}

#[test]
fn an_image_that_cant_be_read_is_still_placed() {
    let pdf = file("BI /W 0 /H 1 /CS /G /BPC 8 ID \x00 EI");
    let mut document = Document::open(&pdf).unwrap();
    let pages = document_images(&mut document);
    let image = &pages[0].images[0];
    assert_eq!(image.description, None);
    assert_eq!(image.file, None);
}
