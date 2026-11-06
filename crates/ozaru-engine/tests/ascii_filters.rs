use miniz_oxide::deflate::compress_to_vec_zlib;
use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::filters::{Decoded, decode_ascii_hex, decode_ascii85, decode_stream};
use ozaru_engine::syntax::{Object, parse_indirect_object};

fn hex(data: &[u8]) -> Decoded {
    decode_ascii_hex(data, 0, Limits::default()).expect("data should be valid")
}

fn a85(data: &[u8]) -> Decoded {
    decode_ascii85(data, 0, Limits::default()).expect("data should be valid")
}

fn a85_error(data: &[u8]) -> (ErrorKind, usize) {
    let error = decode_ascii85(data, 9, Limits::default()).expect_err("should fail");
    (error.kind(), error.offset())
}

fn kinds(decoded: &Decoded) -> Vec<DiagnosticKind> {
    decoded.diagnostics.iter().map(|d| d.kind()).collect()
}

// The ASCII85 encoding of the standard, to check round trips on any bytes.
fn encode_ascii85(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for chunk in data.chunks(4) {
        let mut group = [0u8; 4];
        group[..chunk.len()].copy_from_slice(chunk);
        let value = u32::from_be_bytes(group);
        if value == 0 && chunk.len() == 4 {
            output.push(b'z');
            continue;
        }
        let mut digits = [0u8; 5];
        let mut rest = value;
        for digit in digits.iter_mut().rev() {
            *digit = (rest % 85) as u8 + b'!';
            rest /= 85;
        }
        output.extend_from_slice(&digits[..chunk.len() + 1]);
    }
    output.extend_from_slice(b"~>");
    output
}

// ASCIIHexDecode

#[test]
fn hex_pairs_become_bytes() {
    let decoded = hex(b"48656C6C6F>");

    assert_eq!(decoded.data, b"Hello");
    assert!(decoded.diagnostics.is_empty());
}

#[test]
fn hex_digits_can_be_lowercase_and_spaced() {
    assert_eq!(hex(b"48 65\n6c\t6C 6f >").data, b"Hello");
}

#[test]
fn a_last_hex_digit_alone_counts_as_followed_by_zero() {
    // The example of the standard: `7>` is the byte 0x70.
    assert_eq!(hex(b"901FA>").data, [0x90, 0x1F, 0xA0]);
    assert_eq!(hex(b"7>").data, [0x70]);
}

#[test]
fn bytes_after_the_hex_end_marker_are_ignored() {
    assert_eq!(hex(b"41>junk").data, b"A");
    assert_eq!(hex(b">").data, b"");
}

#[test]
fn hex_data_without_an_end_marker_is_decoded_and_reported() {
    let decoded = decode_ascii_hex(b"4142", 30, Limits::default()).unwrap();

    assert_eq!(decoded.data, b"AB");
    assert_eq!(
        decoded
            .diagnostics
            .iter()
            .map(|d| (d.kind(), d.offset()))
            .collect::<Vec<_>>(),
        [(DiagnosticKind::MissingEndOfDataMarker, 30)]
    );
}

#[test]
fn a_character_that_isnt_hex_is_an_error() {
    for data in [&b"4G>"[..], b"41 ~>", b"<41>"] {
        let error = decode_ascii_hex(data, 9, Limits::default()).unwrap_err();

        assert_eq!(
            (error.kind(), error.offset()),
            (ErrorKind::InvalidAsciiHexData, 9)
        );
    }
}

// ASCII85Decode

#[test]
fn ascii85_matches_a_reference_encoder() {
    // Vectors from Python's base64.a85encode.
    for (text, encoded) in [
        (&b"Hello World"[..], &b"87cURD]i,\"Ebo7~>"[..]),
        (b"Man is distinguished", b"9jqo^BlbD-BleB1DJ+*+F(f,q~>"),
        (b"\xff\xff\xff\xff", b"s8W-!~>"),
        (b"a", b"@/~>"),
        (b"ab", b"@:B~>"),
        (b"abc", b"@:E^~>"),
    ] {
        let decoded = a85(encoded);

        assert_eq!(decoded.data, text, "{}", String::from_utf8_lossy(encoded));
        assert!(decoded.diagnostics.is_empty());
    }
}

#[test]
fn z_stands_for_four_zero_bytes() {
    assert_eq!(a85(b"z~>").data, [0; 4]);
    assert_eq!(a85(b"zz@/~>").data, [0, 0, 0, 0, 0, 0, 0, 0, b'a']);
}

#[test]
fn ascii85_ignores_whitespace_and_a_leading_marker() {
    assert_eq!(a85(b"<~87cUR D]i,\"\nEbo7 ~ >").data, b"Hello World");
}

#[test]
fn any_bytes_survive_a_round_trip() {
    let mut state: u32 = 7;
    for length in 0..64 {
        let data: Vec<u8> = (0..length)
            .map(|i| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                if i % 7 < 4 { 0 } else { (state >> 16) as u8 }
            })
            .collect();

        assert_eq!(a85(&encode_ascii85(&data)).data, data, "{data:?}");
    }
}

#[test]
fn ascii85_data_without_an_end_marker_is_decoded_and_reported() {
    let decoded = a85(b"87cURD]i,\"Ebo7");

    assert_eq!(decoded.data, b"Hello World");
    assert_eq!(kinds(&decoded), [DiagnosticKind::MissingEndOfDataMarker]);
}

#[test]
fn bytes_after_the_ascii85_end_marker_are_ignored() {
    assert_eq!(a85(b"@/~>garbage{}").data, b"a");
}

#[test]
fn invalid_ascii85_data_is_an_error() {
    for data in [
        &b"87cvR~>"[..],
        b"@/{~>",
        b"@z/~>",
        b"s8W-\"~>",
        b"uuuuu~>",
        b"@~>",
        b"@/~x",
        b"@/~",
    ] {
        assert_eq!(
            a85_error(data),
            (ErrorKind::InvalidAscii85Data, 9),
            "{}",
            String::from_utf8_lossy(data)
        );
    }
}

#[test]
fn many_zs_stop_at_the_size_limit() {
    let data = vec![b'z'; 1000];
    let limits = Limits {
        max_decoded_length: 100,
        ..Limits::default()
    };

    let error = decode_ascii85(&data, 3, limits).unwrap_err();

    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::LimitExceeded(Limit::DecodedLength), 3)
    );
}

// In streams

fn decode_file(entries: &str, data: &[u8]) -> ozaru_engine::base::Result<Decoded> {
    let mut file = format!("1 0 obj << {entries} /Length {} >> stream\n", data.len()).into_bytes();
    file.extend_from_slice(data);
    file.extend_from_slice(b"\nendstream endobj");
    let parsed = parse_indirect_object(&file, 0, Limits::default()).unwrap();
    let Object::Stream(stream) = parsed.object.object else {
        panic!("not a stream");
    };
    decode_stream(&stream, &file, Limits::default())
}

#[test]
fn streams_decode_ascii_filters() {
    assert_eq!(
        decode_file("/Filter /ASCIIHexDecode", b"48656C6C6F>")
            .unwrap()
            .data,
        b"Hello"
    );
    assert_eq!(
        decode_file("/Filter /ASCII85Decode", b"87cURD]i,\"Ebo7~>")
            .unwrap()
            .data,
        b"Hello World"
    );
}

#[test]
fn ascii85_can_wrap_flate_data() {
    // How some writers keep a compressed stream as plain text.
    let compressed = compress_to_vec_zlib(b"BT (Hola) Tj ET", 6);
    let file = decode_file(
        "/Filter [/ASCII85Decode /FlateDecode] /DecodeParms [null null]",
        &encode_ascii85(&compressed),
    );

    assert_eq!(file.unwrap().data, b"BT (Hola) Tj ET");
}
