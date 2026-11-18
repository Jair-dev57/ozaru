//! Decodes a JPEG file, prints its size and how long it took, and writes it
//! as a PNM file if asked.
//!
//!     cargo run --release --example jpeg -- photo.jpg [out.pnm]

use std::time::Instant;

use ozaru_engine::base::Limits;
use ozaru_engine::filters::decode_jpeg;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a JPEG file");
    let data = std::fs::read(&path).expect("readable file");
    let limits = Limits {
        max_decoded_length: 1 << 30,
        ..Limits::default()
    };
    let start = Instant::now();
    let image = decode_jpeg(&data, None, 0, limits).expect("a JPEG");
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{}x{}, {} components, {:.1} ms, {:.1} ns per pixel, {} diagnostics",
        image.width,
        image.height,
        image.components,
        seconds * 1e3,
        seconds * 1e9 / (image.width * image.height) as f64,
        image.diagnostics.len()
    );
    if let Some(out) = args.next() {
        let magic = if image.components == 1 { "P5" } else { "P6" };
        let mut bytes = format!("{magic}\n{} {}\n255\n", image.width, image.height).into_bytes();
        bytes.extend_from_slice(&image.data);
        std::fs::write(out, bytes).expect("writable file");
    }
}
