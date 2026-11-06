"""Compares the page count of the engine with other readers, on every PDF of
the repository, and writes the references to expected.tsv. The media box, crop
box and rotation of the first 50 pages of each file, as pypdf reads them, go to
pages.tsv.

The references are ozaru 0.1.7, which reads PDFs with pdf_oxide, pypdf, and
pdfinfo from Poppler. Files that can't be read without a password are opened
with "user", the user password of the corpus, except by ozaru 0.1.7, which
can't take one.

    pip install ozaru==0.1.7 pypdf cryptography
    python compare.py

It prints a table and exits with 1 if the engine disagrees with a reference
that the other references agree with. tests/corpus.rs checks the engine against
both files, so the comparison is repeated by `cargo test` without Python.
"""

from __future__ import annotations

import subprocess
import sys
import warnings
from pathlib import Path

import ozaru
import pypdf

ENGINE = Path(__file__).resolve().parents[2]
ROOT = ENGINE.parents[1]
FOLDERS = ["crates/ozaru-engine/tests/corpus", "crates/ozaru-engine/tests/files", "docs/assets", "tests"]
PASSWORD = "user"


def files() -> list[str]:
    return sorted(str(path.relative_to(ROOT)) for folder in FOLDERS for path in (ROOT / folder).glob("*.pdf"))


def with_ozaru(path: Path) -> str:
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            return str(ozaru.page_count(str(path)))
    except Exception:
        return "error"


def with_pypdf(path: Path) -> str:
    try:
        reader = pypdf.PdfReader(path)
        if reader.is_encrypted:
            reader.decrypt(PASSWORD)
        return str(len(reader.pages))
    except Exception:
        return "error"


def page_attributes(path: Path) -> list[str]:
    """One line per page, for the first 50: media box, crop box and rotation, as
    pypdf reads them, with inherited attributes. Nothing if pypdf can't read the
    file."""
    try:
        reader = pypdf.PdfReader(path)
        if reader.is_encrypted:
            reader.decrypt(PASSWORD)
        lines = []
        for index, page in enumerate(reader.pages[:50]):
            boxes = [page.mediabox, page.cropbox]
            numbers = [f"{float(value):g}" for box in boxes for value in (box.left, box.bottom, box.right, box.top)]
            lines.append("\t".join([str(index), *numbers, str(page.rotation % 360)]))
        return lines
    except Exception:
        return []


def with_pdfinfo(path: Path) -> str:
    # Poppler rejects a wrong password even if the empty one works, so it is
    # tried without one first.
    for options in [[], ["-upw", PASSWORD]]:
        result = subprocess.run(["pdfinfo", *options, str(path)], capture_output=True, text=True)
        for line in result.stdout.splitlines():
            if line.startswith("Pages:"):
                return line.split()[1]
    return "error"


def with_engine(names: list[str]) -> dict[str, str]:
    result = subprocess.run(
        ["cargo", "run", "--quiet", "--package", "ozaru-engine", "--example", "page_count", "--", "--password", PASSWORD, *names],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    counts = {}
    for line in result.stdout.splitlines():
        name, count = line.split("\t")
        counts[name] = "error" if count.startswith("error") else count
    return counts


def main() -> int:
    names = files()
    engine = with_engine(names)
    rows = []
    disagreements = []
    for name in names:
        path = ROOT / name
        references = [with_ozaru(path), with_pypdf(path), with_pdfinfo(path)]
        rows.append((name, *references))
        agreed = set(references) == {references[0]}
        if agreed and engine[name] != references[0]:
            disagreements.append(name)
        mark = "" if engine[name] == references[0] or not agreed else "  <-- differs"
        if not agreed:
            mark = "  (the references disagree)"
        print(f"{name:65} engine {engine[name]:>6}  ozaru {references[0]:>6}  pypdf {references[1]:>6}  pdfinfo {references[2]:>6}{mark}")

    with open(Path(__file__).parent / "expected.tsv", "w", encoding="utf-8") as out:
        out.write("# Page counts of other readers, written by compare.py.\n")
        out.write("file\tozaru-0.1.7\tpypdf\tpdfinfo\n")
        for row in rows:
            out.write("\t".join(row) + "\n")

    with open(Path(__file__).parent / "pages.tsv", "w", encoding="utf-8") as out:
        out.write("# Media box, crop box and rotation of each page as pypdf reads them, written by compare.py.\n")
        out.write("file\tpage\tmedia box (4)\tcrop box (4)\trotation\n")
        for name in names:
            for line in page_attributes(ROOT / name):
                out.write(f"{name}\t{line}\n")

    print(f"\n{len(names)} files, {len(disagreements)} where the engine differs from agreeing references")
    return 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
