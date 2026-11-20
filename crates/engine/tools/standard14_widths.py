"""Writes src/fonts/data/standard14-widths.txt from Adobe's Core 14 AFM files.

The AFM files give the metrics of the standard 14 fonts. Only the name and width
of each glyph are kept, with each file's copyright notices, and the result is
marked as modified, as Adobe's terms in MustRead.html ask. The AFM files can be
found, unmodified, in Apache PDFBox and in PDFKit:

    python standard14_widths.py path/to/afm/folder
"""

from __future__ import annotations

import sys
from pathlib import Path

FONTS = [
    "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
    "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
    "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
    "Symbol", "ZapfDingbats",
]
OUTPUT = Path(__file__).resolve().parents[1] / "src/fonts/data/standard14-widths.txt"

HEADER = """\
# The glyph widths of the standard 14 fonts.
#
# MODIFIED: extracted by ozaru from Adobe's Core 14 AFM files, keeping only the
# name and width (WX) of each glyph and each file's copyright notices. The
# terms are in MustRead.html, next to this file.
#
# Format: a line `font <FontName>`, the notices of its AFM file as comments,
# then one line per glyph: its name and its width in thousandths of an em.
"""


def main(folder: Path) -> None:
    lines = [HEADER]
    for font in FONTS:
        lines.append(f"font {font}")
        for line in (folder / f"{font}.afm").read_text(encoding="latin-1").splitlines():
            line = line.strip()
            if line.startswith(("Comment Copyright", "Notice")):
                lines.append(f"# {line}")
            elif line.startswith("C "):
                fields = dict(
                    part.strip().split(" ", 1) for part in line.split(";") if part.strip()
                )
                lines.append(f"{fields['N']} {fields['WX']}")
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
