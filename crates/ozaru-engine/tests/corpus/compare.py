"""Compares the page count of the engine with other readers, on every PDF of
the repository, and writes the references to expected.tsv. The media box, crop
box and rotation of the first 50 pages of each file, as pypdf reads them, go to
pages.tsv, and the ToUnicode CMap of each font, as pdfminer reads it, goes to
tounicode.tsv. The text and width of each code of each simple font, also as
pdfminer reads them, go to fonts.tsv.

The references are ozaru 0.1.7, which reads PDFs with pdf_oxide, pypdf, and
pdfinfo from Poppler. Files that can't be read without a password are opened
with "user", the user password of the corpus, except by ozaru 0.1.7, which
can't take one.

    pip install ozaru==0.1.7 pypdf cryptography pdfminer.six
    python compare.py

It prints a table and exits with 1 if the engine disagrees with a reference
that the other references agree with. A file that every reference fails on but
the engine reads is shown as a repair, not a disagreement. tests/corpus.rs checks the engine against
both files, so the comparison is repeated by `cargo test` without Python.
"""

from __future__ import annotations

import subprocess
import sys
import warnings
from pathlib import Path

import io

import ozaru
import pypdf
from pdfminer.cmapdb import CMapParser, FileUnicodeMap
from pdfminer.pdfdocument import PDFDocument
from pdfminer.pdffont import PDFUnicodeNotDefined
from pdfminer.pdfinterp import PDFResourceManager
from pdfminer.pdfparser import PDFParser

ENGINE = Path(__file__).resolve().parents[2]
ROOT = ENGINE.parents[1]
FOLDERS = ["crates/ozaru-engine/tests/corpus", "crates/ozaru-engine/tests/files", "docs/assets", "tests"]
PASSWORD = "user"
# Files whose user password isn't "user", as generate.py makes them.
PASSWORDS = {
    "aes-256-password-ix.pdf": "IX",
    "aes-256-password-space.pdf": "a b",
    "aes-128-password-accent.pdf": "año",
}


def password_for(path: Path) -> str:
    return PASSWORDS.get(path.name, PASSWORD)


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
            reader.decrypt(password_for(path))
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
            reader.decrypt(password_for(path))
        lines = []
        for index, page in enumerate(reader.pages[:50]):
            boxes = [page.mediabox, page.cropbox]
            numbers = [f"{float(value):g}" for box in boxes for value in (box.left, box.bottom, box.right, box.top)]
            lines.append("\t".join([str(index), *numbers, str(page.rotation % 360)]))
        return lines
    except Exception:
        return []


def to_unicode_maps(path: Path) -> list[str]:
    """One line per code of each ToUnicode CMap, as pdfminer reads it: the object
    number of the font, the code and its text as hexadecimal code points.
    Nothing for files that pdfminer can't read without a password."""
    lines = []
    try:
        with open(path, "rb") as file:
            document = PDFDocument(PDFParser(file))
            fonts = {}
            for xref in document.xrefs:
                for number in xref.get_objids():
                    try:
                        dictionary = document.getobj(number)
                    except Exception:
                        continue
                    if isinstance(dictionary, dict) and "ToUnicode" in dictionary:
                        fonts[number] = dictionary["ToUnicode"]
            for number, stream in sorted(fonts.items()):
                cmap = FileUnicodeMap()
                CMapParser(cmap, io.BytesIO(stream.resolve().get_data())).run()
                for code, text in sorted(cmap.cid2unichr.items()):
                    points = " ".join(f"{ord(char):04X}" for char in text)
                    lines.append(f"{number}\t{code}\t{points}")
    except Exception:
        return []
    return lines


SIMPLE_FONTS = {"Type1", "MMType1", "TrueType", "Type3"}


# Fonts that aren't embedded and have no widths, already written: they give the
# same in every file, so each is written once.
WRITTEN: set[tuple[str, str]] = set()


def simple_fonts(path: Path) -> list[str]:
    """One line per code of each simple font, as pdfminer reads it: the object
    number of the font, the code, its text as hexadecimal code points (`-` if it
    has none) and its width in text space if the font has /Widths (`-` if not).
    Nothing for files that pdfminer can't read without a password."""
    lines = []
    try:
        with open(path, "rb") as file:
            document = PDFDocument(PDFParser(file))
            manager = PDFResourceManager()
            for xref in document.xrefs:
                for number in sorted(xref.get_objids()):
                    try:
                        spec = document.getobj(number)
                    except Exception:
                        continue
                    if not isinstance(spec, dict) or getattr(spec.get("Type"), "name", None) != "Font":
                        continue
                    if getattr(spec.get("Subtype"), "name", None) not in SIMPLE_FONTS:
                        continue
                    has_widths = "Widths" in spec
                    if not has_widths and "ToUnicode" not in spec:
                        key = (str(spec.get("BaseFont")), str(spec.get("Encoding")))
                        if key in WRITTEN:
                            continue
                        WRITTEN.add(key)
                    font = manager.get_font(number, spec)
                    for code in range(256):
                        try:
                            text = " ".join(f"{ord(char):04X}" for char in font.to_unichr(code))
                        except (PDFUnicodeNotDefined, KeyError):
                            text = "-"
                        width = repr(float(font.char_width(code))) if has_widths else "-"
                        if text != "-" or width not in ("-", "0.0"):
                            lines.append(f"{number}\t{code}\t{text}\t{width}")
    except Exception:
        return []
    return lines


def with_pdfinfo(path: Path) -> str:
    # Poppler rejects a wrong password even if the empty one works, so it is
    # tried without one first.
    for options in [[], ["-upw", password_for(path)]]:
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
        # A file that every reference fails on, but the engine reads, is a repair
        # the references can't make. tests/corpus.rs checks its count by hand.
        repaired = agreed and references[0] == "error" and engine[name] != "error"
        if agreed and engine[name] != references[0] and not repaired:
            disagreements.append(name)
        mark = "" if engine[name] == references[0] or not agreed else "  <-- differs"
        if not agreed:
            mark = "  (the references disagree)"
        if repaired:
            mark = "  (only the engine reads it)"
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

    with open(Path(__file__).parent / "tounicode.tsv", "w", encoding="utf-8") as out:
        out.write("# The ToUnicode CMap of each font as pdfminer reads it, written by compare.py.\n")
        out.write("file\tfont object\tcode\ttext (code points)\n")
        for name in names:
            for line in to_unicode_maps(ROOT / name):
                out.write(f"{name}\t{line}\n")

    with open(Path(__file__).parent / "fonts.tsv", "w", encoding="utf-8") as out:
        out.write("# The text and width of each code of each simple font as pdfminer reads them, written by compare.py.\n")
        out.write("file in this folder\tfont object\tcode\ttext (code points)\twidth\n")
        # Only the corpus: the other PDFs of the repository repeat its fonts.
        for name in names:
            if not name.startswith(FOLDERS[0]):
                continue
            for line in simple_fonts(ROOT / name):
                out.write(f"{Path(name).name}\t{line}\n")

    print(f"\n{len(names)} files, {len(disagreements)} where the engine differs from agreeing references")
    return 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
