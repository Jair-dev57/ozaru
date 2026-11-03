//! Property tests: whatever the input, the lexer, the parsers, the
//! cross-reference readers and repair, the object stream reader, the resolver, the
//! filters and the security handler must return a result or an error, never panic or loop forever.

use ozaru_engine::base::Limits;
use ozaru_engine::filters::{
    decode_ascii_hex, decode_ascii85, decode_flate, decode_lzw, decode_run_length,
};
use ozaru_engine::security::StandardSecurity;
use ozaru_engine::syntax::ObjectId;
use ozaru_engine::syntax::{Lexer, Object, Parser, parse_indirect_object, parse_object};
use ozaru_engine::xref::{
    Resolver, XrefEntry, find_startxref, load_xref, parse_object_stream, parse_xref_stream,
    parse_xref_table, rebuild_xref,
};

const CASES_PER_GENERATOR: usize = 50_000;

// Pieces of PDF syntax, so random inputs reach deep into the parser instead of
// failing on the first byte.
const FRAGMENTS: &[&[u8]] = &[
    b"<<",
    b">>",
    b"[",
    b"]",
    b"{",
    b"}",
    b"(",
    b")",
    b"<",
    b">",
    b"/",
    b"%",
    b"\\",
    b"#",
    b"0",
    b"1",
    b"7",
    b"R",
    b" ",
    b"\n",
    b"\r",
    b"\t",
    b"\0",
    b"-",
    b"+",
    b".",
    b"true",
    b"false",
    b"null",
    b"obj",
    b"41",
    b"(a\\",
    b"\\(",
    b"<4F",
    b"/A#2",
    b"#20",
    b"\\0",
    b"\\377",
    b"9223372036854775807",
    b"9223372036854775808",
    b"\xff",
    b"\x80",
    b"1 0 obj ",
    b"endobj",
    b"stream",
    b"stream\n",
    b"stream\r\n",
    b"endstream",
    b"/Length",
    b"/Length 3",
    b"/Length 0",
    b"/Length -1",
    b"/Length 5 0 R",
    b"xref",
    b"xref\n",
    b"trailer",
    b"startxref\n",
    b"%%EOF",
    b"0 3\n",
    b"0000000017 00000 n \n",
    b"0000000000 65535 f\r\n",
    b"65535",
    b"4294967295",
    b" n",
    b" f",
    b"\x78\x9c",
    b"\x78\x01",
    b"\x03\x00",
    b"/Type /XRef",
    b"/W [1 2 1]",
    b"/W [0 8 0]",
    b"/Index [0 2]",
    b"/Size 3",
    b"/Filter /FlateDecode",
    b"/DecodeParms << /Predictor 12 /Columns 4 >>",
    b"~>",
    b"z",
    b"<~",
    b"9jqo^",
    b"4F6C>",
    b"/Filter /Standard",
    b"/V 4 /R 4",
    b"/V 2 /R 3 /Length 128",
    b"/O <0ba3835f88f90388e74e54584125ce142be0de24c6b0d37746e075b891756671>",
    b"/P -3904",
    b"/CF << /StdCF << /CFM /AESV2 >> >> /StmF /StdCF",
    b"/Prev 0",
    b"/Prev 9",
    b"/XRefStm 0",
];

/// A small deterministic random generator (xorshift64*), so failures can be
/// reproduced from the seed.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

fn random_bytes(random: &mut Random) -> Vec<u8> {
    let length = random.below(64);
    (0..length).map(|_| random.next() as u8).collect()
}

fn random_syntax(random: &mut Random) -> Vec<u8> {
    let pieces = random.below(40);
    (0..pieces)
        .flat_map(|_| FRAGMENTS[random.below(FRAGMENTS.len())].iter().copied())
        .collect()
}

fn check_lexer(input: &[u8]) {
    let mut lexer = Lexer::new(input, Limits::default());
    let mut previous_end = 0;
    loop {
        match lexer.next_token() {
            Ok(Some(token)) => {
                assert!(previous_end <= token.start, "tokens overlap in {input:?}");
                assert!(token.start < token.end, "empty token in {input:?}");
                assert!(token.end <= input.len(), "token past the end in {input:?}");
                previous_end = token.end;
            }
            Ok(None) => break,
            Err(error) => {
                assert!(
                    error.offset() <= input.len(),
                    "error past the end in {input:?}"
                );
                break;
            }
        }
    }
}

fn check_parser(input: &[u8]) {
    let mut parser = Parser::new(input, Limits::default());
    let mut previous_position = 0;
    loop {
        match parser.next_object() {
            Ok(Some(_)) => {
                let position = parser.position();
                assert!(position > previous_position, "no progress in {input:?}");
                assert!(
                    position <= input.len(),
                    "position past the end in {input:?}"
                );
                previous_position = position;
            }
            Ok(None) => break,
            Err(error) => {
                assert!(
                    error.offset() <= input.len(),
                    "error past the end in {input:?}"
                );
                break;
            }
        }
    }
}

fn check_indirect_object(input: &[u8]) {
    match parse_indirect_object(input, 0, Limits::default()) {
        Ok(parsed) => {
            assert!(
                parsed.end > 0 && parsed.end <= input.len(),
                "bad end in {input:?}"
            );
            if let Object::Stream(stream) = &parsed.object.object {
                let range = stream.data_range();
                assert!(range.start <= range.end, "bad stream range in {input:?}");
                assert!(
                    range.end <= parsed.end,
                    "stream past the object in {input:?}"
                );
            }
        }
        Err(error) => assert!(
            error.offset() <= input.len(),
            "error past the end in {input:?}"
        ),
    }
}

fn check_xref(input: &[u8]) {
    match find_startxref(input, Limits::default()) {
        Ok(found) => assert!(
            found.offset < input.len(),
            "startxref past the end in {input:?}"
        ),
        Err(error) => assert!(
            error.offset() <= input.len(),
            "error past the end in {input:?}"
        ),
    }
    match load_xref(input, Limits::default()) {
        Ok(loaded) => assert!(loaded.sections > 0, "no sections in {input:?}"),
        Err(error) => assert!(
            error.offset() <= input.len(),
            "error past the end in {input:?}"
        ),
    }
    if let Ok(rebuilt) = rebuild_xref(input, Limits::default()) {
        for (_, entry) in rebuilt.table.iter() {
            if let XrefEntry::InUse { offset, .. } = entry {
                assert!(offset < input.len(), "entry past the end in {input:?}");
            }
        }
    }
    check_resolver(input);
    for parse in [parse_xref_table, parse_xref_stream] {
        match parse(input, 0, Limits::default()) {
            Ok(parsed) => assert!(
                parsed.end <= input.len(),
                "section past the end in {input:?}"
            ),
            Err(error) => assert!(
                error.offset() <= input.len(),
                "error past the end in {input:?}"
            ),
        }
    }
}

// Opens the input as a whole file and resolves every object it lists, plus a few
// it doesn't. Errors and diagnostics must stay inside the input.
fn check_resolver(input: &[u8]) {
    let Ok(mut resolver) = Resolver::open(input, Limits::default()) else {
        return;
    };
    let listed: Vec<u32> = resolver.table().iter().map(|(n, _)| n).take(200).collect();
    for number in listed.into_iter().chain(0..8) {
        for generation in [0, 1] {
            if let Err(error) = resolver.resolve(ObjectId::new(number, generation)) {
                assert!(
                    error.offset() <= input.len(),
                    "error past the end: {error:?}"
                );
            }
        }
    }
    assert!(
        resolver
            .diagnostics()
            .iter()
            .all(|d| d.offset() <= input.len()),
        "diagnostic past the end: {:?}",
        resolver.diagnostics()
    );
}

fn check_text_and_run_filters(input: &[u8]) {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    let lzw = |input: &[u8], offset, limits| decode_lzw(input, None, offset, limits);
    for decode in [decode_ascii_hex, decode_ascii85, decode_run_length, lzw] {
        match decode(input, 0, limits) {
            Ok(decoded) => assert!(
                decoded.data.len() <= limits.max_decoded_length,
                "decoded too much from {input:?}"
            ),
            Err(error) => assert_eq!(error.offset(), 0, "wrong offset in {input:?}"),
        }
    }
}

fn check_flate(input: &[u8]) {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    let png = parse_object(b"<< /Predictor 12 /Colors 3 /Columns 2 >>", limits).unwrap();
    let tiff = parse_object(b"<< /Predictor 2 /BitsPerComponent 4 /Columns 3 >>", limits).unwrap();
    for parameters in [
        None,
        png.object.as_dictionary(),
        tiff.object.as_dictionary(),
    ] {
        match decode_flate(input, parameters, 0, limits) {
            Ok(decoded) => assert!(
                decoded.data.len() <= limits.max_decoded_length,
                "decoded past the limit in {input:?}"
            ),
            Err(error) => assert_eq!(error.offset(), 0, "wrong offset in {input:?}"),
        }
    }
}

// Uses the input as the data of an object stream, with a /First inside it.
fn check_object_stream(input: &[u8]) {
    let mut file = format!(
        "1 0 obj << /Type /ObjStm /N 4 /First {} /Length {} >> stream\n",
        input.len() / 3,
        input.len()
    )
    .into_bytes();
    file.extend_from_slice(input);
    file.extend_from_slice(b"\nendstream endobj");
    let parsed = parse_indirect_object(&file, 0, Limits::default()).unwrap();
    let stream = parsed.object.object.as_stream().unwrap();

    match parse_object_stream(stream, &file, Limits::default()) {
        Ok(parsed) => {
            assert!(parsed.stream.len() <= 4, "too many objects in {input:?}");
            for index in 0..6 {
                if let Err(error) = parsed.stream.object(index) {
                    assert!(
                        error.offset() <= file.len(),
                        "error past the end in {input:?}"
                    );
                }
            }
        }
        Err(error) => assert!(
            error.offset() <= file.len(),
            "error past the end in {input:?}"
        ),
    }
}

// Reads the input as an /Encrypt dictionary, with itself as the file ID.
fn check_security(input: &[u8]) {
    let Ok(parsed) = parse_object(input, Limits::default()) else {
        return;
    };
    let Some(encrypt) = parsed.object.as_dictionary() else {
        return;
    };
    if let Ok(security) = StandardSecurity::new(encrypt, input, 0) {
        let _ = security.authenticate(input);
        if let Some(key) = security.authenticate(b"") {
            let mut object = parsed.object.clone();
            let id = ObjectId::new(1, 0);
            let _ = security.decrypt_object(&key, id, &mut object, 0);
            let _ = security.decrypt_stream_data(&key, id, input, 0);
        }
    }
}

fn run(seed: u64, generate: fn(&mut Random) -> Vec<u8>) {
    let mut random = Random(seed);
    for _ in 0..CASES_PER_GENERATOR {
        let input = generate(&mut random);
        check_lexer(&input);
        check_parser(&input);
        check_indirect_object(&input);
        check_xref(&input);
        check_flate(&input);
        check_text_and_run_filters(&input);
        check_object_stream(&input);
        check_security(&input);
    }
}

// Starts with a valid header, so most inputs reach the object and stream code.
fn random_indirect_object(random: &mut Random) -> Vec<u8> {
    let mut input = b"1 0 obj ".to_vec();
    input.extend(random_syntax(random));
    input
}

// A valid zlib stream with some bytes changed, so the decoder gets far before it
// finds the damage.
fn damaged_flate(random: &mut Random) -> Vec<u8> {
    let text = random_syntax(random);
    let mut input = miniz_oxide::deflate::compress_to_vec_zlib(&text, random.below(10) as u8);
    for _ in 0..random.below(4) {
        let index = random.below(input.len());
        input[index] = random.next() as u8;
    }
    input.truncate(input.len() - random.below(input.len() / 2 + 1));
    input
}

// A cross-reference stream with random entries in its dictionary and random data,
// sometimes compressed.
fn random_xref_stream(random: &mut Random) -> Vec<u8> {
    let entries = random_syntax(random);
    let mut data: Vec<u8> = (0..random.below(64)).map(|_| random.next() as u8).collect();
    if random.below(2) == 0 {
        data = miniz_oxide::deflate::compress_to_vec_zlib(&data, 6);
    }
    let mut input = b"1 0 obj << /Type /XRef ".to_vec();
    input.extend(entries);
    input.extend(format!(" /Length {} >> stream\n", data.len()).bytes());
    input.extend(data);
    input.extend(b"\nendstream endobj");
    input
}

// A small file whose sections point at random offsets through /Prev and
// /XRefStm, to reach the chain code.
fn random_xref_chain(random: &mut Random) -> Vec<u8> {
    let mut input = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for _ in 0..1 + random.below(4) {
        offsets.push(input.len());
        input.extend(b"xref\n0 1\n0000000000 65535 f \ntrailer << ");
        for key in ["/Prev", "/XRefStm"] {
            if random.below(2) == 0 {
                let target = offsets[random.below(offsets.len())] + random.below(3);
                input.extend(format!("{key} {target} ").bytes());
            }
        }
        input.extend(random_syntax(random).iter().take(8));
        input.extend(b" >>\n");
    }
    let start = offsets[1 + random.below(offsets.len() - 1)];
    input.extend(format!("startxref\n{start}\n%%EOF\n").bytes());
    input
}

// Starts like a cross-reference table, so most inputs reach the entries.
fn random_xref_table(random: &mut Random) -> Vec<u8> {
    let mut input = b"xref\n0 ".to_vec();
    input.extend(random_syntax(random));
    input
}

#[test]
fn random_bytes_never_break_the_lexer_or_the_parser() {
    run(0x0A2A_0001, random_bytes);
}

#[test]
fn random_pdf_syntax_never_breaks_the_lexer_or_the_parser() {
    run(0x0A2A_0002, random_syntax);
}

#[test]
fn random_indirect_objects_never_break_the_parsers() {
    run(0x0A2A_0003, random_indirect_object);
}

#[test]
fn random_xref_tables_never_break_the_readers() {
    run(0x0A2A_0004, random_xref_table);
}

#[test]
fn damaged_flate_data_never_breaks_the_decoder() {
    run(0x0A2A_0005, damaged_flate);
}

#[test]
fn random_xref_streams_never_break_the_readers() {
    run(0x0A2A_0006, random_xref_stream);
}

#[test]
fn random_xref_chains_never_break_the_loader() {
    run(0x0A2A_0007, random_xref_chain);
}

// The real PDFs of the repository with a few bytes changed, removed or set to
// PDF syntax, so the inputs reach deep into the cross-reference and resolver code.
fn mutated_pdf(random: &mut Random) -> Vec<u8> {
    const FILES: [&str; 4] = [
        "../../docs/assets/report.pdf",
        "tests/files/report-flate.pdf",
        "tests/files/report-xrefstm.pdf",
        "../../tests/damaged.pdf",
    ];
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FILES[random.below(FILES.len())]);
    let mut input = std::fs::read(path).expect("file should be readable");
    for _ in 0..1 + random.below(8) {
        let index = random.below(input.len());
        match random.below(3) {
            0 => input[index] = random.next() as u8,
            1 => input[index] = b"0123456789 R/<>[]"[random.below(17)],
            _ => {
                input.remove(index);
            }
        }
    }
    input
}

#[test]
fn mutated_real_pdfs_never_break_the_resolver() {
    let mut random = Random(0x0A2A_0008);
    for _ in 0..CASES_PER_GENERATOR / 5 {
        check_resolver(&mutated_pdf(&mut random));
    }
}
