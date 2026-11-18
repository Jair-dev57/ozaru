//! The JPEG decoder against libjpeg, through Pillow: the files of
//! `tests/files/jpeg`, made by `tests/corpus/jpeg.py`, and the pixels it
//! decodes them to.

use std::fs;
use std::path::PathBuf;

use ozaru_engine::base::{ErrorKind, Limits};
use ozaru_engine::filters::decode_jpeg;

mod common;

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/files/jpeg")
}

fn names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder())
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".jpg").map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

fn read(name: &str) -> Vec<u8> {
    fs::read(folder().join(format!("{name}.jpg"))).unwrap()
}

/// The largest difference and the mean one between the samples.
fn difference(a: &[u8], b: &[u8]) -> (u8, f64) {
    assert_eq!(a.len(), b.len());
    let mut largest = 0;
    let mut total = 0u64;
    for (x, y) in a.iter().zip(b) {
        let d = x.abs_diff(*y);
        largest = largest.max(d);
        total += u64::from(d);
    }
    (largest, total as f64 / a.len().max(1) as f64)
}

#[test]
fn every_file_decodes_to_the_pixels_libjpeg_gives() {
    let names = names();
    assert!(names.len() >= 19, "{names:?}");
    for name in names {
        let image = decode_jpeg(&read(&name), None, 0, Limits::default()).unwrap();
        let expected = fs::read(folder().join(format!("{name}.raw"))).unwrap();
        assert_eq!(
            image.data.len(),
            expected.len(),
            "{name}: {}x{}x{}",
            image.width,
            image.height,
            image.components
        );
        let (largest, mean) = difference(&image.data, &expected);
        eprintln!("{name}: largest {largest}, mean {mean:.3}");
        assert!(image.diagnostics.is_empty(), "{name}");
        assert!(largest <= 3 && mean < 0.25, "{name}: {largest} {mean}");
    }
}

fn decode(data: &[u8]) -> ozaru_engine::base::Result<ozaru_engine::filters::JpegImage> {
    decode_jpeg(data, None, 0, Limits::default())
}

#[test]
fn the_color_transform_is_given_or_left() {
    let data = read("rgb-444");
    let by_default = decode(&data).unwrap();
    let asked = decode_jpeg(&data, Some(true), 0, Limits::default()).unwrap();
    assert_eq!(by_default.data, asked.data);
    // Left as the file has it: Y, Cb and Cr.
    let raw = decode_jpeg(&data, Some(false), 0, Limits::default()).unwrap();
    assert_eq!(raw.data.len(), by_default.data.len());
    assert_ne!(raw.data, by_default.data);
    // A gray pixel at the corner of the picture is ~ (0, 0, 127): Cb and Cr
    // of a color are far from 128 in the color one.
    assert_eq!(raw.components, 3);
}

#[test]
fn damaged_data_gives_what_could_be_decoded() {
    for name in ["rgb-420", "rgb-420-progressive", "gray", "rgb-restart"] {
        let data = read(name);
        let whole = decode(&data).unwrap();
        // Cut a quarter before the end.
        let cut = decode(&data[..data.len() * 3 / 4]).unwrap();
        assert_eq!(cut.data.len(), whole.data.len(), "{name}");
        assert!(!cut.diagnostics.is_empty(), "{name}");
        // The start of the first row is the same, before the pixels that
        // depend on blocks the cut lost.
        let row = (whole.width - 16) * whole.components;
        if name != "rgb-420-progressive" {
            assert_eq!(cut.data[..row], whole.data[..row], "{name}");
        }
    }
}

#[test]
fn no_byte_of_a_file_makes_the_decoder_fail_badly() {
    for name in [
        "rgb-420",
        "rgb-420-progressive",
        "cmyk",
        "rgb-restart",
        "gray",
    ] {
        let data = read(name);
        let limits = Limits {
            max_decoded_length: 1 << 20,
            ..Limits::default()
        };
        for at in (0..data.len()).step_by(5) {
            for value in [0u8, 0xFF, data[at] ^ 0x55] {
                let mut damaged = data.clone();
                damaged[at] = value;
                if let Ok(image) = decode_jpeg(&damaged, None, 0, limits) {
                    assert_eq!(
                        image.data.len(),
                        image.width * image.height * image.components
                    );
                }
            }
        }
    }
}

#[test]
fn other_data_is_an_error() {
    let error = decode(b"not a JPEG").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidJpegData);
    // The start of an image and no frame or scan.
    let error = decode(&[0xFF, 0xD8, 0xFF, 0xD9]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidJpegData);
    // A frame of arithmetic coding, which is left out.
    let data = [
        0xFF, 0xD8, 0xFF, 0xC9, 0x00, 0x0B, 8, 0, 8, 0, 8, 1, 1, 0x11, 0, 0xFF, 0xD9,
    ];
    assert_eq!(
        decode(&data).unwrap_err().kind(),
        ErrorKind::UnsupportedFilter
    );
}

#[test]
fn an_image_larger_than_the_limit_is_refused() {
    let limits = Limits {
        max_decoded_length: 100,
        ..Limits::default()
    };
    let error = decode_jpeg(&read("rgb-420"), None, 0, limits).unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::LimitExceeded(_)));
}

// PDF files hold JPEG as hex, to write them here as text.
fn hex(name: &str) -> String {
    let mut text: String = read(name).iter().map(|b| format!("{b:02X}")).collect();
    text.push('>');
    text
}

fn picture_object(name: &str, size: (usize, usize), space: &str, extra: &str) -> String {
    common::stream_object(
        &format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {space} \
             /BitsPerComponent 8 /Filter [/ASCIIHexDecode /DCTDecode] {extra}",
            size.0, size.1
        ),
        &hex(name),
    )
}

// The pixels of a PNG file, and the number of channels it has.
fn png(file: &ozaru_engine::images::ImageFile) -> (usize, Vec<u8>) {
    assert_eq!(file.format, ozaru_engine::images::ImageFormat::Png);
    let data = &file.data;
    let mut at = 8;
    let mut header = Vec::new();
    let mut idat = Vec::new();
    while at < data.len() {
        let length = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        let kind = &data[at + 4..at + 8];
        let body = &data[at + 8..at + 8 + length];
        match kind {
            b"IHDR" => header = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            _ => {}
        }
        at += 12 + length;
    }
    let width = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
    let channels = match header[9] {
        0 => 1,
        2 => 3,
        4 => 2,
        _ => 4,
    };
    let rows = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).unwrap();
    let row = width * channels;
    (
        channels,
        rows.chunks(row + 1).flat_map(|r| r[1..].to_vec()).collect(),
    )
}

#[test]
fn jpeg_images_with_masks_or_four_components_become_png_files() {
    use common::{file_with, stream_object};
    use ozaru_engine::images::{ImageFormat, document_images};
    let size = (67, 45);
    let pdf = file_with(&[
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /XObject \
             << /A 5 0 R /B 7 0 R /C 8 0 R >> >> /Contents 4 0 R >>"
                .to_string(),
        ),
        (4, stream_object("", "/A Do /B Do /C Do")),
        // With a soft mask that is a JPEG too.
        (
            5,
            picture_object("rgb-420", size, "/DeviceRGB", "/SMask 6 0 R"),
        ),
        (6, picture_object("gray", size, "/DeviceGray", "")),
        // Adobe's CMYK is inverted, which a /Decode array undoes.
        (
            7,
            picture_object("cmyk", size, "/DeviceCMYK", "/Decode [1 0 1 0 1 0 1 0]"),
        ),
        // Without either: as it is.
        (8, picture_object("rgb-444", size, "/DeviceRGB", "")),
    ]);
    let mut document = ozaru_engine::Document::open(&pdf).unwrap();
    let images: Vec<_> = document_images(&mut document)
        .into_iter()
        .flat_map(|page| page.images)
        .collect();
    let file = |number: u32| {
        images
            .iter()
            .find(|i| i.id.is_some_and(|id| id.number == number))
            .and_then(|i| i.file.clone())
            .unwrap()
    };

    let (channels, rgba) = png(&file(5));
    assert_eq!(channels, 4);
    let color = fs::read(folder().join("rgb-420.raw")).unwrap();
    let alpha = fs::read(folder().join("gray.raw")).unwrap();
    for (i, pixel) in rgba.chunks(4).enumerate() {
        for (c, &value) in pixel[..3].iter().enumerate() {
            assert!(value.abs_diff(color[i * 3 + c]) <= 3, "pixel {i}");
        }
        assert!(pixel[3].abs_diff(alpha[i]) <= 3, "alpha {i}");
    }

    let (channels, rgb) = png(&file(7));
    assert_eq!(channels, 3);
    // DeviceCMYK to RGB: the complement of each ink with the black added.
    let inverted = fs::read(folder().join("cmyk.raw")).unwrap();
    for (i, pixel) in rgb.chunks(3).enumerate() {
        let ink = |c: usize| 255 - u32::from(inverted[i * 4 + c]);
        let k = ink(3);
        for c in 0..3 {
            let expected = 255 - (ink(c) + k).min(255);
            assert!(
                u32::from(pixel[c]).abs_diff(expected) <= 3,
                "pixel {i} channel {c}: {} {expected}",
                pixel[c]
            );
        }
    }

    let plain = file(8);
    assert_eq!(plain.format, ImageFormat::Jpeg);
    assert_eq!(plain.data, read("rgb-444"));
}
