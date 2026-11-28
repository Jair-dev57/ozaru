"""Comparing two PDFs."""

import json
import subprocess
import sys
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages


def test_a_pdf_is_identical_to_itself(tmp_path):
    result = ozaru.compare(PDF, PDF)
    assert result.identical and result.changed_pages == []
    assert [p.status for p in result.pages] == ["same"] * 3
    assert result.summary() == "The PDFs are identical."
    # Also from bytes, and a copy saved by another operation.
    copy = tmp_path / "copy.pdf"
    ozaru.rotate(PDF, copy, 0)
    assert ozaru.compare(PDF.read_bytes(), copy).identical


def test_words_that_were_taken_out_are_found_where_they_were(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.redact_text(PDF, out, "Muñoz")
    result = ozaru.compare(PDF, out)
    assert not result.identical
    assert [p.status for p in result.pages] == ["changed", "same", "same"]
    (change,) = result.pages[0].changes
    assert change.removed == "Muñoz" and change.added == ""
    # The box is where search found the word, from the top left.
    (match,) = ozaru.search(PDF, "Muñoz")
    assert change.removed_boxes == [pytest.approx(b, abs=0.01) for b in match.boxes][:1] or change.removed_boxes[0] == pytest.approx(match.bbox, abs=0.01)
    assert change.added_boxes == []
    assert "Muñoz" in result.summary() and "page 1" in result.summary()


def test_changed_words_added_and_removed(tmp_path):
    a, b = tmp_path / "a.pdf", tmp_path / "b.pdf"
    ozaru.draw(PDF, a, [ozaru.Text(1, 72, 400, "the price is 100 dollars", size=14)])
    ozaru.draw(PDF, b, [ozaru.Text(1, 72, 400, "the price is 250 dollars now", size=14)])
    result = ozaru.compare(a, b)
    changes = result.pages[0].changes
    assert [(c.removed, c.added) for c in changes] == [("100", "250"), ("", "now")]
    assert str(changes[0]) == "'100' -> '250'" and str(changes[1]) == "+ 'now'"


def test_pages_added_and_removed_are_found_without_moving_the_others(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.delete_pages(PDF, out, [2])
    result = ozaru.compare(PDF, out)
    assert [(p.page_a, p.page_b, p.status) for p in result.pages] == [
        (1, 1, "same"),
        (2, None, "removed"),
        (3, 2, "same"),
    ]
    again = ozaru.compare(out, PDF)
    assert [(p.page_a, p.page_b, p.status) for p in again.pages] == [
        (1, 1, "same"),
        (None, 2, "added"),
        (2, 3, "same"),
    ]
    assert "was removed" in result.summary() and "was added" in again.summary()


def test_information_bookmarks_attachments_and_fields(tmp_path):
    a, b = tmp_path / "a.pdf", tmp_path / "b.pdf"
    ozaru.add_fields(PDF, tmp_path / "f.pdf", [ozaru.TextField(1, "name", (100, 300, 300, 320), value="Ana")])
    ozaru.set_metadata(tmp_path / "f.pdf", tmp_path / "m.pdf", {"title": "Old"})
    ozaru.add_attachment(tmp_path / "m.pdf", tmp_path / "t.pdf", "old.txt", b"x")
    ozaru.set_bookmarks(tmp_path / "t.pdf", a, [(1, "One", 1), (1, "Two", 2)])
    ozaru.fill_form(a, tmp_path / "v.pdf", {"name": "Luis"})
    ozaru.set_metadata(tmp_path / "v.pdf", tmp_path / "w.pdf", {"title": "New", "author": "Ana"})
    ozaru.remove_attachment(tmp_path / "w.pdf", tmp_path / "x.pdf", "old.txt")
    ozaru.add_attachment(tmp_path / "x.pdf", tmp_path / "y.pdf", "new.txt", b"y")
    ozaru.set_bookmarks(tmp_path / "y.pdf", b, [(1, "One", 1), (1, "Three", 3)])
    result = ozaru.compare(a, b)
    assert [(i.key, i.old, i.new) for i in result.info] == [("Author", "Ozaru", "Ana"), ("Title", "Old", "New")]
    assert (result.bookmarks_removed, result.bookmarks_added) == (["Two"], ["Three"])
    assert (result.attachments_removed, result.attachments_added) == (["old.txt"], ["new.txt"])
    assert [(f.name, f.old, f.new) for f in result.fields] == [("name", "Ana", "Luis")]
    assert all(p.status == "same" for p in result.pages)
    text = result.summary()
    for fragment in ("Information Title", "Bookmark removed: 'Two'", "Attachment added: 'new.txt'", "Field name"):
        assert fragment in text


def test_pages_that_look_different_are_found_by_drawing_them(tmp_path):
    drawn = tmp_path / "drawn.pdf"
    ozaru.draw(PDF, drawn, [ozaru.Rect(2, 100, 100, 200, 160, fill="red", stroke=None)])
    # By words they are the same.
    assert ozaru.compare(PDF, drawn).identical
    result = ozaru.compare(PDF, drawn, visual=True)
    assert [p.status for p in result.pages] == ["same", "changed", "same"]
    page = result.pages[1]
    assert page.changes == [] and page.difference == pytest.approx(6000 / (595 * 842), rel=0.2)
    (x0, y0, x1, y1), = page.regions
    assert x0 <= 100 and y0 <= 100 and x1 >= 200 and y1 >= 160 and x1 - x0 < 130
    assert "looks different" in result.summary()
    # A weaker threshold sees less.
    assert ozaru.compare(PDF, drawn, visual=True, threshold=255).identical


def test_a_picture_of_the_difference(tmp_path):
    drawn = tmp_path / "drawn.pdf"
    ozaru.draw(PDF, drawn, [ozaru.Rect(1, 100, 300, 200, 360, fill="blue", stroke=None)])
    png = ozaru.diff_page(PDF, 1, drawn, 1)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    same = ozaru.diff_page(PDF, 1, PDF, 1)
    assert png != same
    with pytest.raises(IndexError):
        ozaru.diff_page(PDF, 9, drawn, 1)


def test_highlight_marks_both_pdfs(tmp_path):
    a, b = tmp_path / "a.pdf", tmp_path / "b.pdf"
    ozaru.draw(PDF, a, [ozaru.Text(1, 72, 400, "the price is 100 dollars", size=14)])
    ozaru.draw(PDF, b, [ozaru.Text(1, 72, 400, "the price is 250 dollars", size=14)])
    result = ozaru.compare(a, b)
    marked_a, marked_b = tmp_path / "ma.pdf", tmp_path / "mb.pdf"
    result.highlight(marked_a, marked_b)
    (mark_a,) = ozaru.annotations(marked_a)
    (mark_b,) = ozaru.annotations(marked_b)
    assert (mark_a.subtype, mark_a.contents) == ("Highlight", "Removed: 100")
    assert (mark_b.subtype, mark_b.contents) == ("Highlight", "Added: 250")
    assert mark_a.color == [pytest.approx(1.0), pytest.approx(0.612, abs=0.01), pytest.approx(0.612, abs=0.01)]
    # The words of the highlights are the changed ones.
    box_a = result.pages[0].changes[0].removed_boxes[0]
    assert mark_a.rect[0] <= box_a[0] + 0.5 and mark_a.rect[2] >= box_a[2] - 0.5
    with pytest.raises(ValueError, match="doesn't know"):
        ozaru.Comparison([], [], [], [], [], [], [], ).highlight(marked_a, marked_b)


def test_passwords_and_errors(tmp_path):
    locked = tmp_path / "locked.pdf"
    ozaru.encrypt(PDF, locked, user_password="pw")
    with pytest.raises(ozaru.PasswordError):
        ozaru.compare(PDF, locked)
    assert ozaru.compare(PDF, locked, password_b="pw").identical
    with pytest.raises(ValueError, match="threshold"):
        ozaru.compare(PDF, PDF, threshold=300)
    with pytest.raises(ValueError, match="scale"):
        ozaru.compare(PDF, PDF, scale=0)
    with pytest.raises(FileNotFoundError):
        ozaru.compare(PDF, tmp_path / "missing.pdf")


def test_command(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.redact_text(PDF, out, "Muñoz")

    def run(*args, check=True):
        return subprocess.run([sys.executable, "-m", "ozaru", *map(str, args)], capture_output=True, check=check)

    same = run("compare", PDF, PDF)
    assert same.returncode == 0 and b"identical" in same.stdout
    different = run("compare", PDF, out, check=False)
    assert different.returncode == 1 and "Muñoz" in different.stdout.decode()
    data = json.loads(run("compare", PDF, out, "--json", check=False).stdout)
    assert data["pages"][0]["changes"][0]["removed"] == "Muñoz"
    run("compare", PDF, out, "--highlight", tmp_path / "diff", check=False)
    assert (tmp_path / "diff-old.pdf").exists() and (tmp_path / "diff-new.pdf").exists()
    assert run("compare", PDF, check=False).returncode != 0
