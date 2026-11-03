# ozaru-engine

ozaru's own PDF engine, written in Rust, with no other PDF engine underneath.

It's in early development and isn't part of the `ozaru` Python package yet. The
architecture and the roadmap are in [design/engine.md](../../design/engine.md).

## What it does today

- **`syntax`:** reads PDF objects, indirect objects and streams.
- **`filters`:** decodes Flate, LZW, RunLength, ASCIIHex and ASCII85 data, with the
  TIFF and PNG predictors.
- **`xref`:** loads the cross-reference data of a file, with incremental updates,
  xref streams and object streams, repairs it by scanning when it's broken, and
  resolves any reference with `Resolver`:

```rust
use ozaru_engine::base::Limits;
use ozaru_engine::xref::Resolver;

let file = std::fs::read("report.pdf")?;
let mut resolver = Resolver::open(&file, Limits::default())?;
let root = resolver.trailer().get("Root").and_then(|root| root.as_reference());
let catalog = resolver.resolve(root.expect("the trailer has a /Root"))?;
```

Every problem it works around is reported as a diagnostic, and every error has the
byte offset where it happened.

## Rules

- No `unsafe` code: `#![forbid(unsafe_code)]`.
- No global or per-thread state.
- No dependency on PyO3. The Python layer lives in the root crate.

## Run the checks

```bash
cargo fmt -p ozaru-engine --check
cargo clippy -p ozaru-engine --all-targets -- -D warnings
cargo test -p ozaru-engine
```

`tests/robustness.rs` feeds 350,000 random inputs to the lexer, the parsers, the
cross-reference readers and repair, the object stream reader, the resolver and the
Flate decoder on every run, plus 10,000 copies of the repository's PDFs with a few
bytes damaged.

## Dependencies

- [`miniz_oxide`](https://crates.io/crates/miniz_oxide), only to inflate Flate
  data. It's pure Rust, with no `unsafe` code.

## Fuzzing and coverage

Both run in GitHub Actions (`.github/workflows/engine-quality.yml`): fuzzing for 60
seconds per target on every engine change, and for as long as you choose when you
start the workflow by hand. To run them locally you need a nightly toolchain,
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) and
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov):

```bash
cd crates/ozaru-engine/fuzz
cargo +nightly fuzz run parse_object corpus/parse_object seeds/parse_object

cargo llvm-cov -p ozaru-engine --fail-under-lines 90
```

When fuzzing finds a crash, the input is saved in `fuzz/artifacts/`. Fix the bug and
add the input as a test, so it can't come back.
