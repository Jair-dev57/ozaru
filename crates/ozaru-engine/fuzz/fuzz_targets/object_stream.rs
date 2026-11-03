#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::base::Limits;
use ozaru_engine::syntax::parse_indirect_object;
use ozaru_engine::xref::parse_object_stream;

// The first byte sets /N and the second /First, the rest is the stream data. Every
// object is an object or an error, and the header never lists more than /N.
fuzz_target!(|data: &[u8]| {
    let [count, first, data @ ..] = data else {
        return;
    };
    let mut file = format!(
        "1 0 obj << /Type /ObjStm /N {count} /First {first} /Length {} >> stream\n",
        data.len()
    )
    .into_bytes();
    file.extend_from_slice(data);
    file.extend_from_slice(b"\nendstream endobj");
    let parsed = parse_indirect_object(&file, 0, Limits::default()).unwrap();
    let stream = parsed.object.object.as_stream().unwrap();

    if let Ok(parsed) = parse_object_stream(stream, &file, Limits::default()) {
        assert!(parsed.stream.len() <= usize::from(*count));
        for index in 0..=u32::from(*count) {
            let _ = parsed.stream.object(index);
        }
    }
});
