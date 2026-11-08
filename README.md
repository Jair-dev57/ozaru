# ozaru

**Read PDF files from Python with a small, simple API and a Rust core.**

[![PyPI version](https://img.shields.io/pypi/v/ozaru)](https://pypi.org/project/ozaru/)
[![Python versions](https://img.shields.io/pypi/pyversions/ozaru)](https://pypi.org/project/ozaru/)
[![CI](https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml/badge.svg)](https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml)
[![License: MIT](https://img.shields.io/pypi/l/ozaru)](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE)

**Documentation:** <https://jair-dev57.github.io/ozaru/>

```python
import ozaru

text = ozaru.extract_text("invoice.pdf")
pages = ozaru.page_count("invoice.pdf")
info = ozaru.metadata("invoice.pdf")

print(f"{info.title}: {pages} pages")
print(text)
```

## Why ozaru?

- **One line per task.** Pass a path and get the result. There are no objects to manage.
- **Easy to install.** Prebuilt wheels mean you don't need Rust or a compiler.
- **No Python dependencies.** ozaru installs nothing else into your environment.
- **Built on Rust.** Pages are counted by OZARU ENGINE, ozaru's own PDF engine, and text is extracted by [pdf_oxide](https://crates.io/crates/pdf_oxide) until the engine takes over.
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
except OSError:
    print("The file could not be read.")
except ValueError:
    print("The file could not be read as a PDF.")
```

### Damaged pages

If a page can't be read, `extract_text` still returns the other pages, leaves that
page empty and emits an `ozaru.PageWarning` with its `page` number:

```python
import warnings

import ozaru

with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always", ozaru.PageWarning)
    text = ozaru.extract_text("report.pdf")

for warning in caught:
    print(f"Page {warning.message.page} could not be read")
```

## API

| Function             | Returns          | Raises                  | Warns         |
|----------------------|------------------|-------------------------|---------------|
| `extract_text(path)` | `str`            | `OSError`, `ValueError` | `PageWarning` |
| `page_count(path)`   | `int`            | `OSError`, `ValueError` |               |
| `metadata(path)`     | `Metadata`       | `OSError`, `ValueError` |               |
| `page_info(path)`    | `list[PageInfo]` | `OSError`, `ValueError` | `PageWarning` |

`Metadata` has the `title`, `author`, `subject`, `keywords`, `creator` and
`producer` of the PDF as `str`, and its `creation_date` and `modification_date`
as `datetime`. Each one is `None` when the PDF doesn't have it.

`PageInfo` has the `number`, `width`, `height` and `rotation` of a page, with
`width` and `height` in points as a viewer shows it, rotated.

File problems raise the same `OSError` subclasses as Python's `open()`, such as
`FileNotFoundError`, `PermissionError` and `IsADirectoryError`.

`path` can be a `str` or a `pathlib.Path`.

## Supported platforms

Prebuilt wheels are available for CPython 3.10 and newer:

| Operating system | Architectures          |
|------------------|------------------------|
| Linux (glibc)    | x86_64, aarch64        |
| Linux (musl)     | x86_64, aarch64        |
| macOS            | Intel, Apple Silicon   |
| Windows          | x64                    |

Before each release, the tests run against the wheels of glibc Linux, macOS and
Windows on their own platform, with Python 3.10 and 3.14 (only 3.10 on Apple
Silicon).

On other platforms, pip builds ozaru from source, which requires a Rust toolchain.

## Limitations

ozaru is in early development, so its API may change between releases.

- It reads the text stored in a PDF and does not perform OCR, so scanned documents
  may return little or no text.
- It can't extract text from PDFs that need a password to open.

## Contributing

You need [Rust](https://rustup.rs/) and [uv](https://docs.astral.sh/uv/). Then run:

```bash
git clone https://github.com/Jair-dev57/ozaru.git
cd ozaru
uv run pytest
```

See the [contributing guide](https://jair-dev57.github.io/ozaru/contributing/).
Bug reports and ideas are welcome in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).

## License

ozaru is released under the
[MIT License](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE).
