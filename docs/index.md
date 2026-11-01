# ozaru

**Read PDF files from Python with a small, simple API and a Rust core.**

[![PyPI version](https://img.shields.io/pypi/v/ozaru)](https://pypi.org/project/ozaru/)
[![Python versions](https://img.shields.io/pypi/pyversions/ozaru)](https://pypi.org/project/ozaru/)
[![CI](https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml/badge.svg)](https://github.com/Jair-dev57/ozaru/actions/workflows/CI.yml)
[![License: MIT](https://img.shields.io/pypi/l/ozaru)](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE)

---

ozaru gets the text out of a PDF in one line of Python:

```python
--8<-- "docs_src/quickstart.py"
```

Run it on the [sample report](assets/report.pdf){ download } and you get:

```text
--8<-- "docs_src/quickstart.out"
```

## Why ozaru?

- **One line per task.** Pass a path and get the result. There are no objects to manage.
- **Easy to install.** Prebuilt wheels mean you don't need Rust or a compiler.
- **No Python dependencies.** ozaru installs nothing else into your environment.
- **Built on Rust.** The PDF work is done by [pdf_oxide](https://crates.io/crates/pdf_oxide).
- **Typed.** Type hints are included, so your editor can autocomplete and check your calls.

## Install it

=== "pip"

    ```bash
    pip install ozaru
    ```

=== "uv"

    ```bash
    uv add ozaru
    ```

## Where to go next

- New to ozaru? Start with [First steps](first-steps.md).
- Want to learn it properly? Follow the [Tutorial](tutorial/extract-text.md) in order.
- Looking for one exact detail? Go to the [API reference](reference/api.md).
