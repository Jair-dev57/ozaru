# OZARU ENGINE

The PDF engine behind [ozaru](../../README.md): reads, draws, edits and protects
PDF files, written in Rust, with no other PDF engine underneath. The Python package
is a thin layer over it (`src/lib.rs` at the root of the repository). The
architecture, the phases and their reviews are in
[design/engine.md](../../design/engine.md).

## Modules

| Module | What it does |
|---|---|
| `syntax` | Objects, indirect objects, streams and the lexer and parser |
| `xref` | Cross-reference tables and streams, incremental updates, object streams, repair of broken files, and `Resolver`, which reads any object |
| `filters` | Flate, LZW, RunLength, ASCIIHex, ASCII85, CCITT, JBIG2 and JPEG, with predictors |
| `security` | The standard security handler, revisions 2 to 6 (RC4, AES-128, AES-256) |
| `document` | The catalog, page tree, metadata, XMP, annotations and form fields; `Document` is the entry point |
| `content` | The content stream interpreter, with graphics and text state |
| `fonts` | Encodings, CMaps, simple and composite fonts, metrics and glyph outlines |
| `text` | Text, lines, words, blocks and reading order |
| `images` | Images, color spaces and PNG export |
| `functions` | PDF functions |
| `render` | The rasterizer, strokes, clips, transparency, patterns and shadings: `render_page` |
| `writer` | Serializing objects, saving, editing (`Editor`), filling forms and encrypting |
| `base` | Limits, errors and diagnostics |

```rust
use ozaru_engine::Document;
use ozaru_engine::text::document_text;

let file = std::fs::read("report.pdf")?;
let mut document = Document::open(&file)?;
for page in document_text(&mut document) {
    println!("{}", page.text);
}
```

Every problem it works around is reported as a diagnostic, and every error has the
byte offset where it happened.

## Rules

- No `unsafe` code: `#![forbid(unsafe_code)]`.
- No global or per-thread state.
- No dependency on PyO3. The Python layer lives in the root crate.
- Limits on everything a hostile file could make large: depth, lengths, decoded
  sizes, pixels, tiles and the work of each page.

## Run the checks

```bash
cargo fmt -p ozaru-engine --check
cargo clippy -p ozaru-engine --all-targets -- -D warnings
cargo test -p ozaru-engine
cargo llvm-cov -p ozaru-engine --fail-under-lines 90
```

The corpus in `tests/corpus` compares the engine with pypdf, MuPDF, pdfminer and
Poppler, from the references that `compare.py`, `render.py` and the other scripts
there write: `cargo test` runs without Python. See its `MANIFEST.md`.

`tests/robustness.rs` feeds random inputs to the lexer, the parsers, the
cross-reference readers, repair and the Flate decoder on every run, plus copies of
the repository's PDFs with a few bytes damaged.

## Dependencies

None. Everything the engine does is in this crate: Flate (`filters::inflate` and
`deflate_zlib`), the ciphers and hashes of PDF encryption (`crypto`), the Unicode
Bidirectional Algorithm (`text::bidi_levels`), the fonts (TrueType, CFF and Type 1
outlines, `cmap` and `post` tables) and the rasterizer. What isn't code is data:
the tables of Unicode bidirectional classes, generated from the Unicode Character
Database, the Adobe Glyph List, and the font files of the substitutes for the
standard 14 fonts (see `THIRD_PARTY_NOTICES.md`).

Each part is checked against an authority outside the repository: Flate against
Python's `zlib`, the ciphers against `hashlib` and `cryptography`, the Bidirectional
Algorithm against the Unicode Consortium's conformance test (91,707 cases), and
fonts and text against the PDFs of the corpus that other writers made.

## Fuzzing

Fuzzing runs in GitHub Actions (`.github/workflows/engine-quality.yml`), 60 seconds
for each target on every engine change, and as long as you choose when you start
the workflow by hand. Locally you need a nightly toolchain and
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz):

```bash
cd crates/engine/fuzz
mkdir -p corpus/parse_object
cargo +nightly fuzz run parse_object corpus/parse_object seeds/parse_object
```

Pass `corpus/<target>` first, so what the fuzzer finds goes there and not into
`seeds/`. When fuzzing finds a crash, the input is saved in `fuzz/artifacts/`. Fix
the bug and add the input as a test, so it can't come back.
