use std::collections::HashMap;

use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::filters::{Decoded, decode_lzw, decode_run_length, decode_stream};
use ozaru_engine::syntax::{Dictionary, Object, parse_indirect_object, parse_object};

fn lzw(data: &[u8]) -> Decoded {
    decode_lzw(data, None, 0, Limits::default()).expect("data should be valid")
}

fn lzw_with(data: &[u8], parameters: &[u8]) -> Decoded {
    decode_lzw(data, Some(&dictionary(parameters)), 0, Limits::default())
        .expect("data should be valid")
}

fn dictionary(input: &[u8]) -> Dictionary {
    parse_object(input, Limits::default())
        .unwrap()
        .object
        .as_dictionary()
        .unwrap()
        .clone()
}

fn run_length(data: &[u8]) -> Decoded {
    decode_run_length(data, 0, Limits::default()).expect("data should be valid")
}

fn kinds(decoded: &Decoded) -> Vec<DiagnosticKind> {
    decoded.diagnostics.iter().map(|d| d.kind()).collect()
}

// An LZW encoder as PDF writers do it, starting with a clear code and clearing the
// table when it's full, to check round trips with either EarlyChange.
fn encode_lzw(data: &[u8], early_change: usize) -> Vec<u8> {
    let mut codes: Vec<(u16, u32)> = Vec::new();
    let mut table: HashMap<Vec<u8>, u16> = HashMap::new();
    let mut next = 258usize;
    let mut width = 9;
    let reset = |table: &mut HashMap<Vec<u8>, u16>| {
        table.clear();
        for byte in 0..=255u8 {
            table.insert(vec![byte], u16::from(byte));
        }
    };
    reset(&mut table);
    codes.push((256, width));
    let mut word: Vec<u8> = Vec::new();
    for &byte in data {
        let mut longer = word.clone();
        longer.push(byte);
        if table.contains_key(&longer) {
            word = longer;
            continue;
        }
        codes.push((table[&word], width));
        table.insert(longer, next as u16);
        next += 1;
        // The decoder adds each entry one code later, so it widens its codes
        // when it has seen next - 1 entries.
        while width < 12 && next - 1 + early_change >= 1 << width {
            width += 1;
        }
        if next == 4096 {
            codes.push((256, width));
            reset(&mut table);
            next = 258;
            width = 9;
        }
        word = vec![byte];
    }
    if !word.is_empty() {
        codes.push((table[&word], width));
        next += 1;
        while width < 12 && next - 1 + early_change >= 1 << width {
            width += 1;
        }
    }
    codes.push((257, width));

    let mut output = Vec::new();
    let (mut buffer, mut count) = (0u64, 0u32);
    for (code, width) in codes {
        buffer = buffer << width | u64::from(code);
        count += width;
        while count >= 8 {
            count -= 8;
            output.push((buffer >> count) as u8);
        }
    }
    if count > 0 {
        output.push((buffer << (8 - count)) as u8);
    }
    output
}

fn varied(length: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            b"aaab cd\n"[(state >> 16) as usize % 8]
        })
        .collect()
}

// LZWDecode

#[test]
fn the_example_of_the_standard_is_decoded() {
    // ISO 32000-2, section 7.4.4.2: codes 256 45 258 258 65 259 66 257.
    let decoded = lzw(&[0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01]);

    assert_eq!(decoded.data, [45, 45, 45, 45, 45, 65, 45, 45, 45, 66]);
    assert!(decoded.diagnostics.is_empty());
}

#[test]
fn data_survives_a_round_trip_with_either_early_change() {
    for (length, seed) in [(0, 1), (1, 2), (100, 3), (5_000, 4), (60_000, 5)] {
        let data = varied(length, seed);

        assert_eq!(lzw(&encode_lzw(&data, 1)).data, data, "{length} bytes");
        assert_eq!(
            lzw_with(&encode_lzw(&data, 0), b"<< /EarlyChange 0 >>").data,
            data,
            "{length} bytes, EarlyChange 0"
        );
    }
}

#[test]
fn a_full_table_is_cleared_and_reused() {
    // Random-looking bytes fill the 4096 codes many times over.
    let data: Vec<u8> = (0..60_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();

    assert_eq!(lzw(&encode_lzw(&data, 1)).data, data);
}

#[test]
fn the_wrong_early_change_gives_other_data() {
    let data = varied(5_000, 6);

    let decoded = decode_lzw(
        &encode_lzw(&data, 1),
        Some(&dictionary(b"<< /EarlyChange 0 >>")),
        0,
        Limits::default(),
    );

    assert!(decoded.is_err() || decoded.unwrap().data != data);
}

#[test]
fn lzw_data_without_an_end_code_is_decoded_and_reported() {
    let mut data = encode_lzw(b"no end code here", 1);
    // Drop the end code: the last two bytes hold it and padding.
    data.truncate(data.len() - 2);

    let decoded = lzw(&data);

    assert!(b"no end code here".starts_with(&decoded.data));
    assert!(!decoded.data.is_empty());
    assert_eq!(kinds(&decoded), [DiagnosticKind::MissingEndOfDataMarker]);
}

#[test]
fn a_code_not_in_the_table_is_an_error() {
    // 256, then 300, which nothing defined.
    let error = decode_lzw(&[0x80, 0x4B, 0x00], None, 5, Limits::default()).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::InvalidLzwData, 5)
    );
}

#[test]
fn lzw_uses_the_png_predictors() {
    // Two rows of three bytes, each predicted with Up.
    let predicted = [2, 1, 2, 3, 2, 1, 1, 1];

    let decoded = lzw_with(
        &encode_lzw(&predicted, 1),
        b"<< /Predictor 12 /Columns 3 >>",
    );

    assert_eq!(decoded.data, [1, 2, 3, 2, 3, 4]);
}

#[test]
fn early_change_must_be_zero_or_one() {
    for parameters in [&b"<< /EarlyChange 2 >>"[..], b"<< /EarlyChange /No >>"] {
        let error = decode_lzw(
            &encode_lzw(b"a", 1),
            Some(&dictionary(parameters)),
            0,
            Limits::default(),
        );

        assert_eq!(
            error.unwrap_err().kind(),
            ErrorKind::InvalidDecodeParameters
        );
    }
}

#[test]
fn lzw_bombs_stop_at_the_limit() {
    let data = encode_lzw(&vec![b'a'; 1_000_000], 1);
    assert!(data.len() < 4_000, "{} bytes", data.len());
    let limits = Limits {
        max_decoded_length: 100_000,
        ..Limits::default()
    };

    let error = decode_lzw(&data, None, 0, limits).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::LimitExceeded(Limit::DecodedLength));
}

// RunLengthDecode

#[test]
fn literal_and_repeated_runs_are_decoded() {
    // Copy 3 bytes, repeat 'x' 4 times (257 - 253), copy 1 byte, end.
    let decoded = run_length(&[2, b'a', b'b', b'c', 253, b'x', 0, b'!', 128]);

    assert_eq!(decoded.data, b"abcxxxx!");
    assert!(decoded.diagnostics.is_empty());
}

#[test]
fn runs_reach_their_longest_lengths() {
    let mut data = vec![127];
    data.extend(0..128u8);
    data.extend([129, b'z', 128]);

    let decoded = run_length(&data);

    assert_eq!(decoded.data.len(), 128 + 128);
    assert!(decoded.data[128..].iter().all(|&b| b == b'z'));
}

#[test]
fn bytes_after_the_end_marker_are_ignored() {
    assert_eq!(run_length(&[0, b'a', 128, 5, 6]).data, b"a");
}

#[test]
fn run_length_data_without_an_end_marker_is_reported() {
    let decoded = run_length(&[1, b'o', b'k']);

    assert_eq!(decoded.data, b"ok");
    assert_eq!(kinds(&decoded), [DiagnosticKind::MissingEndOfDataMarker]);
}

#[test]
fn a_run_cut_short_keeps_its_bytes() {
    let literal = run_length(&[5, b'a', b'b']);
    let repeat = run_length(&[0, b'a', 200]);

    assert_eq!(literal.data, b"ab");
    assert_eq!(kinds(&literal), [DiagnosticKind::TruncatedFilterData]);
    assert_eq!(repeat.data, b"a");
    assert_eq!(kinds(&repeat), [DiagnosticKind::TruncatedFilterData]);
}

#[test]
fn run_length_stops_at_the_limit() {
    let data: Vec<u8> = [129, 0].repeat(1000);
    let limits = Limits {
        max_decoded_length: 1000,
        ..Limits::default()
    };

    let error = decode_run_length(&data, 2, limits).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::LimitExceeded(Limit::DecodedLength), 2)
    );
}

// In streams

#[test]
fn streams_decode_lzw_and_run_length() {
    for (entries, data) in [
        ("/Filter /LZWDecode", encode_lzw(b"BT (Hola) Tj ET", 1)),
        (
            "/Filter /LZWDecode /DecodeParms << /EarlyChange 0 >>",
            encode_lzw(b"BT (Hola) Tj ET", 0),
        ),
        (
            "/Filter /RunLengthDecode",
            [&[14][..], b"BT (Hola) Tj ET", &[128]].concat(),
        ),
    ] {
        let mut file =
            format!("1 0 obj << {entries} /Length {} >> stream\n", data.len()).into_bytes();
        file.extend_from_slice(&data);
        file.extend_from_slice(b"\nendstream endobj");
        let parsed = parse_indirect_object(&file, 0, Limits::default()).unwrap();
        let Object::Stream(stream) = parsed.object.object else {
            panic!("not a stream");
        };

        let decoded = decode_stream(&stream, &file, Limits::default()).unwrap();

        assert_eq!(decoded.data, b"BT (Hola) Tj ET", "{entries}");
    }
}
