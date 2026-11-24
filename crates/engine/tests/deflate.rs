//! The engine's own Deflate: compression and inflating, checked against streams
//! and sizes that Python's zlib made (`tests/files/zlib-vectors.tsv`).

use ozaru_engine::filters::{InflateStatus, adler32, deflate, deflate_zlib, inflate};

// Bytes from a fixed generator: `alphabet` values from 0 to 255.
fn noise(length: usize, alphabet: u32, seed: u32) -> Vec<u8> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((state >> 16) % alphabet) as u8
        })
        .collect()
}

// Text with words that repeat, as a content stream does.
fn text(length: usize) -> Vec<u8> {
    let words = [
        "BT", "/F1", "12", "Tf", "72", "712", "Td", "(Hello)", "Tj", "ET", "q", "Q", "re", "f",
    ];
    let mut state = 7u32;
    let mut out = Vec::new();
    while out.len() < length {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        out.extend_from_slice(words[(state >> 24) as usize % words.len()].as_bytes());
        out.push(if state & 0x100 != 0 { b' ' } else { b'\n' });
    }
    out.truncate(length);
    out
}

fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("empty", Vec::new()),
        ("one byte", vec![42]),
        ("two bytes", vec![1, 1]),
        ("zeros", vec![0; 1_000_000]),
        ("one letter", noise(70_000, 1, 1)),
        ("bytes", noise(100_000, 256, 2)),
        ("few letters", noise(300_000, 4, 3)),
        ("text", text(400_000)),
        ("short text", text(300)),
        ("block edges", text(65_537)),
        (
            "pattern",
            (0..200_000u32).map(|i| (i % 251) as u8).collect(),
        ),
    ]
}

#[test]
fn what_is_compressed_inflates_to_the_same_bytes_at_every_level() {
    for (name, data) in samples() {
        for level in 0..=10 {
            let raw = deflate(&data, level);
            let (back, status) = inflate(&raw, false, usize::MAX);
            assert_eq!(status, InflateStatus::Done, "{name} level {level}");
            assert!(back == data, "{name} level {level}");
            let wrapped = deflate_zlib(&data, level);
            let (back, status) = inflate(&wrapped, true, usize::MAX);
            assert_eq!(status, InflateStatus::Done, "{name} level {level} zlib");
            assert!(back == data, "{name} level {level} zlib");
        }
    }
}

#[test]
fn the_engine_inflates_what_zlib_compresses() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files/zlib-vectors.tsv");
    let table = std::fs::read_to_string(path).unwrap();
    let samples = samples();
    let mut seen = 0;
    for line in table.lines().filter(|line| !line.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        let (name, level) = (columns[0], columns[1]);
        let stream: Vec<u8> = (0..columns[2].len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&columns[2][i..i + 2], 16).unwrap())
            .collect();
        let (back, status) = inflate(&stream, true, usize::MAX);
        assert_eq!(status, InflateStatus::Done, "{name} level {level}");
        // The vectors are of the first bytes of each sample, as many as they hold.
        let expected = &samples.iter().find(|(n, _)| *n == name).unwrap().1;
        let length = match name {
            "empty" => 0,
            "one byte" => 1,
            "zeros" | "few letters" | "pattern" => 3000,
            "one letter" => 2500,
            "bytes" => 2000,
            _ => 4000,
        };
        assert_eq!(back.len(), length, "{name} level {level}");
        assert!(expected.starts_with(&back), "{name} level {level}");
        seen += 1;
    }
    assert_eq!(seen, 32);
}

// What Python's zlib makes of the bigger samples at levels 1, 6 and 9.
#[test]
fn compression_is_about_as_small_as_zlibs() {
    let sizes: &[(&str, [usize; 3])] = &[
        ("zeros", [4383, 991, 991]),
        ("bytes", [100041, 100041, 100041]),
        ("few letters", [99797, 88024, 85828]),
        ("text", [137906, 112616, 106765]),
        ("pattern", [1837, 1100, 1100]),
    ];
    let samples = samples();
    for &(name, theirs) in sizes {
        let data = &samples.iter().find(|(n, _)| *n == name).unwrap().1;
        for (level, theirs) in [1u8, 6, 9].into_iter().zip(theirs) {
            let ours = deflate_zlib(data, level).len();
            // Within a fifth (the fastest level looks less), plus a little for
            // the very small outputs.
            assert!(
                ours * 10 <= theirs * 12 + 200,
                "{name} level {level}: {ours} bytes against {theirs}"
            );
            assert!(
                ours <= data.len() + data.len() / 100 + 64,
                "{name} level {level}"
            );
        }
    }
}

#[test]
fn inflating_stops_at_the_limit_and_says_so() {
    let data = vec![7u8; 100_000];
    let packed = deflate_zlib(&data, 6);

    let (out, status) = inflate(&packed, true, 1000);
    assert_eq!(status, InflateStatus::LimitReached);
    assert_eq!(out.len(), 1000);
    let (out, status) = inflate(&packed, true, 100_000);
    assert_eq!(status, InflateStatus::Done);
    assert_eq!(out.len(), 100_000);
}

#[test]
fn damaged_data_keeps_what_came_before_and_never_panics() {
    let data = text(50_000);
    let packed = deflate_zlib(&data, 6);

    // Cut short.
    let (out, status) = inflate(&packed[..packed.len() / 2], true, usize::MAX);
    assert_eq!(status, InflateStatus::Damaged);
    assert!(!out.is_empty() && data.starts_with(&out));
    // A wrong checksum.
    let mut wrong = packed.clone();
    *wrong.last_mut().unwrap() ^= 1;
    let (out, status) = inflate(&wrong, true, usize::MAX);
    assert_eq!((status, out == data), (InflateStatus::Damaged, true));
    // Bytes after the end are ignored.
    let mut longer = packed.clone();
    longer.extend_from_slice(b"\r\n");
    assert_eq!(inflate(&longer, true, usize::MAX).1, InflateStatus::Done);
    // Flipped bytes anywhere: whatever happens, no panic.
    for at in (0..packed.len()).step_by(97) {
        let mut broken = packed.clone();
        broken[at] ^= 0x5A;
        let _ = inflate(&broken, true, 1 << 20);
        let _ = inflate(&broken[2..], false, 1 << 20);
    }
    // Garbage.
    for seed in 0..200 {
        let garbage = noise(300, 256, seed);
        let _ = inflate(&garbage, false, 1 << 16);
        let _ = inflate(&garbage, true, 1 << 16);
    }
}

#[test]
fn adler32_matches_the_standard_examples() {
    assert_eq!(adler32(b""), 1);
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    // Long enough to need the reduction in the middle: Python's zlib.adler32.
    assert_eq!(adler32(&vec![0xFF; 100_000]), 0x149A_302C);
}
