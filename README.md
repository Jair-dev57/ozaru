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
- **Built on Rust.** Every PDF is read by OZARU ENGINE, ozaru's own PDF engine, written in Rust with no other PDF library inside.
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

### Find words

`extract_words` returns every word with its page, its line and its box, in
points from the top left corner of the page:

```python
import ozaru

for word in ozaru.extract_words("report.pdf"):
    print(word.page, word.text, word.x0, word.y0, word.x1, word.y1)
```

### Extract images

`extract_images` returns every image with its page, its box and its file:

```python
import ozaru

for image in ozaru.extract_images("brochure.pdf"):
    image.save(f"page{image.page}-{image.number}.{image.format}")
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
| `extract_words(path)`| `list[Word]`     | `OSError`, `ValueError` | `PageWarning` |
| `extract_blocks(path)`| `list[Block]`   | `OSError`, `ValueError` | `PageWarning` |
| `extract_images(path)`| `list[Image]`   | `OSError`, `ValueError` | `PageWarning` |
| `page_count(path)`   | `int`            | `OSError`, `ValueError` |               |
| `metadata(path)`     | `Metadata`       | `OSError`, `ValueError` |               |
| `page_info(path)`    | `list[PageInfo]` | `OSError`, `ValueError` | `PageWarning` |

`Metadata` has the `title`, `author`, `subject`, `keywords`, `creator` and
`producer` of the PDF as `str`, and its `creation_date` and `modification_date`
as `datetime`. Each one is `None` when the PDF doesn't have it.

`Word` has the `page`, `block` and `line` of a word, counted from 1, its `text`,
and its box: `x0`, `y0`, `x1` and `y1`, in points from the top left corner of
the page. `Block` has the `page`, `number`, `text` and box of a paragraph or
another block of text. `Image` has the `page`, `number`, box, `width` and
`height` in pixels, `format` and `data` of an image, and `save(path)` writes it
to a file.

`PageInfo` has the `number`, `width`, `height` and `rotation` of a page, with
`width` and `height` in points as a viewer shows it, rotated.

Every function takes an optional `password` keyword argument, the user or owner
password of a protected PDF. A missing or wrong password raises
`ozaru.PasswordError`, a subclass of `ValueError`.

File problems raise the same `OSError` subclasses as Python's `open()`, such as
`FileNotFoundError`, `PermissionError` and `IsADirectoryError`.

`path` can be a `str` or a `pathlib.Path`.

## Speed

The median time of one call, in milliseconds, on a laptop with an Intel Core
i5-11400H. pypdf and PyMuPDF are timed doing the same work: the text of every
page, and the page count.

`extract_text`:

| File | ozaru | ozaru 0.1.7 | PyMuPDF 2.1 | pypdf 7.0 |
|---|---|---|---|---|
| 3 pages | 0.25 ms | 0.64 ms | 2.0 ms | 3.3 ms |
| An invoice, 3 pages | 0.95 ms | 2.1 ms | 2.7 ms | 7.7 ms |
| Chinese, Japanese and Korean, 1 page | 0.46 ms | 2.1 ms | 1.6 ms | 8.5 ms |
| 500 pages | 9.5 ms | 38 ms | 56 ms | 347 ms |
| 1000 pages | 7.1 ms | 47 ms | 107 ms | 419 ms |

`page_count`:

| File | ozaru | ozaru 0.1.7 | PyMuPDF 2.1 | pypdf 7.0 |
|---|---|---|---|---|
| 3 pages | 0.057 ms | 0.080 ms | 0.16 ms | 0.53 ms |
| 500 pages | 1.9 ms | 0.77 ms | 0.30 ms | 55 ms |
| 1000 pages | 1.9 ms | 0.75 ms | 1.8 ms | 57 ms |

`page_count` reads every page of the PDF, so that damaged files are counted
right, which takes longer on files with hundreds of pages. To measure on your
machine, run `crates/ozaru-engine/benches/compare_libraries.py`.

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
- Columns are read one after the other when the PDF paints them one after the
  other or row by row, but not in other orders.

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
