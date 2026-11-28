# Contributing

Bug reports and ideas are welcome in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).

## Set up the project

You need [Rust](https://rustup.rs/) and [uv](https://docs.astral.sh/uv/).

```bash
git clone https://github.com/Jair-dev57/ozaru.git
cd ozaru
uv run pytest
```

## Test and measure the engine

OZARU ENGINE, the Rust crate in `crates/engine`, has its own tests and
benchmarks:

```bash
cargo test -p ozaru-engine
cargo bench -p ozaru-engine
```

## Preview the documentation

```bash
uv run --group docs mkdocs serve
```

Then open <http://127.0.0.1:8000>. The page reloads when you save a file.

## How the examples are tested

The code in this documentation is not pasted into the pages. Each example lives
in `docs_src/` as a real `.py` file, next to a `.out` file with its exact output,
and the pages include both.

`tests/test_docs_src.py` runs every example against the sample report and fails
if its output changes. To add an example, create both files and include them with:

````markdown
```python
;--8<-- "docs_src/my_example.py"
```
````

## Every change updates the docs

If a change affects users, the same commit also updates:

- the affected pages in `docs/`
- [Limitations](reference/limitations.md), if it removes or adds one
- the **Unreleased** section of the [Changelog](changelog.md)

## Branches

Work is done on branches, named by what they are for:

| Branch | Use |
|---|---|
| `main` | Only published versions: every tag `vX.Y.Z` is on `main`. |
| `develop` | Where finished work is integrated; the next version is built here. |
| `feature/<name>` | New functionality, from `develop`, such as `feature/compare-pdfs`. |
| `bugfix/<name>` | A bug found in what is not published yet, from `develop`. |
| `chore/<name>` | Maintenance: dependencies, CI, configuration, from `develop`. |
| `docs/<name>` | Changes to the documentation only, from `develop`. |
| `release/X.Y.Z` | Preparing a version, from `develop`. |
| `hotfix/<name>` | An urgent fix of what is published, from `main`. |

A branch is merged with `git merge --no-ff` once its CI passes and the tests pass, and it is kept on GitHub after
that, as a record of what was done. Branches of the form `engine/phase-N` are the history of the phases the
engine was built in; new work doesn't use them.

## Every change updates the docs

If a change affects users, the same commit also updates:

- the affected pages in `docs/`
- [Limitations](reference/limitations.md), if it removes or adds one
- the **Unreleased** section of the [Changelog](changelog.md)

## Release a new version

Pushing to `main` doesn't publish anything. Releases group several changes:

1. Create `release/X.Y.Z` from `develop`.
2. Set the new version in `Cargo.toml` and move the **Unreleased** changelog entries under it.
3. Commit and push, then wait for CI to pass.
4. Merge the branch into `main` with `--no-ff`, push `main` and wait for its CI.
5. Create and push the tag, for example `git tag v0.1.1 && git push origin v0.1.1`.
6. Merge `main` back into `develop`.

The tag starts the workflow that publishes to PyPI.

A fix of something already published is a `hotfix/<name>` branch from `main`: merge it into `main`, tag the new
version as above, and merge `main` into `develop` too.
