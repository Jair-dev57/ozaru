"""Times `extract_text` and `page_count` of the installed ozaru against pypdf
and PyMuPDF, on files of the corpus.

The engine's own benchmark (`cargo bench -p ozaru-engine`) measures its parts;
this one measures what a Python user sees. Run it once per ozaru to compare, for
example a release from PyPI and a local build, each in its own environment:

    pip install pypdf pymupdf ozaru==0.1.7      # or: pip install . from the repository
    python compare_libraries.py

It prints the median time of one call, in milliseconds. pypdf and PyMuPDF are
timed doing the same: the text of every page, joined, and the page count.
"""

from __future__ import annotations

import importlib.metadata
import statistics
import time
import warnings
from pathlib import Path

import ozaru
import pymupdf
import pypdf

TESTS = Path(__file__).resolve().parents[1] / "tests"
FILES = [
    TESTS.parents[2] / "docs" / "assets" / "report.pdf",
    TESTS.parents[2] / "tests" / "factura.pdf",
    TESTS / "corpus" / "fonts-cairo-cjk.pdf",
    TESTS / "corpus" / "many-pages.pdf",
    TESTS / "corpus" / "balanced-tree.pdf",
]


def median_ms(work, seconds: float = 0.5) -> float:
    for _ in range(3):
        work()
    times = []
    started = time.perf_counter()
    while time.perf_counter() - started < seconds or len(times) < 10:
        run = time.perf_counter()
        work()
        times.append(time.perf_counter() - run)
    return statistics.median(times) * 1000


def pypdf_text(path: Path) -> str:
    return "\f".join(page.extract_text() for page in pypdf.PdfReader(path).pages)


def pymupdf_text(path: Path) -> str:
    with pymupdf.open(path) as document:
        return "\f".join(page.get_text() for page in document)


def pymupdf_count(path: Path) -> int:
    with pymupdf.open(path) as document:
        return document.page_count


def main() -> None:
    version = importlib.metadata.version("ozaru")
    names = [f"ozaru {version}", f"pypdf {pypdf.__version__}", f"PyMuPDF {pymupdf.__version__}"]
    for function, works in [
        ("extract_text", [ozaru.extract_text, pypdf_text, pymupdf_text]),
        ("page_count", [ozaru.page_count, lambda p: len(pypdf.PdfReader(p).pages), pymupdf_count]),
    ]:
        print(f"\n{function:32}" + "".join(f"{name:>18}" for name in names))
        for path in FILES:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                times = [median_ms(lambda: work(path)) for work in works]
            print(f"{path.name:32}" + "".join(f"{time:>15.3f} ms" for time in times))


if __name__ == "__main__":
    main()
