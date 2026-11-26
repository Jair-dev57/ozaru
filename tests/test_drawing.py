"""Drawing on pages, watermarks and text widths."""

import struct
import zlib
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages
ONE_PAGE = FILES.parent / "crates/engine/tests/corpus/one-page.pdf"


def pixels(png: bytes):
    """A function giving the (r, g, b) of a pixel of a PNG file."""
    chunks, at = {}, 8
    while at < len(png):
        length = struct.unpack(">I", png[at : at + 4])[0]
        chunks.setdefault(png[at + 4 : at + 8], b"")
        chunks[png[at + 4 : at + 8]] += png[at + 8 : at + 8 + length]
        at += 12 + length
    width, height, depth, kind, _, _, interlace = struct.unpack(">IIBBBBB", chunks[b"IHDR"])
    assert depth == 8 and interlace == 0 and kind in (2, 6)
    step = 3 if kind == 2 else 4
    raw = zlib.decompress(chunks[b"IDAT"])
    rows, previous = [], bytes(width * step)
    for y in range(height):
        base = y * (1 + width * step)
        filtered, line = raw[base + 1 : base + 1 + width * step], bytearray(width * step)
        for i, value in enumerate(filtered):
            left = line[i - step] if i >= step else 0
            up = previous[i]
            corner = previous[i - step] if i >= step else 0
            match raw[base]:
                case 0:
                    guess = 0
                case 1:
                    guess = left
                case 2:
                    guess = up
                case 3:
                    guess = (left + up) // 2
                case _:
                    p = left + up - corner
                    pa, pb, pc = abs(p - left), abs(p - up), abs(p - corner)
                    guess = left if pa <= pb and pa <= pc else up if pb <= pc else corner
            line[i] = (value + guess) & 255
        rows.append(bytes(line))
        previous = bytes(line)

    def at_pixel(x, y):
        r, g, b, *alpha = rows[y][x * step : x * step + step]
        if alpha and alpha[0] != 255:
            a = alpha[0]
            return tuple((c * a + 255 * (255 - a)) // 255 for c in (r, g, b))
        return (r, g, b)

    return at_pixel


def render(path, page=1):
    return pixels(ozaru.render_page(path, page))


def test_shapes_are_drawn_where_they_are_asked(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.draw(
        PDF,
        out,
        [
            ozaru.Rect(1, 100, 100, 200, 200, fill="red", stroke=None),
            ozaru.Ellipse(1, 300, 100, 400, 200, fill=(0, 0, 255), stroke=None),
            ozaru.Line(1, 100, 300, 300, 300, color="#00a000", width=8),
            ozaru.Polygon(1, [(100, 400), (200, 400), (150, 480)], fill="orange", stroke=None),
        ],
    )
    at = render(out)
    assert at(150, 150) == (255, 0, 0)
    assert at(350, 150) == (0, 0, 255)
    assert at(305, 105) == (255, 255, 255)  # the corner of the ellipse's box
    assert at(200, 300) == (0, 160, 0)
    assert at(150, 430) == (255, 165, 0)
    assert at(150, 380) == (255, 255, 255)
    # The other pages are as they were.
    assert render(out, 2)(150, 150) == render(PDF, 2)(150, 150)


def test_positions_are_measured_from_the_top_left_like_word_boxes(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.draw(PDF, out, [ozaru.Text(1, 100, 200, "Marker", size=20)])
    word = next(w for w in ozaru.extract_words(out) if w.text == "Marker")
    # The baseline is at y = 200; the box sits on it.
    assert abs(word.x0 - 100) < 0.01
    assert 185 < word.y0 < 200 < word.y1 < 210


def test_text_keeps_the_page_and_adds_to_it(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.draw(PDF, out, [ozaru.Text(2, 72, 72, "Añadido ñ €", font="Times-Bold", size=14, color="blue")])
    before = ozaru.extract_text(PDF).split("\x0c")
    after = ozaru.extract_text(out).split("\x0c")
    assert after[0] == before[0] and after[2] == before[2]
    assert after[1].startswith(before[1].split("\n")[0]) or "Añadido ñ €" in after[1]
    assert "Añadido ñ €" in after[1]
    span = next(s for s in ozaru.extract_spans(out) if "Añadido" in s.text)
    assert (span.font, span.size, span.color, span.bold) == ("Times-Bold", 14.0, (0, 0, 255), True)


def test_alignment_and_rotation(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.draw(
        ONE_PAGE,
        out,
        [
            ozaru.Text(1, 300, 100, "centered", size=20, align="center"),
            ozaru.Text(1, 300, 140, "right", size=20, align="right"),
            ozaru.Text(1, 300, 300, "turned", size=20, rotation=90),
        ],
    )
    words = {w.text: w for w in ozaru.extract_words(out)}
    width = ozaru.text_width("centered", size=20)
    assert abs((words["centered"].x0 + words["centered"].x1) / 2 - 300) < 1
    assert abs(words["centered"].x1 - words["centered"].x0 - width) < 1
    assert abs(words["right"].x1 - 300) < 1
    # Turned 90 degrees, its box is taller than wide.
    box = words["turned"]
    assert box.y1 - box.y0 > box.x1 - box.x0


def test_opacity_and_dashes(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.draw(
        PDF,
        out,
        [
            ozaru.Rect(1, 100, 100, 200, 200, fill="black", stroke=None, opacity=0.5),
            ozaru.Line(1, 100, 300, 300, 300, width=6, dash=(20, 20)),
        ],
    )
    at = render(out)
    assert all(120 <= c <= 135 for c in at(150, 150))
    assert at(110, 300) == (0, 0, 0)
    assert at(130, 300) == (255, 255, 255)


def test_watermark(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.add_watermark(PDF, out, "DRAFT", color="red", opacity=0.5)
    for page in (1, 2, 3):
        texts = [s for s in ozaru.extract_spans(out) if s.text == "DRAFT" and s.page == page]
        assert len(texts) == 1
        assert texts[0].color == (255, 0, 0) and texts[0].bold
    # In the middle of the page, turned 45 degrees: the page's middle is marked.
    info = ozaru.page_info(PDF)[0]
    middle = render(out)(int(info.width / 2), int(info.height / 2))
    assert middle != (255, 255, 255)
    only = tmp_path / "only.pdf"
    ozaru.add_watermark(PDF, only, "CONFIDENTIAL", pages=[2], size=30, rotation=0)
    assert [s.page for s in ozaru.extract_spans(only) if s.text == "CONFIDENTIAL"] == [2]
    word = next(w for w in ozaru.extract_words(only) if w.text == "CONFIDENTIAL")
    assert abs(word.x1 - word.x0 - ozaru.text_width("CONFIDENTIAL", font="Helvetica-Bold", size=30)) < 1
    assert abs((word.x0 + word.x1) / 2 - info.width / 2) < 1


def test_text_width():
    assert ozaru.text_width("Hello") == pytest.approx(27.336)
    assert ozaru.text_width("abc", font="Courier", size=10) == pytest.approx(18.0)
    assert ozaru.text_width("") == 0.0
    assert ozaru.text_width("x", font="times") == ozaru.text_width("x", font="Times-Roman")
    with pytest.raises(ValueError, match="can't draw the character '日'"):
        ozaru.text_width("日本")
    with pytest.raises(ValueError, match="Unknown font"):
        ozaru.text_width("x", font="Comic Sans")


def test_document_methods(tmp_path):
    pdf = ozaru.open(PDF)
    pdf.draw(tmp_path / "a.pdf", [ozaru.Text(1, 72, 72, "viaDocument")])
    assert "viaDocument" in ozaru.extract_text(tmp_path / "a.pdf")
    pdf.add_watermark(tmp_path / "b.pdf", "MARK")
    assert len([s for s in ozaru.extract_spans(tmp_path / "b.pdf") if s.text == "MARK"]) == 3


def test_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(IndexError, match="no page 9"):
        ozaru.draw(PDF, out, [ozaru.Text(9, 0, 0, "x")])
    with pytest.raises(ValueError, match="can't draw the character"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "日本語")])
    with pytest.raises(ValueError, match="Unknown font"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "x", font="Nope")])
    with pytest.raises(ValueError, match="not a color"):
        ozaru.draw(PDF, out, [ozaru.Rect(1, 0, 0, 5, 5, fill="blurple")])
    with pytest.raises(ValueError, match="from 0 to 255"):
        ozaru.draw(PDF, out, [ozaru.Rect(1, 0, 0, 5, 5, fill=(300, 0, 0))])
    with pytest.raises(ValueError, match="opacity"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "x", opacity=2)])
    with pytest.raises(ValueError, match="positive"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "x", size=0)])
    with pytest.raises(ValueError, match="fill, a stroke"):
        ozaru.draw(PDF, out, [ozaru.Rect(1, 0, 0, 5, 5, fill=None, stroke=None)])
    with pytest.raises(ValueError, match="at least two points"):
        ozaru.draw(PDF, out, [ozaru.Polygon(1, [(0, 0)])])
    with pytest.raises(ValueError, match="align"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "x", align="middle")])
    with pytest.raises(TypeError, match="Draw Text"):
        ozaru.draw(PDF, out, ["x"])
    with pytest.raises(ValueError, match="size must be"):
        ozaru.add_watermark(PDF, out, "x", size=-1)
    with pytest.raises(ValueError, match="can't draw"):
        ozaru.add_watermark(PDF, out, "日本")
    assert not out.exists()


def test_color_spellings(tmp_path):
    out = tmp_path / "out.pdf"
    for color in ("red", "RED", "#f00", "#ff0000", (255, 0, 0), [255.0, 0, 0]):
        ozaru.draw(PDF, out, [ozaru.Rect(1, 100, 100, 200, 200, fill=color, stroke=None)])
        assert render(out)(150, 150) == (255, 0, 0)


def test_protected_input_and_bytes(tmp_path):
    protected = tmp_path / "p.pdf"
    ozaru.encrypt(PDF, protected, user_password="s")
    with pytest.raises(ozaru.PasswordError):
        ozaru.add_watermark(protected, tmp_path / "x.pdf", "M")
    ozaru.add_watermark(protected, tmp_path / "x.pdf", "M", password="s")
    ozaru.draw(PDF.read_bytes(), tmp_path / "y.pdf", [ozaru.Text(1, 72, 72, "fromBytes")])
    assert "fromBytes" in ozaru.extract_text(tmp_path / "y.pdf")
    ozaru.draw(PDF, tmp_path / "inc.pdf", [ozaru.Text(1, 72, 72, "inc")], incremental=True)
    assert (tmp_path / "inc.pdf").read_bytes().startswith(PDF.read_bytes())


def test_drawing_on_a_rotated_page_is_in_default_space(tmp_path):
    rotated = tmp_path / "r.pdf"
    ozaru.rotate(PDF, rotated, 90, pages=[1])
    out = tmp_path / "out.pdf"
    ozaru.draw(rotated, out, [ozaru.Text(1, 100, 100, "Turned")])
    word = next(w for w in ozaru.extract_words(out) if w.text == "Turned")
    assert abs(word.x0 - 100) < 0.01


# Liberation Sans, a subset the engine carries: Latin, with letters the standard
# fonts of a PDF reader don't have, such as Ł and Ő.
FONT = FILES.parent / "crates/engine/src/fonts/data/standard/OzaruStdSans-Regular.ttf"


def test_text_in_a_font_file(tmp_path):
    out = tmp_path / "out.pdf"
    text = "Łódź Őő Ğ ñ"
    with pytest.raises(ValueError, match="can't draw the character 'Ł'"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 72, 72, text)])
    ozaru.draw(PDF, out, [ozaru.Text(1, 72, 72, text, font=FONT, size=20, color="blue")])
    assert text in ozaru.extract_text(out)
    span = next(s for s in ozaru.extract_spans(out) if text in s.text)
    assert span.font.endswith("+OzaruStdSans-Regular")
    assert (span.size, span.color) == (20.0, (0, 0, 255))
    assert span.x0 == pytest.approx(72.0, abs=1)
    # Only the glyphs used are in the file, so it is far smaller than the font.
    assert out.stat().st_size < PDF.stat().st_size + FONT.stat().st_size // 2
    # The words are where the width says.
    word = next(w for w in ozaru.extract_words(out) if w.text == "Őő" and w.page == 1)
    assert word.x1 - word.x0 == pytest.approx(ozaru.text_width("Őő", font=FONT, size=20), abs=1)


def test_a_font_path_can_be_a_string_or_a_path(tmp_path):
    for font in (FONT, str(FONT)):
        ozaru.draw(PDF, tmp_path / "out.pdf", [ozaru.Text(1, 72, 72, "Łódź", font=font)])
        assert "Łódź" in ozaru.extract_text(tmp_path / "out.pdf")
    assert ozaru.text_width("Hello", font=FONT, size=12) == pytest.approx(ozaru.text_width("Hello"), abs=0.1)


def test_font_files_in_watermarks_and_the_page_is_drawn(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.add_watermark(PDF, out, "Łódź", font=FONT)
    assert len([s for s in ozaru.extract_spans(out) if s.text == "Łódź"]) == 3
    pixels_of = pixels(ozaru.render_page(out, 1))
    # The watermark is gray over the white page: some pixel is not white.
    assert any(pixels_of(x, y) != (255, 255, 255) for x in range(0, 200, 3) for y in range(0, 300, 3))


def test_two_drawings_with_different_letters_in_one_font(tmp_path):
    first, second = tmp_path / "a.pdf", tmp_path / "b.pdf"
    ozaru.draw(PDF, first, [ozaru.Text(1, 72, 72, "alpha", font=FONT)])
    ozaru.draw(first, second, [ozaru.Text(1, 72, 120, "zeta", font=FONT)])
    text = ozaru.extract_text(second)
    assert "alpha" in text and "zeta" in text


def test_font_file_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(ValueError, match="can't draw the character 'П'"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "Привет", font=FONT)])
    not_font = tmp_path / "x.ttf"
    not_font.write_bytes(b"this is not a font")
    with pytest.raises(ValueError, match="isn't a TrueType font"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "x", font=not_font)])
    with pytest.raises(ValueError, match="path of a TrueType font"):
        ozaru.draw(PDF, out, [ozaru.Text(1, 0, 0, "x", font=tmp_path / "missing.ttf")])
    assert not out.exists()
