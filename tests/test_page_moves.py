"""Inserting, replacing and moving pages."""

import subprocess
import sys
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages


@pytest.fixture
def pdfs(tmp_path):
    """`a.pdf` with pages A1..A4 and `b.pdf` with B1..B3, each with a bookmark for every page."""

    def make(name, label, pages):
        base = tmp_path / f"{name}-base.pdf"
        ozaru.merge([PDF] * ((pages + 2) // 3), base)
        trimmed = tmp_path / f"{name}-trim.pdf"
        ozaru.extract_pages(base, trimmed, list(range(1, pages + 1)))
        out = tmp_path / f"{name}.pdf"
        ozaru.draw(trimmed, out, [ozaru.Text(n, 72, 700, f"{label}{n}", size=30) for n in range(1, pages + 1)])
        marks = tmp_path / f"{name}-marks.pdf"
        ozaru.set_bookmarks(out, marks, [(1, f"{label}{n}", n) for n in range(1, pages + 1)])
        return marks

    return make("a", "A", 4), make("b", "B", 3)


def labels(path):
    return [text.split()[-1] if text.strip() else "" for text in _pages(path)]


def _pages(path):
    # The label of each page is the word of the form "A1", "B2"...
    pages = []
    for n in range(1, ozaru.page_count(path) + 1):
        words = [w.text for w in ozaru.extract_words(path) if w.page == n and len(w.text) == 2 and w.text[0] in "AB" and w.text[1].isdigit()]
        pages.append(words[0] if words else "?")
    return pages


def test_insert_pages_where_asked(tmp_path, pdfs):
    a, b = pdfs
    out = tmp_path / "out.pdf"
    ozaru.insert_pages(a, out, b, at=2, pages=[3, 1])
    assert _pages(out) == ["A1", "B3", "B1", "A2", "A3", "A4"]
    ozaru.insert_pages(a, out, b)
    assert _pages(out) == ["A1", "A2", "A3", "A4", "B1", "B2", "B3"]
    ozaru.insert_pages(a, out, b, at=1, pages=[2])
    assert _pages(out) == ["B2", "A1", "A2", "A3", "A4"]
    ozaru.insert_pages(a, out, b, at=5, pages=[2])  # one past the last: the end
    assert _pages(out)[-1] == "B2"


def test_bookmarks_follow_the_pages_that_stay(tmp_path, pdfs):
    a, b = pdfs
    out = tmp_path / "out.pdf"
    ozaru.insert_pages(a, out, b, at=2, pages=[1])
    marks = {m.title: m.page for m in ozaru.bookmarks(out)}
    assert marks == {"A1": 1, "A2": 3, "A3": 4, "A4": 5}


def test_replace_pages(tmp_path, pdfs):
    a, b = pdfs
    out = tmp_path / "out.pdf"
    ozaru.replace_pages(a, out, b, at=2, pages=[3])
    assert _pages(out) == ["A1", "B3", "A3", "A4"]
    ozaru.replace_pages(a, out, b, at=2, count=2, pages=[1])
    assert _pages(out) == ["A1", "B1", "A4"]
    ozaru.replace_pages(a, out, b, at=1, count=0, pages=[1])
    assert _pages(out) == ["B1", "A1", "A2", "A3", "A4"]
    ozaru.replace_pages(a, out, b, at=4, count=1)
    assert _pages(out) == ["A1", "A2", "A3", "B1", "B2", "B3"]
    # What went isn't in the file.
    ozaru.replace_pages(a, out, b, at=2, pages=[3])
    assert "A2" not in ozaru.extract_text(out)


def test_move_pages(tmp_path, pdfs):
    a, _ = pdfs
    out = tmp_path / "out.pdf"
    ozaru.move_pages(a, out, [4], to=1)
    assert _pages(out) == ["A4", "A1", "A2", "A3"]
    ozaru.move_pages(a, out, [1, 2])
    assert _pages(out) == ["A3", "A4", "A1", "A2"]
    ozaru.move_pages(a, out, [1], to=4)
    assert _pages(out) == ["A2", "A3", "A1", "A4"]
    # The bookmarks follow their pages.
    ozaru.move_pages(a, out, [4], to=1)
    assert {m.title: m.page for m in ozaru.bookmarks(out)} == {"A4": 1, "A1": 2, "A2": 3, "A3": 4}


def test_document_methods_and_sources(tmp_path, pdfs):
    a, b = pdfs
    doc = ozaru.open(a)
    doc.insert_pages(tmp_path / "i.pdf", b.read_bytes(), at=1, pages=[1])
    assert _pages(tmp_path / "i.pdf")[0] == "B1"
    doc.replace_pages(tmp_path / "r.pdf", b, at=1, pages=[2])
    assert _pages(tmp_path / "r.pdf")[0] == "B2"
    doc.move_pages(tmp_path / "m.pdf", [2], to=1)
    assert _pages(tmp_path / "m.pdf")[0] == "A2"


def test_protected_files(tmp_path, pdfs):
    a, b = pdfs
    locked, out = tmp_path / "locked.pdf", tmp_path / "out.pdf"
    ozaru.encrypt(b, locked, user_password="pw")
    with pytest.raises(ozaru.PasswordError):
        ozaru.insert_pages(a, out, locked)
    ozaru.insert_pages(a, out, locked, source_password="pw", pages=[1])
    assert _pages(out)[-1] == "B1"
    ozaru.encrypt(a, tmp_path / "locked-a.pdf", user_password="x")
    ozaru.insert_pages(tmp_path / "locked-a.pdf", out, locked, password="x", source_password="pw", pages=[2])
    assert "B2" in _pages(out)


def test_errors(tmp_path, pdfs):
    a, b = pdfs
    out = tmp_path / "out.pdf"
    with pytest.raises(IndexError, match="no place 7"):
        ozaru.insert_pages(a, out, b, at=7)
    with pytest.raises(IndexError, match="no place 0"):
        ozaru.insert_pages(a, out, b, at=0)
    with pytest.raises(IndexError, match="no page 9"):
        ozaru.insert_pages(a, out, b, pages=[9])
    with pytest.raises(IndexError, match="no page 9 to replace"):
        ozaru.replace_pages(a, out, b, at=9)
    with pytest.raises(IndexError, match="Can't replace 3 pages from page 3"):
        ozaru.replace_pages(a, out, b, at=3, count=3)
    with pytest.raises(ValueError, match="no pages to put"):
        ozaru.replace_pages(a, out, b, at=1, pages=[])
    with pytest.raises(IndexError, match="no page 9"):
        ozaru.move_pages(a, out, [9])
    with pytest.raises(IndexError, match="no place 9"):
        ozaru.move_pages(a, out, [1], to=9)
    with pytest.raises(FileNotFoundError):
        ozaru.insert_pages(a, out, tmp_path / "missing.pdf")
    assert not out.exists()


def test_commands(tmp_path, pdfs):
    a, b = pdfs

    def run(*args):
        return subprocess.run([sys.executable, "-m", "ozaru", *map(str, args)], capture_output=True, check=True)

    run("insert", a, b, "-o", tmp_path / "i.pdf", "--at", 2, "--source-pages", "1,3")
    assert _pages(tmp_path / "i.pdf") == ["A1", "B1", "B3", "A2", "A3", "A4"]
    run("replace", a, b, "-o", tmp_path / "r.pdf", "--at", 1, "--count", 2, "--source-pages", "2")
    assert _pages(tmp_path / "r.pdf") == ["B2", "A3", "A4"]
    run("move", a, "4", "-o", tmp_path / "m.pdf", "--to", 1)
    assert _pages(tmp_path / "m.pdf") == ["A4", "A1", "A2", "A3"]
