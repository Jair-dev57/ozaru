"""Times `page_count` of the installed ozaru against pypdf, on the corpus.

The engine's own benchmark (`cargo bench -p ozaru-engine`) measures its parts;
this one measures what a Python user sees. Run it once per ozaru to compare, for
example a release from PyPI and a local build, each in its own environment:

    pip install pypdf ozaru==0.1.7      # or: pip install . from the repository
    python compare_libraries.py

It prints the median time of one call, in milliseconds.
"""

from __future__ import annotations

import importlib.metadata
import statistics
import time
import warnings
from pathlib import Path

import ozaru
import pypdf

TESTS = Path(__file__).resolve().parents[1] / "tests"
FILES = [
    TESTS.parents[2] / "docs" / "assets" / "report.pdf",
    TESTS / "corpus" / "many-pages.pdf",
    TESTS / "corpus" / "balanced-tree.pdf",
    TESTS / "corpus" / "balanced-tree-compressed.pdf",
    TESTS / "files" / "report-rc4-128.pdf",
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


def main() -> None:
    version = importlib.metadata.version("ozaru")
    print(f"{'file':32} {'ozaru ' + version:>14} {'pypdf ' + pypdf.__version__:>14}")
    for path in FILES:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            ours = median_ms(lambda: ozaru.page_count(path))
            theirs = median_ms(lambda: len(pypdf.PdfReader(path).pages))
        print(f"{path.name:32} {ours:>11.3f} ms {theirs:>11.3f} ms")


if __name__ == "__main__":
    main()
