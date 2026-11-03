//! Property tests: whatever the input, the lexer, the parsers, the
//! cross-reference table and stream readers and the Flate decoder must return a result or an error,
//! never panic or loop forever.

use ozaru_engine::base::Limits;
use ozaru_engine::filters::decode_flate;
use ozaru_engine::syntax::{Lexer, Object, Parser, parse_indirect_object, parse_object};
use ozaru_engine::xref::{find_startxref, parse_xref_stream, parse_xref_table};

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

fn check_flate(input: &[u8]) {
    let limits = Limits {
        max_decoded_length: 1024 * 1024,
        ..Limits::default()
    };
    let png = parse_object(b"<< /Predictor 12 /Colors 3 /Columns 2 >>", limits).unwrap();
    for parameters in [None, png.object.as_dictionary()] {
        match decode_flate(input, parameters, 0, limits) {
            Ok(decoded) => assert!(
                decoded.data.len() <= limits.max_decoded_length,
                "decoded past the limit in {input:?}"
            ),
            Err(error) => assert_eq!(error.offset(), 0, "wrong offset in {input:?}"),
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
