"""Makes the references of the rendering tests: the pages `render.tsv` lists,
as MuPDF draws them with PyMuPDF at 72 dots per inch, in `render/`, one file
for each page: the width and height as two 32-bit numbers, then the red,
green and blue of every pixel, deflated.

It also writes how far the engine may be from them: the mean difference per
sample, and the share of pixels that differ by more than 64 of 255, as the
engine draws them now, with a margin. Run it after a change that moves them,
after building the engine's `render` example:

    cargo build --release --example render
    python render.py

Needs PyMuPDF and Pillow.
"""

from __future__ import annotations

import io
import struct
import subprocess
import zlib
from pathlib import Path

import pymupdf
from PIL import Image, ImageChops

HERE = Path(__file__).resolve().parent
ENGINE = HERE.parent.parent.parent.parent / "target" / "release" / "examples" / "render"

# The pages to draw: file, page numbers from 0, and why they are here.
PAGES = [
    ("content-operators.pdf", [0], "text operators, forms, marked content"),
    ("fonts-cairo-latin.pdf", [0], "embedded TrueType"),
    ("fonts-cairo-cjk.pdf", [0], "embedded CID fonts"),
    ("fonts-gs.pdf", [0], "embedded TrueType"),
    ("fonts-poppler.pdf", [0], "embedded TrueType"),
    ("fonts-reportlab-ttf.pdf", [0], "embedded TrueType"),
    ("fonts-pango-rtl.pdf", [0], "right to left text"),
    ("fonts-standard-14.pdf", [0], "the standard 14 fonts, not embedded"),
    ("fonts-type1.pdf", [0], "embedded Type 1"),
    ("fonts-cff.pdf", [0], "embedded CFF"),
    ("fonts-type3.pdf", [0], "Type 3 fonts"),
    ("transparency.pdf", [0], "opacity, groups, blend modes and soft masks"),
    ("patterns.pdf", [0], "tiling patterns, shading patterns and the sh operator"),
    ("images-jbig2.pdf", [0], "JBIG2 images"),
    ("images-separation.pdf", [0], "spot colors"),
    ("annotations.pdf", [0, 1], "annotations and form widgets with their appearances"),
    ("one-page.pdf", [0], "plain text"),
    ("sizes-and-rotations.pdf", [0, 1, 2, 3, 4, 5], "page sizes and rotations"),
]


def main() -> None:
    folder = HERE / "render"
    folder.mkdir(exist_ok=True)
    rows = ["# file\tpage\tmax mean\tmax share over 64"]
    for name, pages, _ in PAGES:
        document = pymupdf.open(HERE / name)
        for page in pages:
            pixmap = document[page].get_pixmap(matrix=pymupdf.Matrix(1, 1), alpha=False)
            theirs = Image.open(io.BytesIO(pixmap.tobytes("png"))).convert("RGB")
            reference = struct.pack(">II", *theirs.size) + theirs.tobytes()
            (folder / f"{name[:-4]}-{page}.rgb.z").write_bytes(zlib.compress(reference, 9))
            out = folder / "ours.png"
            subprocess.run([str(ENGINE), str(HERE / name), str(page), str(out), "1"], check=True, capture_output=True)
            ours = Image.open(out).convert("RGB")
            assert ours.size == theirs.size, (name, page, ours.size, theirs.size)
            histogram = ImageChops.difference(ours, theirs).convert("L").histogram()
            pixels = ours.size[0] * ours.size[1]
            mean = sum(i * c for i, c in enumerate(histogram)) / pixels
            share = sum(histogram[65:]) / pixels
            rows.append(f"{name}\t{page}\t{mean * 1.3 + 0.02:.3f}\t{share * 1.3 + 0.0005:.5f}")
            out.unlink()
    (HERE / "render.tsv").write_text("\n".join(rows) + "\n")


if __name__ == "__main__":
    main()
