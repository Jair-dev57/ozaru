use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::filters::decode_jbig2;

// A segment header of page 1, with no referred segments, and its data.
fn segment(number: u32, kind: u8, data: &[u8]) -> Vec<u8> {
    let mut out = number.to_be_bytes().to_vec();
    out.extend([kind, 0, 1]);
    out.extend((data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
    out
}

fn page_info(width: u32, height: u32, default_black: bool) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(width.to_be_bytes());
    data.extend(height.to_be_bytes());
    data.extend([0; 8]);
    data.push(if default_black { 4 } else { 0 });
    data.extend([0, 0]);
    data
}

#[test]
fn a_page_without_regions_has_its_default_pixel() {
    let white = segment(1, 48, &page_info(10, 2, false));
    let black = segment(1, 48, &page_info(10, 2, true));

    let page = decode_jbig2(&white, None, 0, Limits::default()).unwrap();
    assert_eq!((page.width, page.height), (10, 2));
    // 0 is black in PDF image data: a white page is all 1 bits.
    assert_eq!(page.data, [0xFF, 0xFF, 0xFF, 0xFF]);
    let page = decode_jbig2(&black, None, 0, Limits::default()).unwrap();
    assert_eq!(page.data, [0, 0, 0, 0]);
}

#[test]
fn data_without_page_information_is_an_error() {
    let error = decode_jbig2(&segment(1, 49, &[]), None, 7, Limits::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidJbig2Data);
    assert_eq!(error.offset(), 7);
    let error = decode_jbig2(&[], None, 0, Limits::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidJbig2Data);
}

#[test]
fn the_globals_come_before_the_page() {
    // The page information in the globals, the end of page in the data.
    let globals = segment(1, 48, &page_info(8, 1, true));
    let page = decode_jbig2(&segment(2, 49, &[]), Some(&globals), 0, Limits::default()).unwrap();
    assert_eq!(page.data, [0]);
}

#[test]
fn damaged_regions_are_left_out_with_a_diagnostic() {
    let mut data = segment(1, 48, &page_info(8, 2, false));
    // A generic region too short to have its region information.
    data.extend(segment(2, 38, &[0, 0, 0]));
    // A header cut short.
    data.extend([0, 0, 0, 3, 38]);

    let page = decode_jbig2(&data, None, 0, Limits::default()).unwrap();
    assert_eq!(page.data, [0xFF, 0xFF]);
    let kinds: Vec<_> = page.diagnostics.iter().map(|d| d.kind()).collect();
    assert_eq!(
        kinds,
        [
            DiagnosticKind::DamagedJbig2Data,
            DiagnosticKind::DamagedJbig2Data
        ]
    );
}

#[test]
fn pages_larger_than_the_limits_are_errors() {
    let data = segment(1, 48, &page_info(1 << 20, 1 << 20, false));
    let error = decode_jbig2(&data, None, 0, Limits::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded(Limit::DecodedLength));
}

#[test]
fn a_page_of_unknown_height_grows_with_its_stripes() {
    let mut data = segment(1, 48, &page_info(8, u32::MAX, false));
    data.extend(segment(2, 50, &4u32.to_be_bytes()));

    let page = decode_jbig2(&data, None, 0, Limits::default()).unwrap();
    assert_eq!((page.width, page.height), (8, 5));
}

#[test]
fn every_jbig2_image_of_the_corpus_decodes_without_problems() {
    use ozaru_engine::Document;
    use ozaru_engine::images::{ImageFormat, document_images};

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/images-jbig2.pdf");
    let file = std::fs::read(path).unwrap();
    let mut document = Document::open(&file).unwrap();
    let pages = document_images(&mut document);
    assert_eq!(pages[0].images.len(), 10);
    for image in &pages[0].images {
        assert_eq!(image.file.as_ref().unwrap().format, ImageFormat::Png);
    }
    assert!(pages[0].problem.is_none());
}

// A file whose JBIG2 image asks for a region of a thousand million pixels, found by
// fuzzing: it took half a minute to decode garbage, and now it is refused at once.
#[test]
fn a_region_too_large_for_the_work_budget_is_refused_at_once() {
    use ozaru_engine::Document;
    use ozaru_engine::content::ImageSource;
    use ozaru_engine::images::read_image;
    use ozaru_engine::syntax::ObjectId;
    use std::time::{Duration, Instant};

    let data = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/files/jbig2/huge-region.pdf"
    ))
    .unwrap();
    let mut document = Document::open(&data).unwrap();
    let limits = document.limits();
    let id = ObjectId::new(8, 0);
    let object = document.resolver().resolve(id).unwrap();
    let started = Instant::now();
    let result = read_image(
        document.resolver(),
        &ImageSource::XObject { id, object },
        limits,
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(result.is_err(), "the region should be refused");
}
