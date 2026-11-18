//! Pages drawn by the engine against MuPDF's: the pages `tests/corpus/render.tsv`
//! lists, with the references `render.py` made.

use std::path::{Path, PathBuf};

use ozaru_engine::Document;
use ozaru_engine::render::{RenderOptions, render_page};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

// The luma of a difference, as Pillow's `convert("L")` gives it.
fn luma(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(r) * 19595 + u32::from(g) * 38470 + u32::from(b) * 7471 + 0x8000) >> 16
}

fn reference(name: &str, page: usize) -> (u32, u32, Vec<u8>) {
    let file = corpus()
        .join("render")
        .join(format!("{}-{page}.rgb.z", name.trim_end_matches(".pdf")));
    let bytes =
        miniz_oxide::inflate::decompress_to_vec_zlib(&std::fs::read(file).unwrap()).unwrap();
    let width = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    (width, height, bytes[8..].to_vec())
}

#[test]
fn every_page_is_drawn_as_mupdf_draws_it() {
    let table = std::fs::read_to_string(corpus().join("render.tsv")).unwrap();
    let mut count = 0;
    for line in table.lines().filter(|l| !l.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        let (name, page) = (columns[0], columns[1].parse::<usize>().unwrap());
        let (max_mean, max_share): (f64, f64) =
            (columns[2].parse().unwrap(), columns[3].parse().unwrap());
        let data = std::fs::read(corpus().join(name)).unwrap();
        let mut document = Document::open(&data).unwrap();
        let limits = document.limits();
        let page_info = document.page(page).unwrap().clone();
        let pixmap = render_page(
            document.resolver(),
            &page_info,
            &RenderOptions::default(),
            limits,
        )
        .unwrap();
        let (width, height, theirs) = reference(name, page);
        assert_eq!(
            (pixmap.width() as u32, pixmap.height() as u32),
            (width, height),
            "{name} {page}"
        );
        let (mut total, mut over) = (0u64, 0u64);
        for (ours, reference) in pixmap.data().chunks(4).zip(theirs.chunks(3)) {
            let d = luma(
                ours[0].abs_diff(reference[0]),
                ours[1].abs_diff(reference[1]),
                ours[2].abs_diff(reference[2]),
            );
            total += u64::from(d);
            over += u64::from(d > 64);
        }
        let pixels = (width * height) as f64;
        let (mean, share) = (total as f64 / pixels, over as f64 / pixels);
        eprintln!("{name} {page}: mean {mean:.3}, share over 64 {share:.5}");
        assert!(
            mean <= max_mean,
            "{name} {page}: mean {mean} over {max_mean}"
        );
        assert!(
            share <= max_share,
            "{name} {page}: share {share} over {max_share}"
        );
        count += 1;
    }
    assert!(count >= 20);
}

#[test]
fn a_page_is_drawn_at_any_scale_and_a_bad_scale_is_refused() {
    let data = std::fs::read(corpus().join("one-page.pdf")).unwrap();
    let mut document = Document::open(&data).unwrap();
    let limits = document.limits();
    let page = document.page(0).unwrap().clone();
    let one = render_page(
        document.resolver(),
        &page,
        &RenderOptions::default(),
        limits,
    )
    .unwrap();
    let options = RenderOptions {
        scale: 2.0,
        ..RenderOptions::default()
    };
    let two = render_page(document.resolver(), &page, &options, limits).unwrap();
    // Sizes round up: a page of 595.28 points is 596 pixels, and 1191 at twice.
    assert!(two.width().abs_diff(one.width() * 2) <= 1);
    assert!(two.height().abs_diff(one.height() * 2) <= 1);
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let options = RenderOptions {
            scale,
            ..RenderOptions::default()
        };
        assert!(render_page(document.resolver(), &page, &options, limits).is_err());
    }
    // A page that would take more than the limit.
    let options = RenderOptions {
        scale: 10_000.0,
        ..RenderOptions::default()
    };
    assert!(render_page(document.resolver(), &page, &options, limits).is_err());
}
