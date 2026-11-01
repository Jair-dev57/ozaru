# ozaru

**Read PDF files from Python with a small, simple API and a Rust core.**

[![PyPI version](https://img.shields.io/pypi/v/ozaru)](https://pypi.org/project/ozaru/)
[![Python versions](https://img.shields.io/pypi/pyversions/ozaru)](https://pypi.org/project/ozaru/)
[![CI](https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml/badge.svg)](https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml)
[![License: MIT](https://img.shields.io/pypi/l/ozaru)](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE)

```python
import ozaru

text = ozaru.extract_text("invoice.pdf")
pages = ozaru.page_count("invoice.pdf")

print(f"{pages} pages")
print(text)
```

## Why ozaru?

- **One line per task.** Pass a path and get the result. There are no objects to manage.
- **Easy to install.** Prebuilt wheels mean you don't need Rust or a compiler.
- **No Python dependencies.** ozaru installs nothing else into your environment.
- **Built on Rust.** The PDF work is done by [pdf_oxide](https://crates.io/crates/pdf_oxide).
- **Typed.** Type hints are included, so your editor can autocomplete and check your calls.

## Installation

```bash
pip install ozaru
```

Or, with [uv](https://docs.astral.sh/uv/):

```bash
uv add ozaru
```

ozaru requires Python 3.10 or newer.

## Usage

### Extract text

`extract_text` returns the text of the whole document as a single string:

```python
import ozaru

text = ozaru.extract_text("report.pdf")
```

### Count pages

`page_count` returns the number of pages as an integer:

```python
import ozaru

pages = ozaru.page_count("report.pdf")
```

### Handle errors

```python
import ozaru

try:
    text = ozaru.extract_text("report.pdf")
except FileNotFoundError:
    print("The file does not exist.")
except ValueError:
    print("The file could not be read as a PDF.")
```

## API

| Function             | Returns | Raises                            |
|----------------------|---------|-----------------------------------|
| `extract_text(path)` | `str`   | `FileNotFoundError`, `ValueError` |
| `page_count(path)`   | `int`   | `FileNotFoundError`, `ValueError` |

`path` must be a `str`. `pathlib.Path` objects are not supported yet, so convert
them first with `str(path)`.

## Supported platforms

Prebuilt wheels are available for CPython 3.10 and newer:

| Operating system | Architectures          |
|------------------|------------------------|
| Linux (glibc)    | x86_64, aarch64        |
| Linux (musl)     | x86_64, aarch64        |
| macOS            | Intel, Apple Silicon   |
| Windows          | x64                    |

On other platforms, pip builds ozaru from source, which requires a Rust toolchain.

## Limitations

ozaru is in early development, so its API may change between releases.

- It reads the text stored in a PDF and does not perform OCR, so scanned documents
  may return little or no text.
- Error messages are currently in Spanish.

## Contributing

You need [Rust](https://rustup.rs/) and [uv](https://docs.astral.sh/uv/). Then run:

```bash
git clone https://github.com/Jair-dev57/ozaru.git
cd ozaru
uv run pytest
```

Bug reports and ideas are welcome in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).

## License

ozaru is released under the
[MIT License](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE).
