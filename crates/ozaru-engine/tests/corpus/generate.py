"""Generates the PDFs of the engine's test corpus.

Every file is made here, from scratch. The `fonts-*` files embed subsets of the
DejaVu and Droid Sans Fallback fonts, as any document written with them does;
see MANIFEST.md for their licenses and for what each file is for.

Needs Python 3.10+ with reportlab, pikepdf, pycairo, fonttools and Pillow with
libtiff, Ghostscript 10 as `gs`,
Poppler's `pdftocairo`, Pango's `pango-view`, and the DejaVu and Droid Sans Fallback fonts, found with
fontconfig's `fc-match`:

    pip install reportlab pikepdf pycairo fonttools
    python generate.py

The files are written next to this script. ReportLab, the writer here and qpdf
write the same bytes each time, except that Ghostscript stamps the current date
and AES encryption uses random initialization vectors.
"""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

import io
import zlib

import cairo
import pikepdf
from fontTools import subset
from fontTools.ttLib import TTFont as FontFile
from fontTools.ttLib.tables._c_m_a_p import cmap_format_4
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


NO_TO_UNICODE = "Año ñandú — Ελληνικά ∑ “ok” ﬁn"


def without_to_unicode() -> None:
    """Fonts without ToUnicode, whose text is only in their embedded programs, as
    older writers make them: a TrueType CIDFont with Identity-H, whose program
    has a Unicode cmap, and a symbolic TrueType font without /Encoding whose
    codes go through a (3, 0) cmap to glyphs with names. The real text of each
    code goes to no-tounicode.tsv."""
    options = subset.Options()
    options.glyph_names = True
    options.notdef_outline = True
    options.name_IDs = ["*"]
    font = FontFile(font_file("DejaVu Sans"))
    subsetter = subset.Subsetter(options)
    subsetter.populate(text=NO_TO_UNICODE)
    subsetter.subset(font)
    order = font.getGlyphOrder()
    best = font.getBestCmap()
    glyph_of = {char: order.index(best[ord(char)]) for char in set(NO_TO_UNICODE)}

    # The CIDFont: codes are glyph numbers, and the program keeps its Unicode
    # cmap and its glyph names.
    cid_program = io.BytesIO()
    font.save(cid_program)
    cid_codes = [glyph_of[char] for char in NO_TO_UNICODE]

    # The symbolic font: one byte codes from 0x21, in a (3, 0) subtable at
    # 0xF000 + code, with no Unicode subtable: only its glyph names say what the
    # glyphs are.
    chars = sorted(set(NO_TO_UNICODE))
    code_of = {char: 0x21 + index for index, char in enumerate(chars)}
    symbol = cmap_format_4(4)
    symbol.platformID, symbol.platEncID, symbol.language = 3, 0, 0
    symbol.cmap = {0xF000 + code_of[char]: best[ord(char)] for char in chars}
    font["cmap"].tables = [symbol]
    symbol_program = io.BytesIO()
    font.save(symbol_program)

    def program(data: bytes) -> bytes:
        packed = zlib.compress(data)
        return b"<< /Length %d /Length1 %d /Filter /FlateDecode >>\nstream\n%s\nendstream" % (
            len(packed), len(data), packed)

    hex_cids = "".join(f"{code:04X}" for code in cid_codes)
    hex_symbols = "".join(f"{code_of[char]:02X}" for char in NO_TO_UNICODE)
    text = f"BT /F1 14 Tf 72 720 Td <{hex_cids}> Tj ET BT /F2 14 Tf 72 680 Td <{hex_symbols}> Tj ET".encode()
    widths = " ".join(str(font["hmtx"][best[ord(char)]][0]) for char in chars)
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R "
        b"/Resources << /Font << /F1 5 0 R /F2 8 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n%s\nendstream" % (len(text), text),
        5: b"<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+DejaVuSans /Encoding /Identity-H "
        b"/DescendantFonts [6 0 R] >>",
        6: b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /ABCDEF+DejaVuSans "
        b"/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> "
        b"/FontDescriptor 7 0 R /CIDToGIDMap /Identity /DW 1000 >>",
        7: b"<< /Type /FontDescriptor /FontName /ABCDEF+DejaVuSans /Flags 4 "
        b"/FontBBox [-1021 -463 1793 1232] /ItalicAngle 0 /Ascent 760 /Descent -240 "
        b"/CapHeight 729 /StemV 87 /FontFile2 10 0 R >>",
        8: b"<< /Type /Font /Subtype /TrueType /BaseFont /BCDEFG+DejaVuSans /FirstChar 33 "
        b"/LastChar %d /Widths [%s] /FontDescriptor 9 0 R >>" % (0x20 + len(chars), widths.encode()),
        9: b"<< /Type /FontDescriptor /FontName /BCDEFG+DejaVuSans /Flags 4 "
        b"/FontBBox [-1021 -463 1793 1232] /ItalicAngle 0 /Ascent 760 /Descent -240 "
        b"/CapHeight 729 /StemV 87 /FontFile2 11 0 R >>",
        10: program(cid_program.getvalue()),
        11: program(symbol_program.getvalue()),
    }
    (HERE / "fonts-no-tounicode.pdf").write_bytes(write(objects, b"/Size 12 /Root 1 0 R"))
    with open(HERE / "no-tounicode.tsv", "w", encoding="utf-8") as out:
        out.write("# The real text of each code of fonts-no-tounicode.pdf, written by generate.py.\n")
        out.write("font object\tcode\ttext (code point)\n")
        for char in chars:
            out.write(f"5\t{glyph_of[char]}\t{ord(char):04X}\n")
        for char in chars:
            out.write(f"8\t{code_of[char]}\t{ord(char):04X}\n")


def content_operators() -> None:
    """Content that the other files don't have: every text operator, a
    rotated matrix, a font set with gs, marked content, an inline image, an
    unknown operator in BX/EX, a Type 3 font, vertical text, nested forms, and
    on a rotated second page, an operation split between two content
    streams."""
    first = (
        b"BT /F1 12 Tf 72 720 Td (Plain text) Tj "
        b"1.5 Tc (spaced) Tj 0 Tc 4 Tw ( word spacing ) Tj 0 Tw "
        b"80 Tz (scaled) Tj 100 Tz 3 Ts (rise) Tj 0 Ts "
        b"14 TL T* (next line) Tj (quote) ' 2 0.5 (double quote) \" "
        b"[(Kern) -250 (ing) 120 (TJ)] TJ "
        b"0 -20 TD (TD line) Tj T* (after TD) Tj ET\n"
        b"q 0.866 0.5 -0.5 0.866 300 400 cm BT /F2 14 Tf 0 0 Td (Rotated by cm) Tj ET Q\n"
        b"BT /F1 10 Tf 0 1 -1 0 500 300 Tm (Rotated by Tm) Tj ET\n"
        b"BT /G1 gs 72 300 Td (Font from gs) Tj ET\n"
        b"/Span << /ActualText (fi) >> BDC BT /F1 12 Tf 72 260 Td (fi) Tj ET EMC\n"
        b"/P /P1 BDC BT /F1 12 Tf 72 240 Td (marked) Tj ET EMC\n"
        b"q 20 0 0 20 300 240 cm BI /W 2 /H 2 /BPC 8 /CS /G ID \x00\xff\xff\x00 EI Q\n"
        b"BX 1 2 unknownop EX\n"
        b"BT /F3 20 Tf 72 200 Td (abab) Tj ET\n"
        b"BT /F4 12 Tf 550 700 Td <0001000200030004> Tj ET\n"
        b"q 1 0 0 1 72 100 cm /X1 Do Q"
    )
    form = b"BT /F1 9 Tf 0 0 Td (In a form) Tj ET q 0.5 0 0 0.5 0 -30 cm /X2 Do Q"
    nested = b"BT /F1 9 Tf (Nested form) Tj ET"
    glyph_a = b"60 0 d0 0 0 60 60 re f"
    glyph_b = b"80 0 d0 0 0 80 80 re f"

    def stream(data: bytes, entries: bytes = b"") -> bytes:
        return b"<< %s /Length %d >>\nstream\n%s\nendstream" % (entries, len(data), data)

    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] >>",
        3: b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << "
        b"/Font << /F1 10 0 R /F2 11 0 R /F3 12 0 R /F4 13 0 R >> "
        b"/XObject << /X1 20 0 R >> /Properties << /P1 << /MCID 0 >> >> "
        b"/ExtGState << /G1 << /Type /ExtGState /Font [11 0 R 16] >> >> >> >>",
        4: b"<< /Type /Page /Parent 2 0 R /Rotate 90 /Contents [6 0 R 7 0 R] "
        b"/Resources << /Font << /F1 10 0 R >> >> >>",
        5: stream(first),
        6: stream(b"BT /F1 12 Tf 72 720"),
        7: stream(b"Td (Split between streams) Tj ET"),
        10: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        11: b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Bold /Encoding /WinAnsiEncoding >>",
        12: b"<< /Type /Font /Subtype /Type3 /FontBBox [0 0 100 100] /FontMatrix [0.01 0 0 0.01 0 0] "
        b"/CharProcs << /a 30 0 R /b 31 0 R >> /Encoding << /Type /Encoding /Differences [97 /a /b] >> "
        b"/FirstChar 97 /LastChar 98 /Widths [60 80] /Resources << >> >>",
        13: b"<< /Type /Font /Subtype /Type0 /BaseFont /Vertical /Encoding /Identity-V "
        b"/DescendantFonts [14 0 R] >>",
        14: b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /Vertical "
        b"/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> "
        b"/DW 1000 /W2 [2 [-800 500 900]] /FontDescriptor 15 0 R >>",
        15: b"<< /Type /FontDescriptor /FontName /Vertical /Flags 4 /FontBBox [0 -120 1000 880] "
        b"/ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 700 /StemV 80 >>",
        20: stream(form, b"/Type /XObject /Subtype /Form /BBox [0 -100 300 100] /Matrix [1 0 0 1 10 10] "
                   b"/Resources << /Font << /F1 10 0 R >> /XObject << /X2 21 0 R >> >>"),
        21: stream(nested, b"/Type /XObject /Subtype /Form /BBox [0 0 300 100]"),
        30: stream(glyph_a),
        31: stream(glyph_b),
    }
    (HERE / "content-operators.pdf").write_bytes(write(objects, b"/Size 32 /Root 1 0 R"))


# Lines of right-to-left text as a real writer lays them out, with the
# Unicode Bidirectional Algorithm: Hebrew, Arabic, and both mixed with numbers
# and Latin words.
BIDI = [
    "Hello world",
    "עברית שלום",
    "العربية مرحبا",
    "שלום עולם 2026",
    "price: 100 שקל today",
]


def pango_bidi() -> None:
    """Writes fonts-pango-rtl.pdf with Pango, which shapes the text and puts
    right-to-left words in the order they are seen, as browsers and word
    processors do: its first letter is on the right. cairo's own text
    functions, as fonts-cairo-rtl.pdf uses them, put the letters from left to
    right in reading order instead, so the page shows them backwards."""
    subprocess.run(
        [
            "pango-view",
            "--no-display",
            "--font=DejaVu Sans 14",
            f"--text={chr(10).join(BIDI)}",
            f"--output={HERE / 'fonts-pango-rtl.pdf'}",
        ],
        check=True,
    )


def fax(image, compression: str, options: int | None = None) -> bytes:
    """The fax data of a 1-bit Pillow image, encoded by libtiff: the strip of
    a TIFF file with one strip. libtiff encodes the 1 bits of Pillow's images,
    its white, as black runs, so the image is inverted first."""
    from PIL import Image, ImageOps

    inverted = ImageOps.invert(image.convert("L")).convert("1")
    out = io.BytesIO()
    info = {278: image.height}
    if options is not None:
        info[292] = options
    inverted.save(out, "TIFF", compression=compression, tiffinfo=info)
    tiff = Image.open(io.BytesIO(out.getvalue()))
    (offset,), (count,) = tiff.tag_v2[273], tiff.tag_v2[279]
    return out.getvalue()[offset : offset + count]


def images() -> None:
    """Writes images.pdf: images of each kind the engine gives as files.
    Pairs of fax images, Group 4, Group 3 one- and two-dimensional and Group
    3 with byte-aligned lines, each followed by the same pixels with Flate;
    a palette, CMYK, 4-bit gray, 16-bit RGB, a stencil mask and a JPEG; and RGB
    images with masks: a soft mask, a soft mask of another size, a stencil mask
    and color key ranges."""
    from PIL import Image, ImageDraw

    def picture(width: int, height: int, seed: int):
        image = Image.new("1", (width, height), 1)
        draw = ImageDraw.Draw(image)
        draw.text((10, 5), f"ozaru fax {seed}", fill=0)
        draw.rectangle([5, height - 20, width - 6, height - 6], outline=0)
        draw.line([(0, 0), (width - 1, height - 1)], fill=0)
        for x in range(0, width, 7 + seed):
            draw.point((x, height // 2), fill=0)
        draw.rectangle([width - 40, 10, width - 10, 30], fill=0)
        return image

    def image(entries: bytes, data: bytes) -> bytes:
        return b"<< /Type /XObject /Subtype /Image %s /Length %d >>\nstream\n%s\nendstream" % (
            entries,
            len(data),
            data,
        )

    kinds: list[bytes] = []
    for seed, (width, height) in enumerate([(200, 60), (173, 41), (1800, 30)], 1):
        bitmap = picture(width, height, seed)
        size = b"/Width %d /Height %d /ColorSpace /DeviceGray /BitsPerComponent 1" % (width, height)
        flate = image(size + b" /Filter /FlateDecode", zlib.compress(bitmap.tobytes()))
        for k, compression, options, align in [
            (-1, "group4", None, False),
            (0, "group3", 0, False),
            (1, "group3", 1, False),
            (0, "group3", 4, True),
        ]:
            parameters = b"<< /K %d /Columns %d /Rows %d%s >>" % (
                k,
                width,
                height,
                b" /EncodedByteAlign true" if align else b"",
            )
            data = fax(bitmap, compression, options)
            kinds.append(image(size + b" /Filter /CCITTFaxDecode /DecodeParms " + parameters, data))
            kinds.append(flate)

    gradient = Image.new("RGB", (64, 48))
    for x in range(64):
        for y in range(48):
            gradient.putpixel((x, y), (x * 4, y * 5, 255 - x * 2))
    palette = gradient.quantize(colors=16)
    lookup = bytes(palette.getpalette()[: 16 * 3])
    kinds.append(
        image(
            b"/Width 64 /Height 48 /ColorSpace [/Indexed /DeviceRGB 15 <%s>] /BitsPerComponent 8 "
            b"/Filter /FlateDecode" % lookup.hex().encode(),
            zlib.compress(palette.tobytes()),
        )
    )
    cmyk = gradient.convert("CMYK")
    kinds.append(
        image(
            b"/Width 64 /Height 48 /ColorSpace /DeviceCMYK /BitsPerComponent 8 /Filter /FlateDecode",
            zlib.compress(cmyk.tobytes()),
        )
    )
    gray = gradient.convert("L")
    nibbles = bytes(
        (gray.getpixel((x, y)) >> 4) << 4 | (gray.getpixel((x + 1, y)) >> 4)
        for y in range(48)
        for x in range(0, 64, 2)
    )
    kinds.append(
        image(
            b"/Width 64 /Height 48 /ColorSpace /DeviceGray /BitsPerComponent 4 /Filter /FlateDecode",
            zlib.compress(nibbles),
        )
    )
    wide = bytes(b for pixel in gradient.tobytes() for b in (pixel, pixel ^ 0x5A))
    kinds.append(
        image(
            b"/Width 64 /Height 48 /ColorSpace /DeviceRGB /BitsPerComponent 16 /Filter /FlateDecode",
            zlib.compress(wide),
        )
    )
    mask = picture(96, 32, 4)
    kinds.append(
        image(b"/Width 96 /Height 32 /ImageMask true /Filter /FlateDecode", zlib.compress(mask.tobytes()))
    )
    jpeg = io.BytesIO()
    gradient.save(jpeg, "JPEG", quality=90)
    kinds.append(
        image(
            b"/Width 64 /Height 48 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
            jpeg.getvalue(),
        )
    )

    # The masked images come last, their masks after every image.
    masked_count = 4
    masks_at = 5 + len(kinds) + masked_count
    rgb = image(
        b"/Width 64 /Height 48 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
        zlib.compress(gradient.tobytes()),
    )
    alpha = bytes(min(255, x * 4 + y) for y in range(48) for x in range(64))
    small_alpha = bytes((x * 32 + y * 16) % 256 for y in range(12) for x in range(16))
    stencil = picture(64, 48, 5)
    def with_mask(entry: bytes) -> bytes:
        return rgb.replace(b"/Filter /FlateDecode", entry + b" /Filter /FlateDecode", 1)
    kinds.append(with_mask(b"/SMask %d 0 R" % masks_at))
    kinds.append(with_mask(b"/SMask %d 0 R" % (masks_at + 1)))
    kinds.append(with_mask(b"/Mask %d 0 R" % (masks_at + 2)))
    kinds.append(with_mask(b"/Mask [0 60 0 255 0 255]"))
    masks = [
        image(b"/Width 64 /Height 48 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode", zlib.compress(alpha)),
        image(b"/Width 16 /Height 12 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode", zlib.compress(small_alpha)),
        image(b"/Width 64 /Height 48 /ImageMask true /Filter /FlateDecode", zlib.compress(stencil.tobytes())),
    ]

    first = 5
    names = b" ".join(b"/Im%d %d 0 R" % (i, first + i) for i in range(len(kinds)))
    paint = b"".join(
        b"q 100 0 0 40 %d %d cm /Im%d Do Q\n" % (20 + 115 * (i % 5), 740 - 50 * (i // 5), i)
        for i in range(len(kinds))
    )
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << %s >> >> /Contents 4 0 R >>"
        % names,
        4: b"<< /Length %d >>\nstream\n%s\nendstream" % (len(paint), paint),
    }
    for i, kind in enumerate(kinds + masks):
        objects[first + i] = kind
    assert first + len(kinds) == masks_at
    size = first + len(kinds) + len(masks)
    (HERE / "images.pdf").write_bytes(write(objects, b"/Size %d /Root 1 0 R" % size))


def separations() -> None:
    """Writes images-separation.pdf: images in spot inks, /Separation and
    /DeviceN, whose colors come from functions of each type: exponential to
    RGB, a table of samples to gray, a calculator program from two inks to
    RGB, exponential to CMYK, stitching to RGB, and a palette of a spot ink."""

    def stream(entries: bytes, data: bytes) -> bytes:
        return b"<< %s /Length %d >>\nstream\n%s\nendstream" % (entries, len(data), data)

    def image(space: bytes, components: int, data: bytes) -> bytes:
        return stream(
            b"/Type /XObject /Subtype /Image /Width 64 /Height 32 /ColorSpace %s "
            b"/BitsPerComponent 8 /Filter /FlateDecode" % space,
            zlib.compress(data),
        )

    ramp = bytes(min(255, x * 4 + y * 2) for y in range(32) for x in range(64))
    two = bytes(v for y in range(32) for x in range(64) for v in (x * 4, y * 8))
    table = bytes([0, 10, 30, 60, 100, 140, 170, 195, 215, 230, 240, 247, 251, 253, 254, 255])
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        5: image(b"[/Separation /Gold /DeviceRGB << /FunctionType 2 /Domain [0 1] "
                 b"/C0 [1 1 1] /C1 [0.8 0.6 0.1] /N 1 >>]", 1, ramp),
        6: image(b"[/Separation /Spot /DeviceGray 11 0 R]", 1, ramp),
        7: image(b"[/DeviceN [/Cyan /Magenta] /DeviceRGB 12 0 R]", 2, two),
        8: image(b"[/Separation /Pantone#20185 /DeviceCMYK << /FunctionType 2 /Domain [0 1] "
                 b"/C0 [0 0 0 0] /C1 [0 0.9 0.8 0] /N 1.5 >>]", 1, ramp),
        9: image(b"[/Separation /Teal /DeviceRGB << /FunctionType 3 /Domain [0 1] /Bounds [0.5] "
                 b"/Encode [0 1 0 1] /Functions [<< /FunctionType 2 /Domain [0 1] /C0 [1 1 1] "
                 b"/C1 [0.5 0.8 0.8] /N 1 >> << /FunctionType 2 /Domain [0 1] /C0 [0.5 0.8 0.8] "
                 b"/C1 [0 0.3 0.3] /N 1 >>] >>]", 1, ramp),
        10: image(b"[/Indexed [/Separation /Spot /DeviceGray 11 0 R] 3 <00557FFF>]", 1,
                  bytes((x // 16) for y in range(32) for x in range(64))),
        11: stream(b"/FunctionType 0 /Domain [0 1] /Range [0 1] /Size [16] /BitsPerSample 8 "
                   b"/Decode [1 0]", table),
        12: stream(b"/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]",
                   b"{ 1 exch sub exch 1 exch sub exch 1 }"),
    }
    names = b" ".join(b"/Im%d %d 0 R" % (i, 5 + i) for i in range(6))
    paint = b"".join(
        b"q 160 0 0 80 %d %d cm /Im%d Do Q\n" % (40 + 180 * (i % 3), 600 - 120 * (i // 3), i)
        for i in range(6)
    )
    objects[3] = (
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /XObject << %s >> >> /Contents 4 0 R >>" % names
    )
    objects[4] = b"<< /Length %d >>\nstream\n%s\nendstream" % (len(paint), paint)
    (HERE / "images-separation.pdf").write_bytes(write(objects, b"/Size 13 /Root 1 0 R"))


def jbig2_images() -> None:
    """Writes images-jbig2.pdf: JBIG2 images written by jbig2.py, the
    corpus's own encoder: a generic region of each template, with typical
    prediction and moved adaptive pixels; one coded with MMR; symbols placed
    by a text region, the symbol dictionary in /JBIG2Globals, with the
    arithmetic coder and with Huffman tables, whose symbols come as bitmaps
    uncompressed and coded with MMR; a halftone of patterns of dots on a grid
    turned a little; and two regions combined with XOR."""
    from PIL import Image, ImageDraw, ImageFont

    import jbig2

    def picture(width: int, height: int, seed: int, blank_rows: bool = False):
        image = Image.new("1", (width, height), 0)
        draw = ImageDraw.Draw(image)
        draw.text((6, 4), f"JBIG2 {seed}", fill=1)
        draw.ellipse([width - 50, 6, width - 10, height - 8], outline=1)
        draw.line([(0, height - 1), (width - 1, 0)], fill=1)
        if blank_rows:
            draw.rectangle([0, height // 2 - 6, width - 1, height // 2 + 6], fill=0)
        return [[image.getpixel((x, y)) for x in range(width)] for y in range(height)]

    def bits(bitmap) -> "Image.Image":
        image = Image.new("1", (len(bitmap[0]), len(bitmap)), 1)
        for y, row in enumerate(bitmap):
            for x, pixel in enumerate(row):
                if pixel:
                    image.putpixel((x, y), 0)
        return image

    def page(width, height, *regions) -> bytes:
        data = jbig2.segment(1, 48, jbig2.page_info(width, height))
        for number, region in enumerate(regions, start=2):
            data += jbig2.segment(number, 38, region)
        return data + jbig2.segment(len(regions) + 2, 49, b"")

    images = []
    for template, typical, adaptive in [
        (0, False, None),
        (1, True, None),
        (2, False, [(-3, -1)]),
        (3, True, [(3, -1)]),
    ]:
        bitmap = picture(160, 48, template + 1, blank_rows=typical)
        region = jbig2.generic_region(bitmap, template=template, typical=typical, adaptive=adaptive)
        images.append((160, 48, page(160, 48, region), None))

    fax_bitmap = picture(144, 40, 5)
    mmr = fax(bits(fax_bitmap), "group4")
    images.append((144, 40, page(144, 40, jbig2.mmr_region(144, 40, mmr)), None))

    # Letters as symbols, placed twice each on two lines.
    font = ImageFont.load_default()
    letters = "ozaru JBIG"
    symbols, widths = [], []
    for letter in letters:
        box = font.getbbox(letter)
        width, height = max(1, box[2]), max(1, box[3])
        image = Image.new("1", (width, height), 0)
        ImageDraw.Draw(image).text((0, 0), letter, font=font, fill=1)
        symbols.append([[image.getpixel((x, y)) for x in range(width)] for y in range(height)])
    dictionary, numbering = jbig2.symbol_dictionary(symbols)
    instances = []
    for line, y in enumerate([4, 24]):
        x = 4
        for i, symbol in enumerate(symbols):
            if letters[i] != " ":
                instances.append((i, x, y + line))
            x += len(symbol[0]) + 1
    text = jbig2.text_region(120, 44, symbols, instances, numbering)
    globals_data = jbig2.segment(1, 0, dictionary)
    text_page = (
        jbig2.segment(2, 48, jbig2.page_info(120, 44))
        + jbig2.segment(3, 6, text, referred=[1])
        + jbig2.segment(4, 49, b"")
    )
    images.append((120, 44, text_page, globals_data))

    # The same letters, coded with Huffman tables, the dictionary in the
    # globals for one image and in the image for the other.
    huffman_dictionary, huffman_numbering = jbig2.symbol_dictionary_huffman(
        symbols, lambda bitmap: fax(bits(bitmap), "group4")
    )
    huffman_text = jbig2.text_region_huffman(120, 44, symbols, instances, huffman_numbering)
    images.append((
        120,
        44,
        jbig2.segment(2, 48, jbig2.page_info(120, 44))
        + jbig2.segment(3, 6, huffman_text, referred=[1])
        + jbig2.segment(4, 49, b""),
        jbig2.segment(1, 0, huffman_dictionary),
    ))
    images.append((
        120,
        44,
        jbig2.segment(1, 0, huffman_dictionary)
        + jbig2.segment(2, 48, jbig2.page_info(120, 44))
        + jbig2.segment(3, 6, huffman_text, referred=[1])
        + jbig2.segment(4, 49, b""),
        None,
    ))

    # A halftone: 8 patterns of growing dots, and a gradient of gray values
    # on a grid turned a little.
    patterns = []
    for level in range(8):
        cell = [[0] * 4 for _ in range(4)]
        for k, (px, py) in enumerate([(1, 1), (2, 2), (2, 1), (1, 2), (0, 0), (3, 3), (0, 3), (3, 0)][:level]):
            cell[py][px] = 1
        patterns.append(cell)
    grays = [[(gx + gy) * 7 // 36 for gx in range(24)] for gy in range(14)]
    halftone = (
        jbig2.segment(1, 16, jbig2.pattern_dictionary(patterns))
        + jbig2.segment(2, 48, jbig2.page_info(100, 60))
        + jbig2.segment(3, 22, jbig2.halftone_region(100, 60, grays, 8, 2 << 8, 1 << 8, 4 * 256, 256 // 8), referred=[1])
        + jbig2.segment(4, 49, b"")
    )
    images.append((100, 60, halftone, None))

    first, second = picture(96, 40, 7), picture(64, 24, 8)
    combined = page(
        96,
        40,
        jbig2.generic_region(first),
        jbig2.generic_region(second, x=20, y=10, combination=2),
    )
    images.append((96, 40, combined, None))

    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    }
    names, paint = [], b""
    number = 5
    for i, (width, height, data, globals_data) in enumerate(images):
        parameters = b""
        if globals_data is not None:
            objects[number + 1] = b"<< /Length %d >>\nstream\n%s\nendstream" % (len(globals_data), globals_data)
            parameters = b" /DecodeParms << /JBIG2Globals %d 0 R >>" % (number + 1)
        objects[number] = (
            b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceGray "
            b"/BitsPerComponent 1 /Filter /JBIG2Decode%s /Length %d >>\nstream\n%s\nendstream"
            % (width, height, parameters, len(data), data)
        )
        names.append(b"/Im%d %d 0 R" % (i, number))
        paint += b"q %d 0 0 %d %d %d cm /Im%d Do Q\n" % (
            width, height, 40 + 180 * (i % 3), 650 - 100 * (i // 3), i
        )
        number += 2 if globals_data is not None else 1
    objects[3] = (
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /XObject << %s >> >> /Contents 4 0 R >>" % b" ".join(names)
    )
    objects[4] = b"<< /Length %d >>\nstream\n%s\nendstream" % (len(paint), paint)
    (HERE / "images-jbig2.pdf").write_bytes(write(objects, b"/Size %d /Root 1 0 R" % number))


def type_fonts() -> None:
    """Writes fonts-type1.pdf and fonts-cff.pdf, with Type 1 fonts embedded
    as Type 1 and as CFF (Ghostscript at compatibility level 1.1 and 1.4, from
    the URW fonts it has), and fonts-type3.pdf, a Type 3 font made by hand: a
    glyph with colors of its own (`d0`) and one without (`d1`)."""
    program = b"""%!PS
/NimbusSans-Regular findfont 20 scalefont setfont
50 700 moveto (Type 1 and CFF: Hello AVATAR fi) show
/NimbusRoman-Italic findfont 18 scalefont setfont
50 660 moveto (Nimbus Roman Italic - quick brown fox) show
/NimbusMonoPS-Regular findfont 14 scalefont setfont
50 620 moveto (mono: 0123456789 {}[]) show
/StandardSymbolsPS findfont 18 scalefont setfont
50 580 moveto (abgd) show
showpage
"""
    with tempfile.TemporaryDirectory() as folder:
        source = Path(folder) / "types.ps"
        source.write_bytes(program)
        for level, name in [("1.1", "fonts-type1.pdf"), ("1.4", "fonts-cff.pdf")]:
            subprocess.run(
                [
                    "gs", "-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=pdfwrite",
                    f"-dCompatibilityLevel={level}", "-dEmbedAllFonts=true",
                    f"-sOutputFile={HERE / name}", str(source),
                ],
                check=True,
            )
    boxes = b"1 0 0 rg 0 0 700 700 re f"
    triangle = b"0 0 m 700 0 l 350 700 l h f"
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: (
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R "
            b"/Resources << /Font << /T3 5 0 R >> >> >>"
        ),
        4: None,
        5: (
            b"<< /Type /Font /Subtype /Type3 /FontBBox [0 0 700 700] /FontMatrix [0.001 0 0 0.001 0 0] "
            b"/CharProcs << /a 6 0 R /b 7 0 R >> /Encoding << /Type /Encoding /Differences [97 /a /b] >> "
            b"/FirstChar 97 /LastChar 98 /Widths [800 800] >>"
        ),
        6: b"<< /Length %d >>\nstream\n800 0 d0 %s\nendstream" % (len(b"800 0 d0 ") + len(boxes), boxes),
        7: b"<< /Length %d >>\nstream\n800 0 0 0 700 700 d1 %s\nendstream" % (len(b"800 0 0 0 700 700 d1 ") + len(triangle), triangle),
    }
    content = b"BT /T3 40 Tf 20 120 Td 0 0.4 0.8 rg (abab) Tj 0 -60 Td 0.8 0.2 0 rg (baba) Tj ET"
    objects[4] = b"<< /Length %d >>\nstream\n%s\nendstream" % (len(content), content)
    (HERE / "fonts-type3.pdf").write_bytes(write(objects, b"/Size 8 /Root 1 0 R"))


def fonts() -> None:
    ttf = reportlab_truetype()
    cairo_text("fonts-cairo-latin.pdf", "DejaVu Sans", TEXT)
    cairo_text("fonts-cairo-cjk.pdf", "Droid Sans Fallback", TEXT + CJK)
    cairo_text("fonts-cairo-rtl.pdf", "DejaVu Sans", TEXT + RIGHT_TO_LEFT)
    ghostscript(ttf, "fonts-gs.pdf")
    subprocess.run(["pdftocairo", "-pdf", str(ttf), str(HERE / "fonts-poppler.pdf")], check=True)
    standard_14()
    without_to_unicode()
    pango_bidi()


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
    content_operators()
    images()
    separations()
    jbig2_images()
    type_fonts()


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
