# ozaru-engine

ozaru's own PDF engine, written in Rust, with no other PDF engine underneath.

It's in early development and isn't part of the `ozaru` Python package yet. The
architecture and the roadmap are in [design/engine.md](../../design/engine.md).

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
