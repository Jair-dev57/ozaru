use miniz_oxide::deflate::{compress_to_vec, compress_to_vec_zlib};
use ozaru_engine::base::{DiagnosticKind, ErrorKind, Limit, Limits};
use ozaru_engine::filters::{Decoded, decode_flate, decode_stream};
use ozaru_engine::syntax::{Dictionary, Object, parse_indirect_object, parse_object};

fn decode(data: &[u8]) -> Decoded {
    decode_flate(data, None, 0, Limits::default()).expect("data should be valid")
}

fn decode_with(data: &[u8], parameters: &[u8]) -> Decoded {
    decode_flate(data, Some(&dictionary(parameters)), 0, Limits::default())
        .expect("data should be valid")
}

fn error_with(data: &[u8], parameters: &[u8]) -> ErrorKind {
    decode_flate(data, Some(&dictionary(parameters)), 0, Limits::default())
        .expect_err("should fail")
        .kind()
}

fn dictionary(input: &[u8]) -> Dictionary {
    parse_object(input, Limits::default())
        .expect("dictionary should be valid")
        .object
        .as_dictionary()
        .expect("object should be a dictionary")
        .clone()
}

fn diagnostics(decoded: &Decoded) -> Vec<(DiagnosticKind, usize)> {
    decoded
        .diagnostics
        .iter()
        .map(|d| (d.kind(), d.offset()))
        .collect()
}

// Text that doesn't repeat, so it compresses into many bytes.
fn varied_text(length: usize) -> Vec<u8> {
    let mut state: u32 = 1;
    (0..length)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            b'a' + (state >> 16) as u8 % 26
        })
        .collect()
}

// Flate

#[test]
fn zlib_data_is_decompressed() {
    let text = b"BT /F1 12 Tf 72 712 Td (Hello) Tj ET";

    let decoded = decode(&compress_to_vec_zlib(text, 6));

    assert_eq!(decoded.data, text);
    assert!(decoded.diagnostics.is_empty());
}

#[test]
fn every_compression_level_is_read() {
    let text = varied_text(5000);
    for level in 0..=10 {
        assert_eq!(decode(&compress_to_vec_zlib(&text, level)).data, text);
    }
}

#[test]
fn empty_data_decodes_to_nothing() {
    let decoded = decode(b"");

    assert!(decoded.data.is_empty());
    assert!(decoded.diagnostics.is_empty());
}

#[test]
fn raw_deflate_data_is_read_and_reported() {
    let decoded = decode_flate(
        &compress_to_vec(b"no zlib header", 6),
        None,
        40,
        Limits::default(),
    )
    .unwrap();

    assert_eq!(decoded.data, b"no zlib header");
    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::MissingZlibHeader, 40)]
    );
}

#[test]
fn data_cut_short_keeps_what_was_decoded() {
    let text = varied_text(20_000);
    let compressed = compress_to_vec_zlib(&text, 6);

    let decoded = decode(&compressed[..compressed.len() / 2]);

    assert!(!decoded.data.is_empty());
    assert!(decoded.data.len() < text.len());
    assert!(text.starts_with(&decoded.data));
    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::DamagedFlateData, 0)]
    );
}

#[test]
fn a_wrong_checksum_keeps_the_data_and_is_reported() {
    let mut compressed = compress_to_vec_zlib(b"checksum", 6);
    *compressed.last_mut().unwrap() ^= 0xFF;

    let decoded = decode(&compressed);

    assert_eq!(decoded.data, b"checksum");
    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::DamagedFlateData, 0)]
    );
}

#[test]
fn trailing_bytes_after_the_data_are_ignored() {
    let mut compressed = compress_to_vec_zlib(b"text", 6);
    compressed.extend_from_slice(b"\r\n");

    let decoded = decode(&compressed);

    assert_eq!(decoded.data, b"text");
    assert!(decoded.diagnostics.is_empty());
}

#[test]
fn data_that_cant_be_decoded_is_an_error() {
    for data in [&b"\x78\x9c\xff\xff\xff"[..], b"x", b"not compressed at all"] {
        let error = decode_flate(data, None, 12, Limits::default()).expect_err("should fail");

        assert_eq!(error.kind(), ErrorKind::InvalidFlateData, "{data:?}");
        assert_eq!(error.offset(), 12);
    }
}

#[test]
fn decompression_bombs_stop_at_the_limit() {
    let zeros = vec![0; 10 * 1024 * 1024];
    let compressed = compress_to_vec_zlib(&zeros, 9);
    assert!(compressed.len() < 20_000);
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };

    let error = decode_flate(&compressed, None, 7, limits).expect_err("should fail");

    assert_eq!(error.kind(), ErrorKind::LimitExceeded(Limit::DecodedLength));
    assert_eq!(error.offset(), 7);
    assert_eq!(decode(&compressed).data.len(), zeros.len());
}

#[test]
fn data_under_the_limit_is_decoded() {
    let text = varied_text(1000);
    let limits = Limits {
        max_decoded_length: 2000,
        ..Limits::default()
    };

    let decoded = decode_flate(&compress_to_vec_zlib(&text, 6), None, 0, limits).unwrap();

    assert_eq!(decoded.data, text);
}

// PNG predictors

// Applies a PNG filter to each row, as a PDF writer does before compressing.
fn predict(rows: &[&[u8]], filter: u8, pixel_length: usize) -> Vec<u8> {
    let mut output = Vec::new();
    let mut above: Vec<u8> = vec![0; rows[0].len()];
    for row in rows {
        output.push(filter);
        for i in 0..row.len() {
            let left = if i >= pixel_length {
                row[i - pixel_length]
            } else {
                0
            };
            let above_left = if i >= pixel_length {
                above[i - pixel_length]
            } else {
                0
            };
            let prediction = match filter {
                0 => 0,
                1 => left,
                2 => above[i],
                3 => ((u16::from(left) + u16::from(above[i])) / 2) as u8,
                _ => {
                    let estimate = i16::from(left) + i16::from(above[i]) - i16::from(above_left);
                    let distance = |v: u8| (estimate - i16::from(v)).abs();
                    if distance(left) <= distance(above[i])
                        && distance(left) <= distance(above_left)
                    {
                        left
                    } else if distance(above[i]) <= distance(above_left) {
                        above[i]
                    } else {
                        above_left
                    }
                }
            };
            output.push(row[i].wrapping_sub(prediction));
        }
        above = row.to_vec();
    }
    output
}

#[test]
fn each_png_filter_is_undone() {
    // Five RGB pixels per row.
    let rows: [&[u8]; 3] = [
        &[
            10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150,
        ],
        &[255, 0, 128, 1, 2, 3, 200, 201, 202, 7, 8, 9, 250, 251, 252],
        &[0, 0, 0, 255, 255, 255, 3, 99, 17, 64, 32, 16, 8, 4, 2],
    ];
    let expected = rows.concat();

    for filter in 0..=4 {
        let predicted = predict(&rows, filter, 3);
        let decoded = decode_with(
            &compress_to_vec_zlib(&predicted, 6),
            b"<< /Predictor 12 /Colors 3 /Columns 5 >>",
        );

        assert_eq!(decoded.data, expected, "filter {filter}");
        assert!(decoded.diagnostics.is_empty());
    }
}

#[test]
fn rows_can_use_different_filters() {
    let mut predicted = predict(&[&[1, 2, 3, 4]], 1, 1);
    predicted.extend(predict(&[&[1, 2, 3, 4], &[5, 6, 7, 8]], 2, 1)[5..].iter());

    let decoded = decode_with(
        &compress_to_vec_zlib(&predicted, 6),
        b"<< /Predictor 15 /Columns 4 >>",
    );

    assert_eq!(decoded.data, [1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn a_cross_reference_stream_with_the_up_filter_is_decoded() {
    // Three entries of /W [1 2 1], as PDF writers usually store them.
    let rows: [&[u8]; 3] = [&[0, 0, 0, 255], &[1, 0, 15, 0], &[1, 2, 76, 0]];
    let predicted = predict(&rows, 2, 1);

    let decoded = decode_with(
        &compress_to_vec_zlib(&predicted, 6),
        b"<< /Columns 4 /Predictor 12 >>",
    );

    assert_eq!(decoded.data, rows.concat());
}

#[test]
fn components_smaller_than_a_byte_use_one_byte_pixels() {
    // 12 one-bit pixels take two bytes per row.
    let rows: [&[u8]; 2] = [&[0b1010_1010, 0b1111_0000], &[0b0101_0101, 0b0000_1111]];

    for filter in 0..=4 {
        let decoded = decode_with(
            &compress_to_vec_zlib(&predict(&rows, filter, 1), 6),
            b"<< /Predictor 10 /BitsPerComponent 1 /Columns 12 >>",
        );

        assert_eq!(decoded.data, rows.concat(), "filter {filter}");
    }
}

#[test]
fn sixteen_bit_components_use_two_bytes_each() {
    let rows: [&[u8]; 2] = [&[1, 2, 3, 4], &[200, 100, 50, 25]];

    let decoded = decode_with(
        &compress_to_vec_zlib(&predict(&rows, 1, 2), 6),
        b"<< /Predictor 11 /BitsPerComponent 16 /Columns 2 >>",
    );

    assert_eq!(decoded.data, rows.concat());
}

#[test]
fn an_incomplete_last_row_is_decoded_as_far_as_it_goes() {
    let mut predicted = predict(&[&[1, 2, 3, 4], &[5, 6, 7, 8]], 2, 1);
    predicted.truncate(predicted.len() - 2);

    let decoded = decode_flate(
        &compress_to_vec_zlib(&predicted, 6),
        Some(&dictionary(b"<< /Predictor 12 /Columns 4 >>")),
        30,
        Limits::default(),
    )
    .unwrap();

    assert_eq!(decoded.data, [1, 2, 3, 4, 5, 6]);
    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::IncompletePredictorRow, 30)]
    );
}

#[test]
fn huge_columns_dont_allocate_a_huge_row() {
    let decoded = decode_with(
        &compress_to_vec_zlib(&[2, 1, 2, 3], 6),
        b"<< /Predictor 12 /Columns 1099511627776 >>",
    );

    assert_eq!(decoded.data, [1, 2, 3]);
    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::IncompletePredictorRow, 0)]
    );
}

#[test]
fn an_unknown_row_filter_is_an_error() {
    let data = compress_to_vec_zlib(&[5, 1, 2, 3], 6);

    assert_eq!(
        error_with(&data, b"<< /Predictor 12 /Columns 3 >>"),
        ErrorKind::InvalidPredictorData
    );
}

#[test]
fn no_predictor_leaves_the_data_unchanged() {
    let data = compress_to_vec_zlib(&[2, 1, 2, 3], 6);

    assert_eq!(decode_with(&data, b"<< >>").data, [2, 1, 2, 3]);
    assert_eq!(decode_with(&data, b"<< /Predictor 1 >>").data, [2, 1, 2, 3]);
}

// TIFF predictor

// Applies the TIFF predictor as a writer does: each component of a row, from the
// second pixel on, minus the same component of the pixel to its left.
fn predict_tiff(rows: &[Vec<u32>], colors: usize, bits: usize) -> Vec<u8> {
    let mask = (1u64 << bits) - 1;
    let mut output = Vec::new();
    for row in rows {
        let differences: Vec<u32> = (0..row.len())
            .map(|i| {
                let left = if i >= colors { row[i - colors] } else { 0 };
                ((u64::from(row[i]) + (1 << bits) - u64::from(left)) & mask) as u32
            })
            .collect();
        output.extend(pack(&differences, bits));
    }
    output
}

// Packs components of `bits` bits, most significant bit first, padding the row to
// a whole byte.
fn pack(components: &[u32], bits: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let (mut buffer, mut count) = (0u64, 0);
    for &component in components {
        buffer = buffer << bits | u64::from(component);
        count += bits;
        while count >= 8 {
            count -= 8;
            bytes.push((buffer >> count) as u8);
        }
    }
    if count > 0 {
        bytes.push((buffer << (8 - count)) as u8);
    }
    bytes
}

// Rows of `colors` components per pixel and 5 pixels, with values that use every
// bit available.
fn sample_rows(colors: usize, bits: usize) -> Vec<Vec<u32>> {
    let mut state: u64 = 99;
    (0..3)
        .map(|_| {
            (0..colors * 5)
                .map(|_| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    ((state >> 33) & ((1 << bits) - 1)) as u32
                })
                .collect()
        })
        .collect()
}

#[test]
fn the_tiff_predictor_is_undone_at_every_depth() {
    for bits in [1, 2, 4, 8, 16] {
        for colors in [1, 3] {
            let rows = sample_rows(colors, bits);
            let expected: Vec<u8> = rows.iter().flat_map(|row| pack(row, bits)).collect();
            let parameters =
                format!("<< /Predictor 2 /Colors {colors} /BitsPerComponent {bits} /Columns 5 >>");

            let decoded = decode_with(
                &compress_to_vec_zlib(&predict_tiff(&rows, colors, bits), 6),
                parameters.as_bytes(),
            );

            assert_eq!(decoded.data, expected, "{bits} bits, {colors} colors");
            assert!(decoded.diagnostics.is_empty());
        }
    }
}

#[test]
fn the_tiff_predictor_keeps_the_first_pixel_and_wraps_around() {
    // Gray, 8 bits: 200, then +100 wraps to 44, then -50.
    let data = compress_to_vec_zlib(&[200, 100, 206], 6);

    let decoded = decode_with(&data, b"<< /Predictor 2 /Columns 3 >>");

    assert_eq!(decoded.data, [200, 44, 250]);
}

#[test]
fn the_tiff_predictor_restarts_on_each_row() {
    let data = compress_to_vec_zlib(&[10, 1, 20, 2], 6);

    let decoded = decode_with(&data, b"<< /Predictor 2 /Columns 2 >>");

    assert_eq!(decoded.data, [10, 11, 20, 22]);
}

#[test]
fn padding_bits_of_a_tiff_row_are_left_alone() {
    // Three 2-bit components and two padding bits per row.
    let data = compress_to_vec_zlib(&[0b01_01_01_11], 6);

    let decoded = decode_with(&data, b"<< /Predictor 2 /BitsPerComponent 2 /Columns 3 >>");

    assert_eq!(decoded.data, [0b01_10_11_11]);
}

#[test]
fn an_incomplete_last_tiff_row_is_decoded_as_far_as_it_goes() {
    let data = compress_to_vec_zlib(&[1, 1, 1, 5, 5], 6);

    let decoded = decode_flate(
        &data,
        Some(&dictionary(b"<< /Predictor 2 /Columns 3 >>")),
        8,
        Limits::default(),
    )
    .unwrap();

    assert_eq!(decoded.data, [1, 2, 3, 5, 10]);
    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::IncompletePredictorRow, 8)]
    );
}

#[test]
fn the_tiff_predictor_works_with_lzw_too() {
    let mut file = b"1 0 obj << /Filter /LZWDecode /DecodeParms << /Predictor 2 /Columns 3 >> /Length 6 >> stream\n".to_vec();
    // 256, 1, 1, 1, 257 in 9-bit codes: the bytes 1 1 1.
    file.extend_from_slice(&[0x80, 0x00, 0x40, 0x20, 0x18, 0x08]);
    file.extend_from_slice(b"\nendstream endobj");

    let decoded = decode_file(&file).unwrap();

    assert_eq!(decoded.data, [1, 2, 3]);
}

#[test]
fn invalid_predictor_parameters_are_an_error() {
    let data = compress_to_vec_zlib(b"data", 6);
    for parameters in [
        &b"<< /Predictor 9 >>"[..],
        b"<< /Predictor 16 >>",
        b"<< /Predictor -1 >>",
        b"<< /Predictor /Up >>",
        b"<< /Predictor 12 /Columns 0 >>",
        b"<< /Predictor 12 /Columns 1.5 >>",
        b"<< /Predictor 12 /Colors 0 >>",
        b"<< /Predictor 12 /BitsPerComponent 3 >>",
        b"<< /Predictor 12 /Colors 4294967296 /Columns 4294967296 /BitsPerComponent 16 >>",
    ] {
        assert_eq!(
            error_with(&data, parameters),
            ErrorKind::InvalidDecodeParameters,
            "{}",
            String::from_utf8_lossy(parameters)
        );
    }
}

#[test]
fn parameters_are_checked_before_decompressing() {
    assert_eq!(
        error_with(b"not compressed", b"<< /Predictor 99 >>"),
        ErrorKind::InvalidDecodeParameters
    );
}

// Streams

// The stream of `1 0 obj` with `entries` in its dictionary, and the file it's in.
fn stream_file(entries: &str, data: &[u8]) -> Vec<u8> {
    let mut file = format!("1 0 obj << {entries} /Length {} >> stream\n", data.len()).into_bytes();
    file.extend_from_slice(data);
    file.extend_from_slice(b"\nendstream endobj");
    file
}

fn data_start(file: &[u8]) -> usize {
    file.windows(7).position(|w| w == b"stream\n").unwrap() + 7
}

fn decode_file(file: &[u8]) -> ozaru_engine::base::Result<Decoded> {
    let parsed = parse_indirect_object(file, 0, Limits::default()).unwrap();
    let Object::Stream(stream) = parsed.object.object else {
        panic!("not a stream");
    };
    decode_stream(&stream, file, Limits::default())
}

#[test]
fn a_stream_without_filters_is_returned_as_it_is() {
    let file = stream_file("", b"raw bytes");

    assert_eq!(decode_file(&file).unwrap().data, b"raw bytes");
}

#[test]
fn a_flate_stream_is_decoded_with_its_parameters() {
    let predicted = predict(&[&[1, 2], &[3, 4]], 2, 1);
    let file = stream_file(
        "/Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 2 >>",
        &compress_to_vec_zlib(&predicted, 6),
    );

    assert_eq!(decode_file(&file).unwrap().data, [1, 2, 3, 4]);
}

#[test]
fn chained_filters_are_applied_in_order() {
    let twice = compress_to_vec_zlib(&compress_to_vec_zlib(b"twice", 6), 6);
    let file = stream_file("/Filter [/FlateDecode /FlateDecode]", &twice);

    assert_eq!(decode_file(&file).unwrap().data, b"twice");
}

#[test]
fn diagnostics_of_each_filter_are_kept() {
    let mut data = compress_to_vec_zlib(b"checksum", 6);
    *data.last_mut().unwrap() ^= 0xFF;
    let file = stream_file("/Filter /FlateDecode", &data);

    let decoded = decode_file(&file).unwrap();

    assert_eq!(
        diagnostics(&decoded),
        [(DiagnosticKind::DamagedFlateData, data_start(&file))]
    );
}

#[test]
fn unsupported_filters_are_an_error_at_the_data() {
    // Image filters come with images in phase 10, and Crypt with decryption.
    for filter in [
        "/DCTDecode",
        "/JPXDecode",
        "/CCITTFaxDecode",
        "/JBIG2Decode",
        "/Crypt",
        "/Unknown",
        "42",
    ] {
        let file = stream_file(&format!("/Filter {filter}"), b"data");

        let error = decode_file(&file).unwrap_err();

        assert_eq!(
            (error.kind(), error.offset()),
            (ErrorKind::UnsupportedFilter, data_start(&file)),
            "{filter}"
        );
    }
}

#[test]
fn decode_parameters_must_match_the_filters() {
    let data = compress_to_vec_zlib(b"data", 6);
    for entries in [
        "/Filter /FlateDecode /DecodeParms [<< >> << >>]",
        "/Filter /FlateDecode /DecodeParms 12",
        "/DecodeParms << >>",
    ] {
        let file = stream_file(entries, &data);

        assert_eq!(
            decode_file(&file).unwrap_err().kind(),
            ErrorKind::InvalidDecodeParameters,
            "{entries}"
        );
    }
}
