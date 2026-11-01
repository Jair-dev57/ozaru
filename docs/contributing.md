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

## Release a new version

Pushing to `main` doesn't publish anything. Releases group several changes:

1. Set the new version in `Cargo.toml`.
2. Move the **Unreleased** changelog entries under the new version.
3. Commit and push, then wait for CI to pass.
4. Create and push the tag, for example `git tag v0.1.1 && git push origin v0.1.1`.

The tag starts the workflow that publishes to PyPI.
