"""Times what a Python user sees: the installed ozaru against pypdf and PyMuPDF.

It makes its own test files with PyMuPDF (pages full of text, and pages that are
photographs) in a temporary folder, so the numbers don't depend on the files of
anyone's machine, and prints the median time of one call, in milliseconds, as
tables for the documentation. Every library is timed doing the same work:

    pip install ozaru pypdf pymupdf pillow numpy     # or `uv run --with ...`
    python compare_libraries.py

`ozaru.render_page` and `ozaru.extract_words` have no pypdf equivalent.
"""

from __future__ import annotations

import importlib.metadata
import io
import platform
import random
import statistics
import tempfile
import time
import warnings
from pathlib import Path

import numpy as np
import ozaru
import pymupdf
import pypdf
from PIL import Image, ImageFilter

WORDS = (
    "the quick brown fox jumps over lazy dog lorem ipsum dolor sit amet consectetur "
    "adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua"
).split()


def median_ms(work, seconds: float = 0.7, at_least: int = 5) -> float:
    work()
    times = []
    started = time.perf_counter()
    while (time.perf_counter() - started < seconds or len(times) < at_least) and len(times) < 200:
        run = time.perf_counter()
        work()
        times.append(time.perf_counter() - run)
    return statistics.median(times) * 1000


def text_pdf(path: Path, pages: int) -> None:
    random.seed(3)
    document = pymupdf.open()
    for number in range(pages):
        page = document.new_page()
        page.insert_text((50, 40), f"Page {number + 1}", fontsize=16)
        for line in range(38):
            text = " ".join(random.choice(WORDS) for _ in range(14))
            page.insert_text((50, 60 + 18 * line), text, fontsize=10)
    document.save(path, deflate=True)


def photo_pdf(path: Path, pages: int) -> None:
    rng = np.random.default_rng(2)
    document = pymupdf.open()
    for _ in range(pages):
        x = np.linspace(0, 1, 1600)[None, :, None]
        y = np.linspace(0, 1, 1200)[:, None, None]
        pixels = np.concatenate([x + 0 * y, y + 0 * x, x * y], axis=2) * 255
        pixels = pixels + rng.normal(0, 14, pixels.shape)
        image = Image.fromarray(np.clip(pixels, 0, 255).astype("uint8"))
        data = io.BytesIO()
        image.filter(ImageFilter.GaussianBlur(0.8)).save(data, "JPEG", quality=85)
        page = document.new_page(width=800, height=600)
        page.insert_image(page.rect, stream=data.getvalue())
    document.save(path)


def pypdf_text(path: Path) -> str:
    return "\f".join(page.extract_text() for page in pypdf.PdfReader(path).pages)


def pymupdf_text(path: Path) -> str:
    with pymupdf.open(path) as document:
        return "\f".join(page.get_text() for page in document)


def pymupdf_words(path: Path) -> list:
    with pymupdf.open(path) as document:
        return [page.get_text("words") for page in document]


def pymupdf_count(path: Path) -> int:
    with pymupdf.open(path) as document:
        return document.page_count


def pymupdf_render(path: Path) -> bytes:
    with pymupdf.open(path) as document:
        return document[0].get_pixmap(alpha=False).tobytes("png")


def row(label: str, times: list[float | None]) -> str:
    cells = ["-" if t is None else (f"{t:.2f} ms" if t < 10 else f"{t:.0f} ms") for t in times]
    return f"| {label} | " + " | ".join(cells) + " |"


def main() -> None:
    warnings.simplefilter("ignore")
    names = [
        f"ozaru {importlib.metadata.version('ozaru')}",
        f"PyMuPDF {pymupdf.__version__}",
        f"pypdf {pypdf.__version__}",
    ]
    print(f"{platform.processor() or platform.machine()}, Python {platform.python_version()}\n")
    with tempfile.TemporaryDirectory() as folder:
        folder = Path(folder)
        files = {}
        for pages in (10, 200, 1000):
            files[f"{pages} pages of text"] = folder / f"text{pages}.pdf"
            text_pdf(files[f"{pages} pages of text"], pages)
        files["12 pages of photographs"] = folder / "photos.pdf"
        photo_pdf(files["12 pages of photographs"], 12)
        tasks = [
            ("extract_text", ozaru.extract_text, pymupdf_text, pypdf_text),
            ("extract_words", ozaru.extract_words, pymupdf_words, None),
            ("page_count", ozaru.page_count, pymupdf_count, lambda p: len(pypdf.PdfReader(p).pages)),
            ("render_page, page 1", lambda p: ozaru.render_page(p, 1), pymupdf_render, None),
        ]
        for title, ours, theirs, pypdf_work in tasks:
            print(f"`{title}`:\n")
            print("| File | " + " | ".join(names) + " |")
            print("|---|---|---|---|")
            for label, path in files.items():
                big = "1000" in label
                fast = pypdf_work if not (big and title == "extract_text") else None
                times = [
                    median_ms(lambda: ours(path), 0.5 if big else 0.7),
                    median_ms(lambda: theirs(path), 0.5 if big else 0.7),
                    median_ms(lambda: fast(path), 0.3, 2) if fast else None,
                ]
                print(row(label, times))
            print()


if __name__ == "__main__":
    main()
