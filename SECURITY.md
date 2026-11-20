# Security policy

ozaru reads files that come from anywhere, so a PDF that crashes it, makes it
take too long or fills the memory is a security problem, and so is a PDF it
draws or decrypts wrongly.

## Reporting a vulnerability

Please **don't open a public issue**. Use GitHub's private reporting:
<https://github.com/Jair-dev57/ozaru/security/advisories/new>.

Say which version of ozaru you use (`pip show ozaru` or `uv pip show ozaru`)
and, if you can, attach the PDF or a script that reproduces it. You will get an
answer within a few days, and the fix comes with a new release and a note in the
[changelog](docs/changelog.md).

## Supported versions

Only the latest release gets fixes: ozaru is in `0.x`, and the API still changes.

## How the engine defends itself

OZARU ENGINE is written in Rust with `#![forbid(unsafe_code)]`, applies limits
to depth, lengths, decoded sizes, pixels and the work of each page, and is
fuzzed in CI with one target for each part (parsers, filters, fonts, drawing,
forms, encryption). See [crates/engine/README.md](crates/engine/README.md).
