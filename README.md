<p align="center">
  <img src="https://raw.githubusercontent.com/Jair-dev57/ozaru/main/assets/gorilla.png" alt="ozaru" width="180">
</p>

<h1 align="center">ozaru</h1>

<p align="center"><strong>Read, draw, edit and protect PDF files from Python.<br>
Small API, Rust core, no dependencies.</strong></p>

<p align="center">
  <a href="https://pypi.org/project/ozaru/"><img alt="PyPI" src="https://img.shields.io/pypi/v/ozaru?color=F47B20"></a>
  <a href="https://pypi.org/project/ozaru/"><img alt="Python" src="https://img.shields.io/pypi/pyversions/ozaru"></a>
  <a href="https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml"><img alt="CI" src="https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml/badge.svg"></a>
  <a href="https://github.com/Jair-dev57/ozaru/blob/main/LICENSE"><img alt="License: MIT" src="https://img.shields.io/pypi/l/ozaru"></a>
</p>

<p align="center">
  <a href="https://jair-dev57.github.io/ozaru/">Documentation</a> ·
  <a href="https://pypi.org/project/ozaru/">PyPI</a> ·
  <a href="https://jair-dev57.github.io/ozaru/changelog/">Changelog</a>
</p>

---

```python
import ozaru

text = ozaru.extract_text("invoice.pdf")
png = ozaru.render_page("invoice.pdf", 1, scale=2)
ozaru.fill_form("form.pdf", "filled.pdf", {"name": "Ana"})
```

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Jair-dev57/ozaru/main/assets/speed-dark.svg">
    <img alt="Time to extract text and draw pages: ozaru against PyMuPDF and pypdf, shorter is better" src="https://raw.githubusercontent.com/Jair-dev57/ozaru/main/assets/speed-light.svg" width="760">
  </picture>
</p>

<p align="center"><sub>Median time of one call, on an Intel Core i5-11400H.
<a href="https://jair-dev57.github.io/ozaru/reference/speed/">All the numbers</a>.</sub></p>

| | **ozaru** | PyMuPDF | pypdf |
|---|---|---|---|
| **License** | MIT | AGPL or paid | BSD |
| **Python dependencies** | none | none | none |
| **Engine** | Rust, `forbid(unsafe_code)`, fuzzed in CI | C (MuPDF) | pure Python |
| **Install** | one wheel, no compiler | one wheel | pure Python |
| **Speed** | faster than PyMuPDF at text, words and drawing pages, and about 8 times faster than pypdf | very fast | slow on big files |
| **Draws pages** | yes | yes | no |
| **Edits** | rotate, extract, split, merge, fill forms, encrypt: more on the way | everything | pages, forms, metadata |

One function per task, an engine of its own in Rust, nothing else to install.
Young and moving fast: the [roadmap](https://github.com/Jair-dev57/ozaru/blob/main/design/engine.md#phases-14-and-15-own-everything-speed-and-the-rest-of-the-libraries)
to 1.0.0 is to do everything PyMuPDF and pypdf do that matters.

## Installation

With [uv](https://docs.astral.sh/uv/), in your project:

```bash
uv add ozaru
```

From the terminal, with nothing installed:

```bash
uvx ozaru text invoice.pdf
```

Or into the active environment: `uv pip install ozaru`. It also installs with `pip`.
Python 3.10 or newer; wheels for Linux, macOS and Windows, so no Rust needed.

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
read [SECURITY.md](https://github.com/Jair-dev57/ozaru/blob/main/SECURITY.md).

## License

ozaru is released under the
[MIT License](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE).
