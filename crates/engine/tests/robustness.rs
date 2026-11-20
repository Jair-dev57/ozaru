//! Property tests: whatever the input, the lexer, the parsers, the
//! cross-reference readers and repair, the object stream reader, the resolver, the
//! filters, the security handler and the document reader must return a result or
//! an error, never panic or loop forever.

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
use ozaru_engine::{Document, OpenOptions};

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
    b"/V 5 /R 6",
    b"/UE <20838853b2a91db91049a4aad8d2ce985e5a8bb1c1dff18b2e55c855eb66041f>",
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
            let mut diagnostics = ozaru_engine::base::Diagnostics::new(100);
            let _ = security.decrypt_object(&key, id, &mut object, 0, &mut diagnostics);
            let _ = security.decrypt_stream_data(&key, id, input, 0, &mut diagnostics);
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
    const FILES: [&str; 5] = [
        "../../docs/assets/report.pdf",
        "tests/files/report-flate.pdf",
        "tests/files/report-xrefstm.pdf",
        "../../tests/damaged.pdf",
        "tests/files/report-rc4-128.pdf",
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

// Opens the input as a document, locked and then with the password "user", and
// checks what it reads: boxes with area, crop boxes inside media boxes, rotations
// in 0 to 270, and errors and diagnostics inside the input.
fn check_document(input: &[u8]) {
    let limits = Limits {
        max_pages: 10_000,
        ..Limits::default()
    };
    let options = OpenOptions {
        limits,
        password: None,
    };
    let mut document = match Document::open_with(input, options) {
        Ok(document) => document,
        Err(error) => {
            assert!(
                error.offset() <= input.len(),
                "error past the end: {error:?}"
            );
            return;
        }
    };
    check_open_document(&document, input.len());
    if document.unlock(b"user") {
        check_open_document(&document, input.len());
    }
}

fn check_open_document(document: &Document<'_>, length: usize) {
    for page in document.pages() {
        let media_box = page.media_box();
        assert!(
            media_box.width() > 0.0 && media_box.height() > 0.0,
            "{media_box:?}"
        );
        let crop_box = page.crop_box();
        assert!(
            crop_box.left >= media_box.left
                && crop_box.right <= media_box.right
                && crop_box.bottom >= media_box.bottom
                && crop_box.top <= media_box.top,
            "{crop_box:?} outside {media_box:?}"
        );
        assert!(page.rotation() % 90 == 0 && page.rotation() < 360);
    }
    if let Some(info) = document.info() {
        let _ = (info.title(), info.creation_date(), info.trapped());
    }
    let diagnostics = document.diagnostics();
    assert!(
        diagnostics.iter().all(|d| d.offset() <= length),
        "diagnostic past the end: {diagnostics:?}"
    );
}

#[test]
fn mutated_real_pdfs_never_break_the_document_reader() {
    let mut random = Random(0x0A2A_0009);
    for _ in 0..CASES_PER_GENERATOR / 10 {
        check_document(&mutated_pdf(&mut random));
    }
}

#[test]
fn real_pdfs_cut_short_never_break_the_document_reader() {
    for name in [
        "../../docs/assets/report.pdf",
        "../../docs/assets/damaged-page.pdf",
        "tests/files/report-xrefstm.pdf",
        "tests/files/report-user-password.pdf",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let file = std::fs::read(path).expect("file should be readable");
        for end in 0..file.len() {
            check_document(&file[..end]);
        }
    }
}

// Page tree entries, valid and broken, for random page trees.
const PAGE_TREE_PIECES: &[&str] = &[
    "/Type /Pages",
    "/Type /Page",
    "/Type /Catalog",
    "/Type /Font",
    "/Kids []",
    "/Kids 3",
    "/Count 2",
    "/Count -1",
    "/Count 99999999999",
    "/Parent 2 0 R",
    "/MediaBox [0 0 612 792]",
    "/MediaBox [612 792 0 0]",
    "/MediaBox [0 0 0 0]",
    "/MediaBox [0 0 1e400 5]",
    "/MediaBox [-5 -5 5]",
    "/MediaBox 4 0 R",
    "/CropBox [10 10 20 20]",
    "/CropBox [1000 1000 2000 2000]",
    "/Rotate 90",
    "/Rotate -270",
    "/Rotate 45",
    "/Rotate 99999999999999999999",
    "/Rotate 3 0 R",
    "/Resources << /Font << /F1 5 0 R >> >>",
    "/Resources 6 0 R",
    "/Resources (none)",
    "/Pages 2 0 R",
    "/Pages 3 0 R",
];

// A small file of page tree nodes that refer to each other at random: cycles,
// repeats, missing objects, wrong types and broken attributes.
fn random_page_tree(random: &mut Random) -> Vec<u8> {
    let count = 2 + random.below(12) as u32;
    let mut file = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for number in 1..=count {
        offsets.push(file.len());
        let mut body = String::from("<<");
        if number == 1 {
            body.push_str(" /Type /Catalog /Pages 2 0 R");
        }
        for _ in 0..random.below(5) {
            body.push(' ');
            body.push_str(PAGE_TREE_PIECES[random.below(PAGE_TREE_PIECES.len())]);
        }
        if random.below(2) == 0 {
            body.push_str(" /Kids [");
            for _ in 0..random.below(5) {
                match random.below(4) {
                    0 => body.push_str(" << /Type /Page >>"),
                    1 => body.push_str(" 42"),
                    _ => body.push_str(&format!(" {} 0 R", 1 + random.below(count as usize + 2))),
                }
            }
            body.push_str(" ]");
        }
        body.push_str(" >>");
        if random.below(10) == 0 {
            body = PAGE_TREE_PIECES[random.below(PAGE_TREE_PIECES.len())].to_string();
        }
        file.extend(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    let start = file.len();
    file.extend(format!("xref\n0 {}\n0000000000 65535 f \n", count + 1).as_bytes());
    for offset in offsets {
        file.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    let root = if random.below(8) == 0 {
        ""
    } else {
        "/Root 1 0 R"
    };
    file.extend(
        format!(
            "trailer\n<< /Size {} {root} >>\nstartxref\n{start}\n%%EOF\n",
            count + 1
        )
        .as_bytes(),
    );
    file
}

#[test]
fn random_page_trees_never_break_the_document_reader() {
    let mut random = Random(0x0A2A_000A);
    for _ in 0..CASES_PER_GENERATOR / 5 {
        check_document(&random_page_tree(&mut random));
    }
}

#[test]
fn a_page_tree_with_fifty_thousand_pages_is_read_quickly() {
    // A root with 500 nodes of 100 pages each, as large writers make them.
    const NODES: u32 = 500;
    const PER_NODE: u32 = 100;
    let mut file = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |file: &mut Vec<u8>, number: u32, body: &str| {
        offsets.push((number, file.len()));
        file.extend(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    let node = |index: u32| 3 + index;
    let page = |index: u32| 3 + NODES + index;
    let kids: Vec<String> = (0..NODES).map(|i| format!("{} 0 R", node(i))).collect();
    object(&mut file, 1, "<< /Type /Catalog /Pages 2 0 R >>");
    object(
        &mut file,
        2,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {} /MediaBox [0 0 612 792] >>",
            kids.join(" "),
            NODES * PER_NODE
        ),
    );
    for i in 0..NODES {
        let kids: Vec<String> = (0..PER_NODE)
            .map(|j| format!("{} 0 R", page(i * PER_NODE + j)))
            .collect();
        let body = format!(
            "<< /Type /Pages /Parent 2 0 R /Kids [{}] /Count {PER_NODE} >>",
            kids.join(" ")
        );
        object(&mut file, node(i), &body);
    }
    for i in 0..NODES * PER_NODE {
        let body = format!("<< /Type /Page /Parent {} 0 R >>", node(i / PER_NODE));
        object(&mut file, page(i), &body);
    }
    let start = file.len();
    let size = page(NODES * PER_NODE);
    file.extend(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    offsets.sort();
    for (_, offset) in &offsets {
        file.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    file.extend(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n").as_bytes(),
    );

    let started = std::time::Instant::now();
    let document = Document::open(&file).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(document.page_count(), 50_000);
    assert!(document.diagnostics().is_empty());
    // Generous, for slow machines and debug builds: it takes well under a second
    // in release builds.
    assert!(elapsed.as_secs() < 20, "took {elapsed:?}");
    eprintln!("50,000 pages read in {elapsed:?}");
}
