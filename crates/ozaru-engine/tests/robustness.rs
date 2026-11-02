//! Property tests: whatever the input, the lexer and the parser must return a
//! result or an error, never panic or loop forever.

use ozaru_engine::base::Limits;
use ozaru_engine::syntax::{Lexer, Parser};

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

fn run(seed: u64, generate: fn(&mut Random) -> Vec<u8>) {
    let mut random = Random(seed);
    for _ in 0..CASES_PER_GENERATOR {
        let input = generate(&mut random);
        check_lexer(&input);
        check_parser(&input);
    }
}

#[test]
fn random_bytes_never_break_the_lexer_or_the_parser() {
    run(0x0A2A_0001, random_bytes);
}

#[test]
fn random_pdf_syntax_never_breaks_the_lexer_or_the_parser() {
    run(0x0A2A_0002, random_syntax);
}
