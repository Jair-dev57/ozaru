"""Compares the page count of the engine with other readers, on every PDF of
the repository, and writes the references to expected.tsv. The media box, crop
box and rotation of the first 50 pages of each file, as pypdf reads them, go to
pages.tsv, and the ToUnicode CMap of each font, as pdfminer reads it, goes to
tounicode.tsv. The text and width of each code of each simple font, also as
pdfminer reads them, go to fonts.tsv, and those of each CID of each composite
font to composite.tsv. Where pdfminer places each glyph of the first pages of
each file, with its font and size, goes to glyphs.tsv.

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
from pdfminer.converter import PDFPageAggregator
from pdfminer.pdfdocument import PDFDocument
from pdfminer.pdffont import PDFUnicodeNotDefined
from pdfminer.pdfinterp import PDFPageInterpreter, PDFResourceManager
from pdfminer.pdfpage import PDFPage
from pdfminer.pdfparser import PDFParser
from pdfminer.pdftypes import dict_value, list_value, num_value
from pdfminer.psparser import PSLiteral
from pdfminer.utils import translate_matrix

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
    has none) and its width in text space, from /Widths or, for the standard 14
    fonts without it, from their metrics.
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
                        width = repr(float(font.char_width(code)))
                        if text != "-" or width != "0.0":
                            lines.append(f"{number}\t{code}\t{text}\t{width}")
    except Exception:
        return []
    return lines


def composite_fonts(path: Path) -> list[str]:
    """One line per CID of each composite font that pdfminer finds text or a
    width for: the object number of the font, its CMap, the CID, its text as
    hexadecimal code points (`-` if it has none) and its width in text space."""
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
                    if not isinstance(spec, dict) or getattr(spec.get("Subtype"), "name", None) != "Type0":
                        continue
                    font = manager.get_font(number, spec)
                    cmap = getattr(spec.get("Encoding"), "name", "stream")
                    cids = set(font.widths)
                    if font.unicode_map is not None:
                        cids |= set(font.unicode_map.cid2unichr)
                    for cid in sorted(cids):
                        try:
                            text = " ".join(f"{ord(char):04X}" for char in font.to_unichr(cid))
                        except (PDFUnicodeNotDefined, KeyError):
                            text = "-"
                        lines.append(f"{number}\t{cmap}\t{cid}\t{text}\t{float(font.char_width(cid))!r}")
    except Exception:
        return []
    return lines


# The pages of each file whose glyphs go to glyphs.tsv: the rest repeat them.
GLYPH_PAGES = 10


class _FontNames(PDFResourceManager):
    """Remembers the /BaseFont of each font it reads."""

    def __init__(self) -> None:
        super().__init__()
        self.names: dict[int, str] = {}

    def get_font(self, objid, spec):
        font = super().get_font(objid, spec)
        name = spec.get("BaseFont") if isinstance(spec, dict) else None
        self.names[id(font)] = name.name if isinstance(name, PSLiteral) else "-"
        return font


class _Glyphs(PDFPageAggregator):
    """Records each glyph as pdfminer renders it."""

    def __init__(self, manager: _FontNames) -> None:
        super().__init__(manager)
        self.manager = manager
        self.glyphs: list[tuple[str, float, float, float, float]] = []

    def render_char(self, matrix, font, fontsize, scaling, rise, cid, ncs, graphicstate):
        advance = super().render_char(matrix, font, fontsize, scaling, rise, cid, ncs, graphicstate)
        # The origin of the glyph: the matrix maps the unscaled text space, so
        # the rise is still to be added.
        a, b, c, d, e, f = matrix
        name = self.manager.names.get(id(font), "-")
        self.glyphs.append((name, fontsize, c * rise + e, d * rise + f, font.char_width(cid)))
        return advance

    def render_string_horizontal(self, seq, matrix, pos, font, fontsize, scaling, charspace, wordspace, rise, dxscale, ncs, graphicstate):
        """Places the glyphs of a string as section 9.4.4 says: each one moves
        the position by its width, the character spacing and, for code 32, the
        word spacing. pdfminer leaves out the character spacing after the last
        glyph of each string."""
        x, y = pos
        for item in seq:
            if isinstance(item, (int, float)):
                x -= item * dxscale
            elif isinstance(item, bytes):
                for cid in font.decode(item):
                    x += self.render_char(translate_matrix(matrix, (x, y)), font, fontsize, scaling, rise, cid, ncs, graphicstate)
                    x += charspace + (wordspace if cid == 32 else 0)
        return (x, y)


class _DefaultUserSpace(PDFPageInterpreter):
    """Runs pages without moving them to the origin or rotating them, so the
    glyphs are in the default user space of the page, as the engine gives
    them. It also corrects four places where pdfminer departs from the
    standard, which PyMuPDF follows: the `"` operator, the character spacing
    after the last glyph of a string, the font of an ExtGState, and the text
    state that forms start with."""

    def dup(self):
        """The interpreter of a form, which pdfminer starts with a new text
        state: section 8.10.1 says a form starts with the graphics state of
        what paints it, the text state included."""
        child = super().dup()
        child.inherited_text = self.textstate.copy()
        return child

    def init_state(self, ctm) -> None:
        super().init_state(ctm)
        inherited = getattr(self, "inherited_text", None)
        if inherited is not None:
            self.textstate = inherited

    def do_gs(self, name) -> None:
        """Sets the font of an ExtGState's /Font, as table 57 says, which
        pdfminer leaves out."""
        states = dict_value(self.resources.get("ExtGState", {}))
        state = dict_value(states.get(getattr(name, "name", name), {}))
        if "Font" in state:
            reference, size = list_value(state["Font"])
            self.textstate.font = self.rsrcmgr.get_font(getattr(reference, "objid", None), dict_value(reference))
            self.textstate.fontsize = num_value(size)

    def do__w(self, aw, ac, s) -> None:
        """The `"` operator, which pdfminer runs without moving to the next
        line, as section 9.4.3 says it does."""
        self.do_Tw(aw)
        self.do_Tc(ac)
        self.do__q(s)

    def process_page(self, page) -> None:
        ctm = (1, 0, 0, 1, 0, 0)
        self.device.begin_page(page, ctm)
        self.render_contents(page.resources, page.contents, ctm=ctm)
        self.device.end_page(page)


def glyphs(path: Path) -> list[str]:
    """One line per glyph of the first pages, as pdfminer places them: the page,
    the glyph's index on it, its font's /BaseFont (`-` if it has none), the font
    size, the x and y of its origin in the default user space of the page, and
    its width in text space.
    Nothing for files that pdfminer can't read."""
    # pdfminer doesn't try the empty password when given another one.
    for password in ["", password_for(path)]:
        lines = []
        try:
            with open(path, "rb") as file:
                manager = _FontNames()
                device = _Glyphs(manager)
                interpreter = _DefaultUserSpace(manager, device)
                pages = PDFPage.get_pages(file, password=password, maxpages=GLYPH_PAGES)
                for number, page in enumerate(pages, start=1):
                    device.glyphs = []
                    interpreter.process_page(page)
                    for index, (font, size, x, y, width) in enumerate(device.glyphs):
                        lines.append(f"{number}\t{index}\t{font}\t{float(size)!r}\t{x:.3f}\t{y:.3f}\t{float(width)!r}")
            return lines
        except Exception:
            continue
    return []


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

    with open(Path(__file__).parent / "composite.tsv", "w", encoding="utf-8") as out:
        out.write("# The text and width of each CID of each composite font as pdfminer reads them, written by compare.py.\n")
        out.write("file in this folder\tfont object\tCMap\tCID\ttext (code points)\twidth\n")
        for name in names:
            if name.startswith(FOLDERS[0]):
                for line in composite_fonts(ROOT / name):
                    out.write(f"{Path(name).name}\t{line}\n")

    with open(Path(__file__).parent / "glyphs.tsv", "w", encoding="utf-8") as out:
        out.write(f"# Each glyph of the first {GLYPH_PAGES} pages of each file as pdfminer places it, written by compare.py.\n")
        out.write("file\tpage\tglyph\tfont\tsize\tx\ty\twidth\n")
        for name in names:
            for line in glyphs(ROOT / name):
                out.write(f"{name}\t{line}\n")

    print(f"\n{len(names)} files, {len(disagreements)} where the engine differs from agreeing references")
    return 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
