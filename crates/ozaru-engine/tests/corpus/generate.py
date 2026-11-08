"""Generates the PDFs of the engine's test corpus.

Every file is made here, from scratch. The `fonts-*` files embed subsets of the
DejaVu and Droid Sans Fallback fonts, as any document written with them does;
see MANIFEST.md for their licenses and for what each file is for.

Needs Python 3.10+ with reportlab, pikepdf and pycairo, Ghostscript 10 as `gs`,
Poppler's `pdftocairo`, and the DejaVu and Droid Sans Fallback fonts, found with
fontconfig's `fc-match`:

    pip install reportlab pikepdf pycairo
    python generate.py

The files are written next to this script. ReportLab, the writer here and qpdf
write the same bytes each time, except that Ghostscript stamps the current date
and AES encryption uses random initialization vectors.
"""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

import cairo
import pikepdf
from reportlab.lib.pagesizes import A4, A5, letter, landscape
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

HERE = Path(__file__).parent


# ReportLab: ordinary files, as most PDF writers make them.


def reportlab(name: str, pages: list[tuple[tuple[float, float], int]], **info: str) -> Path:
    path = HERE / name
    pdf = canvas.Canvas(str(path), invariant=1)
    pdf.setTitle(info.get("title", name))
    pdf.setAuthor(info.get("author", "ozaru"))
    for number, (size, rotation) in enumerate(pages, start=1):
        pdf.setPageSize(size)
        pdf.setPageRotation(rotation)
        pdf.setFont("Helvetica", 12)
        pdf.drawString(72, size[1] - 72, f"Page {number} of {len(pages)}")
        pdf.showPage()
    pdf.save()
    return path


# A minimal writer, for page trees that no tool writes: deep, balanced, with
# inherited attributes, or updated incrementally.

FONT = b"<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >>"


def content(text: str) -> bytes:
    data = f"BT /F1 12 Tf 72 720 Td ({text}) Tj ET".encode()
    return b"<< /Length %d >>\nstream\n%s\nendstream" % (len(data), data)


def write(
    objects: dict[int, bytes],
    trailer: bytes,
    version: str = "1.7",
    base: bytes = b"",
    previous: int | None = None,
) -> bytes:
    """A file with `objects`, or an incremental update of `base` with them."""
    out = bytearray(base or f"%PDF-{version}\n%\xe2\xe3\xcf\xd3\n".encode("latin-1"))
    offsets = {}
    for number in sorted(objects):
        offsets[number] = len(out)
        out += b"%d 0 obj\n%s\nendobj\n" % (number, objects[number])
    start = len(out)
    out += b"xref\n"
    if previous is None:
        out += b"0 1\n0000000000 65535 f \n"
    for number in sorted(offsets):
        out += b"%d 1\n%010d 00000 n \n" % (number, offsets[number])
    if previous is not None:
        trailer += b" /Prev %d" % previous
    out += b"trailer\n<< %s >>\nstartxref\n%d\n%%%%EOF\n" % (trailer, start)
    return bytes(out)


def page(parent: int, contents: int, extra: bytes = b"") -> bytes:
    return b"<< /Type /Page /Parent %d 0 R /Contents %d 0 R %s >>" % (parent, contents, extra)


def deep_tree() -> bytes:
    # A chain of 40 page tree nodes, with pages at depths 1, 20 and 40.
    objects = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 3: content("deep")}
    depth = 40
    nodes = list(range(10, 10 + depth))
    pages = {nodes[0]: 100, nodes[19]: 101, nodes[-1]: 102}
    objects[2] = b"<< /Type /Pages /Kids [%d 0 R] /Count 3 /MediaBox [0 0 612 792] /Resources %s >>" % (nodes[0], FONT)
    for index, node in enumerate(nodes):
        parent = 2 if index == 0 else nodes[index - 1]
        kids = []
        if node in pages:
            kids.append(pages[node])
        if index + 1 < depth:
            kids.append(nodes[index + 1])
        below = sum(1 for n in nodes[index:] if n in pages)
        kid_refs = b" ".join(b"%d 0 R" % kid for kid in kids)
        objects[node] = b"<< /Type /Pages /Parent %d 0 R /Kids [%s] /Count %d >>" % (parent, kid_refs, below)
    for node, number in pages.items():
        objects[number] = page(node, 3)
    return write(objects, b"/Size 103 /Root 1 0 R")


def balanced_tree(count: int = 1000, fanout: int = 10) -> bytes:
    # Pages in groups of `fanout`, and groups of groups, as large writers do.
    objects = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 3: content("balanced")}
    next_number = 10
    level = []
    for _ in range(count):
        level.append(next_number)
        next_number += 1
    counts = {number: 1 for number in level}
    parents = {}
    while len(level) > fanout:
        groups = [level[i : i + fanout] for i in range(0, len(level), fanout)]
        level = []
        for group in groups:
            node = next_number
            next_number += 1
            for kid in group:
                parents[kid] = node
            counts[node] = sum(counts[kid] for kid in group)
            objects[node] = group  # filled in below
            level.append(node)
    for kid in level:
        parents[kid] = 2
    objects[2] = b"<< /Type /Pages /Kids [%s] /Count %d /MediaBox [0 0 595 842] /Resources %s >>" % (
        b" ".join(b"%d 0 R" % kid for kid in level),
        count,
        FONT,
    )
    for number in range(10, next_number):
        if number in objects and isinstance(objects[number], list):
            kids = objects[number]
            objects[number] = b"<< /Type /Pages /Parent %d 0 R /Kids [%s] /Count %d >>" % (
                parents[number],
                b" ".join(b"%d 0 R" % kid for kid in kids),
                counts[number],
            )
        elif number not in objects:
            objects[number] = page(parents[number], 3)
    return write(objects, b"/Size %d /Root 1 0 R" % next_number)


def inherited() -> bytes:
    # Attributes set on page tree nodes, and replaced by some pages:
    #   2 (A4, Rotate 90, fonts) -- 4 (Letter) -- 10, 11 (Rotate 0)
    #                            -- 12 (A5 crop box, Rotate -90)
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [4 0 R 12 0 R] /Count 3 /MediaBox [0 0 595 842] /Rotate 90 /Resources %s >>" % FONT,
        3: content("inherited"),
        4: b"<< /Type /Pages /Parent 2 0 R /Kids [10 0 R 11 0 R] /Count 2 /MediaBox [0 0 612 792] >>",
        10: page(4, 3),
        11: page(4, 3, b"/Rotate 0"),
        12: page(2, 3, b"/CropBox [0 0 420 595] /Rotate -90"),
    }
    return write(objects, b"/Size 13 /Root 1 0 R")


def incremental() -> bytes:
    # Two pages, then an update that adds a third, then one that removes the
    # first and frees its object.
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 /MediaBox [0 0 612 792] /Resources %s >>" % FONT,
        3: content("incremental"),
        4: page(2, 3),
        5: page(2, 3),
    }
    file = write(objects, b"/Size 6 /Root 1 0 R")
    first_start = int(file.rsplit(b"startxref\n", 1)[1].split(b"\n")[0])
    update = {
        2: b"<< /Type /Pages /Kids [4 0 R 5 0 R 6 0 R] /Count 3 /MediaBox [0 0 612 792] /Resources %s >>" % FONT,
        6: page(2, 3),
    }
    file = write(update, b"/Size 7 /Root 1 0 R", base=file, previous=first_start)
    second_start = int(file.rsplit(b"startxref\n", 1)[1].split(b"\n")[0])
    # The last update frees object 4 with a free entry of generation 1.
    removal = (
        b"2 0 obj\n<< /Type /Pages /Kids [5 0 R 6 0 R] /Count 2 /MediaBox [0 0 612 792] /Resources "
        + FONT
        + b" >>\nendobj\n"
    )
    start = len(file) + len(removal)
    tail = (
        b"xref\n0 1\n0000000004 65535 f \n2 1\n%010d 00000 n \n4 1\n0000000000 00001 f \n"
        b"trailer\n<< /Size 7 /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n"
    ) % (len(file), second_start, start)
    return file + removal + tail


def version_1_0() -> bytes:
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 612 792] /Resources %s >>" % FONT,
        3: content("PDF 1.0"),
        4: page(2, 3),
    }
    return write(objects, b"/Size 5 /Root 1 0 R", version="1.0")


# Ghostscript and qpdf: other writers, versions and features.


def ghostscript(source: Path, name: str, *options: str) -> None:
    subprocess.run(
        [
            "gs",
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dSAFER",
            "-sDEVICE=pdfwrite",
            "-dAutoRotatePages=/None",
            *options,
            f"-sOutputFile={HERE / name}",
            str(source),
        ],
        check=True,
    )


def qpdf(source: Path, name: str, **options) -> None:
    # qpdf can't derive the ID of an encrypted file from its content, so those
    # get a fixed one.
    identity = {"static_id": True} if "encryption" in options else {"deterministic_id": True}
    with pikepdf.open(source) as pdf:
        pdf.save(HERE / name, **identity, **options)


# Fonts: text in many scripts, with the fonts embedded the way each writer does
# it, and the standard 14 fonts, which are never embedded.

TEXT = [
    "Año ñandú, café y pingüino. ¿Qué tal? ¡Olé!",
    "Ελληνικά: αβγδε ΣΩ, Кириллица: привет",
    "€ — “comillas” ‘simples’ … • ‰ ™ © ®",
    "Ligaduras: fi fl ffi, números 0123456789",
]
CJK = ["中文文本：你好，世界。日本語のテキスト。", "한국어 텍스트"]
RIGHT_TO_LEFT = ["עברית שלום", "العربية مرحبا"]

STANDARD_14 = [
    "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
    "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
    "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
]


def font_file(family: str) -> str:
    return subprocess.run(["fc-match", "-f", "%{file}", family], capture_output=True, text=True, check=True).stdout


def reportlab_truetype() -> Path:
    """DejaVu Sans and Serif as TrueType subsets, with ToUnicode CMaps."""
    path = HERE / "fonts-reportlab-ttf.pdf"
    pdf = canvas.Canvas(str(path), invariant=1)
    pdfmetrics.registerFont(TTFont("DejaVu", font_file("DejaVu Sans")))
    pdfmetrics.registerFont(TTFont("DejaVuSerif", font_file("DejaVu Serif")))
    for index, line in enumerate(TEXT):
        pdf.setFont("DejaVu" if index % 2 == 0 else "DejaVuSerif", 12)
        pdf.drawString(50, 800 - 20 * index, line)
    pdf.showPage()
    pdf.save()
    return path


def cairo_text(name: str, family: str, lines: list[str]) -> Path:
    """cairo writes CID fonts (Type 0) and TrueType subsets."""
    path = HERE / name
    surface = cairo.PDFSurface(str(path), *A4)
    surface.set_metadata(cairo.PDFMetadata.CREATE_DATE, "2026-01-01T00:00:00")
    context = cairo.Context(surface)
    context.select_font_face(family)
    context.set_font_size(12)
    for index, line in enumerate(lines):
        context.move_to(50, 40 + 20 * index)
        context.show_text(line)
    context.select_font_face("DejaVu Serif", cairo.FONT_SLANT_ITALIC, cairo.FONT_WEIGHT_BOLD)
    context.move_to(50, 300)
    context.show_text("Cursiva negrita: áéíóú ÁÉÍÓÚ")
    surface.finish()
    return path


def standard_14() -> None:
    """Text in each of the standard 14 fonts, with their built-in encodings."""
    pdf = canvas.Canvas(str(HERE / "fonts-standard-14.pdf"), invariant=1)
    y = 800
    for font in STANDARD_14:
        pdf.setFont(font, 11)
        pdf.drawString(50, y, f"{font}: Año café pingüino € “ok” — fin")
        y -= 18
    pdf.setFont("Symbol", 11)
    pdf.drawString(50, y, "αβγδε ΣΩ ≥ ∞ ≈ ∑")
    pdf.setFont("ZapfDingbats", 11)
    pdf.drawString(50, y - 18, "✁✂✓✔❤➔")
    pdf.showPage()
    pdf.save()


def fonts() -> None:
    ttf = reportlab_truetype()
    cairo_text("fonts-cairo-latin.pdf", "DejaVu Sans", TEXT)
    cairo_text("fonts-cairo-cjk.pdf", "Droid Sans Fallback", TEXT + CJK)
    cairo_text("fonts-cairo-rtl.pdf", "DejaVu Sans", TEXT + RIGHT_TO_LEFT)
    ghostscript(ttf, "fonts-gs.pdf")
    subprocess.run(["pdftocairo", "-pdf", str(ttf), str(HERE / "fonts-poppler.pdf")], check=True)
    standard_14()


def main() -> None:
    reportlab("one-page.pdf", [(A4, 0)])
    many = reportlab("many-pages.pdf", [(A4, 0)] * 500)
    sizes = reportlab(
        "sizes-and-rotations.pdf",
        [(A4, 0), (letter, 0), (landscape(A4), 0), (A5, 90), (A4, 180), (letter, 270)],
    )
    reportlab(
        "unicode-info.pdf",
        [(A4, 0)],
        title="Informe año 2026 — ñandú ✓",
        author="José Núñez",
    )

    for name, data in [
        ("deep-tree.pdf", deep_tree()),
        ("balanced-tree.pdf", balanced_tree()),
        ("inherited-attributes.pdf", inherited()),
        ("incremental-updates.pdf", incremental()),
        ("pdf-1.0.pdf", version_1_0()),
    ]:
        (HERE / name).write_bytes(data)

    ghostscript(sizes, "pdf-1.2.pdf", "-dCompatibilityLevel=1.2")
    ghostscript(sizes, "pdf-2.0.pdf", "-dCompatibilityLevel=2.0")
    ghostscript(many, "many-pages-gs.pdf")

    compressed = pikepdf.ObjectStreamMode.generate
    qpdf(sizes, "object-streams.pdf", object_stream_mode=compressed)
    qpdf(sizes, "linearized.pdf", linearize=True)
    qpdf(HERE / "balanced-tree.pdf", "balanced-tree-compressed.pdf", object_stream_mode=compressed)
    qpdf(sizes, "aes-128.pdf", encryption=pikepdf.Encryption(owner="owner", user="", R=4, aes=True))
    qpdf(sizes, "aes-256.pdf", encryption=pikepdf.Encryption(owner="owner", user="", R=6))
    qpdf(
        sizes,
        "aes-256-user-password.pdf",
        encryption=pikepdf.Encryption(owner="owner", user="user", R=6),
        object_stream_mode=compressed,
    )
    special_passwords()
    fonts()


def special_passwords() -> None:
    """Passwords that need SASLprep (revision 6) or a single-byte encoding
    (revision 4) to open the file, as they would be typed."""
    one_page = HERE / "one-page.pdf"
    for name, user, revision in [
        ("aes-256-password-ix.pdf", "IX", 6),
        ("aes-256-password-space.pdf", "a b", 6),
        ("aes-128-password-accent.pdf", "año", 4),
    ]:
        encryption = pikepdf.Encryption(owner="owner", user=user, R=revision, aes=True)
        qpdf(one_page, name, encryption=encryption)


if __name__ == "__main__":
    main()
