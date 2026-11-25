"""Writing metadata, bookmarks, page labels, attachments, and cropping,
deleting and adding pages."""

import datetime
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three pages
NAVIGATION = FILES.parent / "crates/engine/tests/corpus/navigation.pdf"  # seven pages


def test_set_metadata(tmp_path):
    out = tmp_path / "out.pdf"
    when = datetime.datetime(2026, 11, 25, 10, 30, 5, tzinfo=datetime.timezone(datetime.timedelta(hours=-5)))
    ozaru.set_metadata(
        PDF,
        out,
        {"title": "Informe é 日本", "author": "Ana", "creation_date": when, "xmp": "<x/>"},
    )
    metadata = ozaru.metadata(out)
    assert metadata.title == "Informe é 日本"
    assert metadata.author == "Ana"
    assert metadata.creation_date == when
    assert metadata.xmp == "<x/>"
    # What isn't named stays; None removes.
    assert metadata.producer == ozaru.metadata(PDF).producer
    ozaru.set_metadata(out, tmp_path / "again.pdf", {"author": None, "xmp": None})
    again = ozaru.metadata(tmp_path / "again.pdf")
    assert again.author is None and again.xmp is None
    assert again.title == "Informe é 日本"


def test_set_metadata_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(ValueError, match='no metadata called "color"'):
        ozaru.set_metadata(PDF, out, {"color": "red"})
    with pytest.raises(TypeError, match="title"):
        ozaru.set_metadata(PDF, out, {"title": 3})
    with pytest.raises(TypeError, match="creation_date"):
        ozaru.set_metadata(PDF, out, {"creation_date": "yesterday"})
    assert not out.exists()


def test_set_bookmarks(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.set_bookmarks(
        PDF,
        out,
        [(1, "Uno", 1), (2, "Uno.uno ñ", 2), (1, "Web", "https://example.com"), (1, "Nada", None)],
    )
    found = ozaru.bookmarks(out)
    assert [(b.level, b.title, b.page, b.uri) for b in found] == [
        (1, "Uno", 1, None),
        (2, "Uno.uno ñ", 2, None),
        (1, "Web", None, "https://example.com"),
        (1, "Nada", None, None),
    ]
    # Bookmark objects can be given back, as read.
    ozaru.set_bookmarks(out, tmp_path / "copy.pdf", ozaru.bookmarks(out))
    assert ozaru.bookmarks(tmp_path / "copy.pdf") == ozaru.bookmarks(out)
    ozaru.set_bookmarks(out, tmp_path / "none.pdf", [])
    assert ozaru.bookmarks(tmp_path / "none.pdf") == []


def test_set_bookmarks_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(ValueError, match="start at 1"):
        ozaru.set_bookmarks(PDF, out, [(2, "A", 1)])
    with pytest.raises(IndexError, match="no such page"):
        ozaru.set_bookmarks(PDF, out, [(1, "A", 9)])
    with pytest.raises(ValueError, match="counted from 1"):
        ozaru.set_bookmarks(PDF, out, [(1, "A", 0)])
    with pytest.raises(TypeError, match="Bookmark or a"):
        ozaru.set_bookmarks(PDF, out, ["A"])


def test_set_page_labels(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.set_page_labels(
        NAVIGATION,
        out,
        [
            {"page": 1, "style": "roman_lower"},
            {"page": 3, "style": "decimal"},
            {"page": 6, "style": "letters", "prefix": "App-", "start": 2},
        ],
    )
    assert ozaru.page_labels(out) == ["i", "ii", "1", "2", "3", "App-B", "App-C"]
    ozaru.set_page_labels(out, tmp_path / "none.pdf", [])
    assert ozaru.page_labels(tmp_path / "none.pdf") == [str(n) for n in range(1, 8)]


def test_set_page_labels_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(ValueError, match="start at the first page"):
        ozaru.set_page_labels(PDF, out, [{"page": 2}])
    with pytest.raises(ValueError, match="Unknown style"):
        ozaru.set_page_labels(PDF, out, [{"page": 1, "style": "greek"}])
    with pytest.raises(ValueError, match="needs a page"):
        ozaru.set_page_labels(PDF, out, [{"style": "decimal"}])
    with pytest.raises(ValueError, match='no "colour"'):
        ozaru.set_page_labels(PDF, out, [{"page": 1, "colour": "red"}])
    with pytest.raises(TypeError, match="dict"):
        ozaru.set_page_labels(PDF, out, [(1, "D")])


def test_attachments(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.add_attachment(PDF, out, "notas", b"hola " * 500, filename="notas.txt", description="Una é")
    (found,) = ozaru.attachments(out)
    assert (found.name, found.filename, found.description) == ("notas", "notas.txt", "Una é")
    assert found.mime_type == "text/plain" and found.size == 2500
    assert found.data == b"hola " * 500
    # From a file, with the name guessed from it, and replacing one by name.
    source = tmp_path / "datos.csv"
    source.write_bytes(b"a,b\n1,2\n")
    ozaru.add_attachment(out, tmp_path / "two.pdf", "datos", source)
    ozaru.add_attachment(tmp_path / "two.pdf", tmp_path / "three.pdf", "notas", b"nuevo")
    by_name = {a.name: a for a in ozaru.attachments(tmp_path / "three.pdf")}
    assert by_name["datos"].filename == "datos.csv"
    assert by_name["datos"].mime_type == "text/csv"
    assert by_name["notas"].data == b"nuevo"
    ozaru.remove_attachment(tmp_path / "three.pdf", tmp_path / "four.pdf", "notas")
    assert [a.name for a in ozaru.attachments(tmp_path / "four.pdf")] == ["datos"]
    with pytest.raises(ValueError, match='no attachment called "zzz"'):
        ozaru.remove_attachment(out, tmp_path / "x.pdf", "zzz")
    with pytest.raises(ValueError, match="needs a name"):
        ozaru.add_attachment(PDF, tmp_path / "x.pdf", "", b"x")


def test_crop(tmp_path):
    out = tmp_path / "out.pdf"
    full = ozaru.page_info(PDF)
    ozaru.crop(PDF, out, (10, 20, 110, 120), pages=[2])
    info = ozaru.page_info(out)
    assert (info[1].width, info[1].height) == (100, 100)
    assert (info[0].width, info[0].height) == (full[0].width, full[0].height)
    # The box is measured from the top left, so a word inside it stays inside.
    words = ozaru.extract_words(PDF)
    ozaru.crop(PDF, out, (0, 0, 595, 100))
    assert all(info.height == 100 for info in ozaru.page_info(out))
    cropped = ozaru.extract_words(out)
    assert 0 < len(cropped) < len(words)
    assert all(word.y1 <= 100.01 for word in cropped)
    with pytest.raises(ValueError, match="positive width and height"):
        ozaru.crop(PDF, out, (10, 10, 10, 50))
    with pytest.raises(IndexError):
        ozaru.crop(PDF, out, (0, 0, 10, 10), pages=[9])


def test_delete_pages(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.delete_pages(PDF, out, [1, 3])
    assert ozaru.page_count(out) == 1
    assert ozaru.extract_text(out) == ozaru.extract_text(PDF).split("\x0c")[1]
    with pytest.raises(ValueError, match="at least one page"):
        ozaru.delete_pages(PDF, out, [1, 2, 3])
    with pytest.raises(IndexError):
        ozaru.delete_pages(PDF, out, [4])


def test_add_blank_page(tmp_path):
    out = tmp_path / "out.pdf"
    first = ozaru.page_info(PDF)[0]
    ozaru.add_blank_page(PDF, out)
    assert ozaru.page_count(out) == 4
    last = ozaru.page_info(out)[3]
    assert (last.width, last.height) == (first.width, first.height)
    assert ozaru.extract_text(out).split("\x0c")[3] == ""
    ozaru.add_blank_page(PDF, out, position=1, width=300, height=400)
    info = ozaru.page_info(out)
    assert (info[0].width, info[0].height) == (300, 400)
    assert ozaru.extract_text(out).split("\x0c")[1] == ozaru.extract_text(PDF).split("\x0c")[0]
    with pytest.raises(IndexError, match="from 1 to 4"):
        ozaru.add_blank_page(PDF, out, position=5)
    with pytest.raises(ValueError, match="positive"):
        ozaru.add_blank_page(PDF, out, width=0)


def test_document_methods(tmp_path):
    pdf = ozaru.open(PDF)
    pdf.set_metadata(tmp_path / "a.pdf", {"title": "T"})
    assert ozaru.metadata(tmp_path / "a.pdf").title == "T"
    pdf.set_bookmarks(tmp_path / "b.pdf", [(1, "A", 1)])
    assert [b.title for b in ozaru.bookmarks(tmp_path / "b.pdf")] == ["A"]
    pdf.set_page_labels(tmp_path / "c.pdf", [{"page": 1, "style": "roman"}])
    assert ozaru.page_labels(tmp_path / "c.pdf") == ["I", "II", "III"]
    pdf.add_attachment(tmp_path / "d.pdf", "x", b"1")
    assert ozaru.attachments(tmp_path / "d.pdf")[0].data == b"1"
    pdf.crop(tmp_path / "e.pdf", (0, 0, 50, 50))
    assert ozaru.page_info(tmp_path / "e.pdf")[0].width == 50
    pdf.delete_pages(tmp_path / "f.pdf", [1])
    assert ozaru.page_count(tmp_path / "f.pdf") == 2
    pdf.add_blank_page(tmp_path / "g.pdf")
    assert ozaru.page_count(tmp_path / "g.pdf") == 4
    with_attachment = ozaru.open(tmp_path / "d.pdf")
    with_attachment.remove_attachment(tmp_path / "h.pdf", "x")
    assert ozaru.attachments(tmp_path / "h.pdf") == []


def test_protected_input_and_incremental(tmp_path):
    protected = tmp_path / "p.pdf"
    ozaru.encrypt(PDF, protected, user_password="s")
    with pytest.raises(ozaru.PasswordError):
        ozaru.set_metadata(protected, tmp_path / "x.pdf", {"title": "T"})
    ozaru.set_metadata(protected, tmp_path / "x.pdf", {"title": "T"}, password="s")
    assert ozaru.metadata(tmp_path / "x.pdf").title == "T"
    ozaru.set_metadata(PDF, tmp_path / "inc.pdf", {"title": "Inc"}, incremental=True)
    assert (tmp_path / "inc.pdf").read_bytes().startswith(PDF.read_bytes())
    assert ozaru.metadata(tmp_path / "inc.pdf").title == "Inc"


def test_bytes_input(tmp_path):
    ozaru.set_metadata(PDF.read_bytes(), tmp_path / "x.pdf", {"title": "B"})
    assert ozaru.metadata(tmp_path / "x.pdf").title == "B"
