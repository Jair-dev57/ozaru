# Installation

ozaru requires **Python 3.10 or newer**.

## Install the package

=== "pip"

    ```bash
    pip install ozaru
    ```

=== "uv"

    ```bash
    uv add ozaru
    ```

!!! tip
    Install ozaru inside a virtual environment, so each project keeps its own
    packages. With uv, `uv add` takes care of that for you.

## Check that it works

Open Python and import the package:

```python
import ozaru

print(ozaru.page_count)
```

If you see `<built-in function page_count>`, ozaru is installed.

## Supported platforms

Prebuilt wheels are available for CPython 3.10 and newer:

| Operating system | Architectures        |
|------------------|----------------------|
| Linux (glibc)    | x86_64, aarch64      |
| Linux (musl)     | x86_64, aarch64      |
| macOS            | Intel, Apple Silicon |
| Windows          | x64                  |

Before each release, the tests run against the wheels of glibc Linux, macOS and
Windows on their own platform, with Python 3.10 and 3.14 (only 3.10 on Apple
Silicon).

On any other platform, pip builds ozaru from source. In that case you need a
[Rust toolchain](https://rustup.rs/).

Next: [First steps](first-steps.md).
