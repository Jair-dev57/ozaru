//! The PNG reader, against ImageMagick's decoding of the files of
//! `files/png` (made by `corpus/png.py`).

use ozaru_engine::base::ErrorKind;
use ozaru_engine::images::{PngColor, read_png};
use std::fs;
use std::path::Path;

fn rgba(data: &[u8]) -> Vec<u8> {
    let png = read_png(data).unwrap();
    let count = (png.width * png.height) as usize;
    let mut out = Vec::with_capacity(count * 4);
    for i in 0..count {
        let [r, g, b] = match png.color {
            PngColor::Gray => [png.pixels[i]; 3],
            _ => [
                png.pixels[3 * i],
                png.pixels[3 * i + 1],
                png.pixels[3 * i + 2],
            ],
        };
        out.extend_from_slice(&[r, g, b, png.alpha.as_ref().map_or(255, |a| a[i])]);
    }
    out
}

#[test]
fn every_file_decodes_as_imagemagick_does() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/png");
    let mut checked = 0;
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "png") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let expected = fs::read(path.with_extension("raw")).unwrap();
        let got = rgba(&fs::read(&path).unwrap());
        assert_eq!(got.len(), expected.len(), "{name}");
        let slack = if name.contains("16") { 1 } else { 0 };
        for (i, (a, b)) in got.iter().zip(&expected).enumerate() {
            // Where a pixel is fully transparent its color doesn't matter.
            if got[i | 3] == 0 && expected[i | 3] == 0 {
                continue;
            }
            assert!(
                a.abs_diff(*b) <= slack,
                "{name}: byte {i} is {a}, ImageMagick has {b}"
            );
        }
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} files");
}

#[test]
fn what_is_not_a_png_or_is_damaged_is_an_error() {
    let good =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/png/rgb8.png")).unwrap();
    for bad in [&b""[..], b"not a png", &good[..8], &good[..30]] {
        assert_eq!(read_png(bad).unwrap_err().kind(), ErrorKind::InvalidImage);
    }
    // A color type that doesn't exist.
    let mut wrong = good.clone();
    wrong[25] = 5;
    assert!(read_png(&wrong).is_err());
}

#[test]
fn cut_short_pixels_keep_the_rows_there_were() {
    let good =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/png/rgb8.png")).unwrap();
    // Take the end off the data: the header and some of the pixels stay.
    let cut = &good[..good.len() - 40];
    let png = read_png(cut).unwrap();
    assert_eq!((png.width, png.height), (13, 11));
    assert_eq!(png.pixels.len(), 13 * 11 * 3);
}
