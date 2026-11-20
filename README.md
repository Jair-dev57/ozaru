<p align="center">
  <img src="https://raw.githubusercontent.com/Jair-dev57/ozaru/main/assets/logo.png" alt="ozaru" width="220">
</p>

<h1 align="center">ozaru</h1>

<p align="center"><strong>Read, draw, edit and protect PDF files from Python.<br>
Small API, Rust core, no dependencies.</strong></p>

<p align="center">
  <a href="https://pypi.org/project/ozaru/"><img alt="PyPI" src="https://img.shields.io/pypi/v/ozaru?color=F47B20"></a>
  <a href="https://pypi.org/project/ozaru/"><img alt="Python" src="https://img.shields.io/pypi/pyversions/ozaru"></a>
  <a href="https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml"><img alt="CI" src="https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml/badge.svg"></a>
  <a href="https://github.com/Jair-dev57/ozaru/blob/main/LICENSE"><img alt="License: MIT" src="https://img.shields.io/pypi/l/ozaru"></a>
  <a href="https://github.com/sponsors/Jair-dev57"><img alt="Sponsor" src="https://img.shields.io/badge/sponsor-%E2%99%A5-D9A441"></a>
</p>

<p align="center">
  <a href="https://jair-dev57.github.io/ozaru/">Documentation</a> ·
  <a href="https://pypi.org/project/ozaru/">PyPI</a> ·
  <a href="https://jair-dev57.github.io/ozaru/changelog/">Changelog</a> ·
  <a href="https://github.com/sponsors/Jair-dev57">Sponsor</a>
</p>

---

```python
import ozaru

text = ozaru.extract_text("invoice.pdf")              # every word on every page
words = ozaru.extract_words("invoice.pdf")            # ...with where it is
png = ozaru.render_page("invoice.pdf", 1, scale=2)    # a page as an image
ozaru.fill_form("form.pdf", "filled.pdf", {"name": "Ana", "agree": True})
ozaru.encrypt("report.pdf", "protected.pdf", user_password="1234")
```

One function per task: you pass a path and get the result. No objects to manage,
no setup, and nothing else gets installed.

## Why ozaru?

| | **ozaru** | PyMuPDF | pypdf |
|---|---|---|---|
| **License** | MIT | AGPL or paid | BSD |
| **Python dependencies** | none | none | none |
| **Engine** | Rust, `forbid(unsafe_code)`, fuzzed in CI | C (MuPDF) | pure Python |
| **Install** | one wheel, no compiler | one wheel | pure Python |
| **Speed** | faster than PyMuPDF at text, on par drawing pages, tens of times faster than pypdf ([numbers](#speed)) | very fast | slow on big files |
| **Draws pages** | yes | yes | no |
| **Edits** | rotate, extract, split, merge, fill forms, encrypt: more on the way | everything | pages, forms, metadata |

Every PDF is read by **OZARU ENGINE**, ozaru's own PDF engine: it doesn't wrap
MuPDF, PDFium or any other library, and hostile files hit limits on depth, sizes and
work instead of crashing or filling your memory. Typed, with type hints included, and
every error says what to fix.

ozaru is young: the features above are checked against PyMuPDF and pypdf in a
public corpus, and the road to 1.0.0 is to have everything those libraries do
that matters. See the [roadmap](design/engine.md#phases-14-and-15-own-everything-speed-and-the-rest-of-the-libraries).

## Installation

With [uv](https://docs.astral.sh/uv/), in your project:

```bash
uv add ozaru
```

To try it once, without installing anything:

```bash
uv run --with ozaru python your_script.py
```

Or into the active environment: `uv pip install ozaru`. It also installs with `pip`.
ozaru requires Python 3.10 or newer, and ships wheels for Linux, macOS and Windows,
so you don't need Rust.

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

### Draw a page as an image

`render_page` draws a page, counted from 1, as a PNG image, with `scale` pixels for
each point of the page:

```python
import ozaru

png = ozaru.render_page("report.pdf", 1, scale=2)
with open("page-1.png", "wb") as file:
    file.write(png)
```

### Read the annotations

`annotations` returns the links, notes, highlights and form widgets of every page,
with where they are and what they say. `render_page` draws them unless you pass
`annotations=False`:

```python
import ozaru

for annotation in ozaru.annotations("report.pdf"):
    print(annotation.page, annotation.subtype, annotation.uri or annotation.contents)
```

### Read and fill forms

`form_fields` lists the fields of a form with their values, and `fill_form` fills
them and saves another file:

```python
import ozaru

for field in ozaru.form_fields("form.pdf"):
    print(field.name, field.kind, field.value)

ozaru.fill_form("form.pdf", "filled.pdf", {"name": "Ana", "agree": True})
```

### Protect a PDF

`encrypt` saves a copy protected with passwords and permissions, using AES-256:

```python
import ozaru

ozaru.encrypt("report.pdf", "protected.pdf", user_password="1234", permissions=["print"])
```

### Rotate pages

`rotate` turns pages clockwise and saves the result to another file:

```python
import ozaru

ozaru.rotate("report.pdf", "rotated.pdf", 90)              # every page
ozaru.rotate("report.pdf", "rotated.pdf", -90, pages=[2])  # only page 2
```

### Extract and split pages

`extract_pages` saves some pages, in the order given, to a new file, and
`split` saves each page as its own file in a folder:

```python
import ozaru

ozaru.extract_pages("report.pdf", "summary.pdf", [3, 1])
paths = ozaru.split("report.pdf", "pages")  # pages/report-1.pdf, ...
```

### Merge PDFs

`merge` joins several PDFs into one file, in order:

```python
import ozaru

ozaru.merge(["cover.pdf", "report.pdf", "annex.pdf"], "full.pdf")
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
| `annotations(path)`  | `list[Annotation]` | `OSError`, `ValueError` | `PageWarning` |
| `form_fields(path)`  | `list[FormField]` | `OSError`, `ValueError` | `PageWarning` |
| `fill_form(path, output, values)` | `None` | `OSError`, `ValueError` | `PageWarning` |
| `encrypt(path, output)` | `None` | `OSError`, `ValueError` | `PageWarning` |
| `permissions(path)`  | `list[str]` or `None` | `OSError`, `ValueError` | |
| `render_page(path, page)` | `bytes` | `OSError`, `ValueError`, `IndexError` | `PageWarning` |
| `rotate(path, output, degrees)` | `None` | `OSError`, `ValueError`, `IndexError` | `PageWarning` |
| `extract_pages(path, output, pages)` | `None` | `OSError`, `ValueError`, `IndexError` | `PageWarning` |
| `split(path, folder)` | `list[Path]` | `OSError`, `ValueError` | `PageWarning` |
| `merge(paths, output)` | `None` | `OSError`, `ValueError` | `PageWarning` |

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

`render_page` at scale 1, as PNG bytes, opening the file each time:

| File | ozaru | PyMuPDF 2.1 |
|---|---|---|
| An invoice, 1 page | 6.9 ms | 15 ms |
| A photo cover (1080 x 1920 pixels) | 217 ms | 217 ms |
| A smaller photo cover | 16.5 ms | 22 ms |
| A page of text of 2,380 glyphs | 41 ms | 41 ms |

`page_count` reads every page of the PDF, so that damaged files are counted
right, which takes longer on files with hundreds of pages. To measure on your
machine, run `crates/engine/benches/compare_libraries.py`.

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

On other platforms, uv (or pip) builds ozaru from source, which requires a Rust toolchain.

## Sponsor

ozaru is built and maintained by [Jair Estupiñan](https://github.com/Jair-dev57).
If it saves you time, you can
[sponsor the project](https://github.com/sponsors/Jair-dev57): it pays for the
time that goes into the engine, the tests and the documentation.

## Contributing

You need [Rust](https://rustup.rs/) and [uv](https://docs.astral.sh/uv/). Then run:

```bash
git clone https://github.com/Jair-dev57/ozaru.git
cd ozaru
uv run pytest
```

See the [contributing guide](https://jair-dev57.github.io/ozaru/contributing/).
Bug reports and ideas are welcome in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues); for security problems,
read [SECURITY.md](SECURITY.md).

## License

ozaru is released under the
[MIT License](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE).
