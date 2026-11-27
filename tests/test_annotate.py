"""Marks on text: highlight, underline, strike out and squiggly."""

import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages


def test_highlight_every_place_a_text_is_found(tmp_path):
    out = tmp_path / "out.pdf"
    found = ozaru.search(PDF, "café")
    assert found
    count = ozaru.highlight(PDF, out, "café", note="the coffee", author="Ana")
    assert count == len(found)
    marks = [a for a in ozaru.annotations(out) if a.subtype == "Highlight"]
    assert len(marks) == count
    assert marks[0].contents == "the coffee" and marks[0].author == "Ana"
    assert marks[0].color == (1.0, 1.0, 0.0) or tuple(marks[0].color) == (1.0, 1.0, 0.0)
    # The mark covers the box of the match, as the page's own coordinates.
    x0, y0, x1, y1 = found[0].bbox
    mx0, my0, mx1, my1 = marks[0].rect
    assert mx0 <= x0 + 0.5 and mx1 >= x1 - 0.5 and my0 <= y0 + 0.5 and my1 >= y1 - 0.5
    # The text of the page is as it was.
    assert ozaru.extract_text(out) == ozaru.extract_text(PDF)


def test_every_kind_and_annotate_with_boxes(tmp_path):
    out = tmp_path / "out.pdf"
    box = (50, 100, 150, 120)
    ozaru.annotate(
        PDF,
        out,
        [
            ozaru.Highlight(1, box),
            ozaru.Underline(1, [box, (50, 125, 150, 145)], color="blue"),
            ozaru.StrikeOut(2, box, color="#0a0", opacity=0.5, subject="gone"),
            ozaru.Squiggly(3, box, date=datetime(2026, 1, 2, 3, 4, 5, tzinfo=timezone.utc)),
        ],
    )
    found = ozaru.annotations(out)
    assert [(a.page, a.subtype) for a in found] == [
        (1, "Highlight"),
        (1, "Underline"),
        (2, "StrikeOut"),
        (3, "Squiggly"),
    ]
    assert len(ozaru.annotations(out)[1].quad_points) == 2
    assert found[2].subject == "gone"
    assert found[3].modified.year == 2026


def test_the_marks_are_drawn(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.highlight(PDF, out, "Factura", pages=[1])
    with_marks = ozaru.render_page(out, 1)
    without = ozaru.render_page(PDF, 1)
    assert with_marks != without
    assert ozaru.render_page(out, 1, annotations=False) == without


def test_nothing_found_writes_the_file_unchanged(tmp_path):
    out = tmp_path / "out.pdf"
    assert ozaru.highlight(PDF, out, "this text is not in the file") == 0
    assert ozaru.annotations(out) == ozaru.annotations(PDF)


def test_incremental_and_document_method(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.open(PDF).annotate(out, [ozaru.Highlight(1, (10, 10, 60, 30))], incremental=True)
    assert out.read_bytes().startswith(PDF.read_bytes())
    assert len(ozaru.annotations(out)) == 1


def test_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(IndexError, match="no page 9"):
        ozaru.annotate(PDF, out, [ozaru.Highlight(9, (0, 0, 5, 5))])
    with pytest.raises(ValueError, match="at least one box"):
        ozaru.annotate(PDF, out, [ozaru.Highlight(1, [])])
    with pytest.raises(ValueError, match="x0, y0, x1, y1"):
        ozaru.annotate(PDF, out, [ozaru.Highlight(1, [(1, 2, 3)])])
    with pytest.raises(ValueError, match="not a color"):
        ozaru.annotate(PDF, out, [ozaru.Highlight(1, (0, 0, 5, 5), color="blurple")])
    with pytest.raises(TypeError, match="Annotate with"):
        ozaru.annotate(PDF, out, ["x"])
    with pytest.raises(ValueError, match="kind must be"):
        ozaru.highlight(PDF, out, "x", kind="circle")
    assert not out.exists()


def test_command(tmp_path):
    out = tmp_path / "out.pdf"
    done = subprocess.run(
        [sys.executable, "-m", "ozaru", "highlight", str(PDF), "café", "-o", str(out), "--kind", "underline", "--color", "red"],
        capture_output=True,
        check=True,
    )
    assert b"marked" in done.stdout
    assert {a.subtype for a in ozaru.annotations(out)} == {"Underline"}


def test_notes_links_and_shapes(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.annotate(
        PDF,
        out,
        [
            ozaru.Note(1, 300, 40, "Check the total", author="Ana", open=True),
            ozaru.Link(1, (50, 100, 150, 120), uri="https://example.com"),
            ozaru.Link(1, (50, 130, 150, 150), to_page=2, border=1, color="blue"),
            ozaru.Box(1, (200, 100, 260, 140), fill="#ffeeaa", color="orange", width=2),
            ozaru.Oval(1, (270, 100, 330, 140), color="green"),
            ozaru.Arrow(1, (350, 200), (450, 150), color="purple", width=2),
            ozaru.Ink(1, [(60, 300), (80, 280), (100, 300), (120, 280)], color="blue"),
            ozaru.TextBox(1, (50, 400, 200, 460), "A text in a box that wraps over lines", size=11, fill="#ffffcc", border=1),
        ],
    )
    found = {a.subtype: a for a in reversed(ozaru.annotations(out))}
    assert set(found) == {"Text", "Link", "Square", "Circle", "Line", "Ink", "FreeText"}
    note = found["Text"]
    assert note.contents == "Check the total" and note.author == "Ana"
    assert note.rect[:2] == pytest.approx((300, 40)) and note.rect[3] - note.rect[1] == pytest.approx(20)
    links = [a for a in ozaru.annotations(out) if a.subtype == "Link"]
    assert links[0].uri == "https://example.com" and links[1].target_page == 2
    assert all(a.has_appearance for a in ozaru.annotations(out) if a.subtype != "Link")
    # They are drawn.
    assert ozaru.render_page(out, 1) != ozaru.render_page(PDF, 1)


def test_note_text_defaults_and_text_box_comment(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.annotate(PDF, out, [ozaru.TextBox(1, (50, 400, 200, 460), "Hello ñ")])
    assert ozaru.annotations(out)[0].contents == "Hello ñ"


def test_delete_and_update_annotations(tmp_path):
    marked, out = tmp_path / "marked.pdf", tmp_path / "out.pdf"
    ozaru.annotate(
        PDF,
        marked,
        [
            ozaru.Highlight(1, (50, 100, 150, 120), author="Ana", note="one"),
            ozaru.Underline(1, (50, 130, 150, 150), author="Ana", note="two"),
            ozaru.Highlight(2, (50, 100, 150, 120), author="Luis", note="three"),
            ozaru.Note(2, 10, 10, "four"),
        ],
    )
    assert ozaru.delete_annotations(marked, out, subtypes=["Highlight"]) == 2
    assert [a.contents for a in ozaru.annotations(out)] == ["two", "four"]
    assert ozaru.delete_annotations(marked, out, pages=[2]) == 2
    assert [a.contents for a in ozaru.annotations(out)] == ["one", "two"]
    assert ozaru.delete_annotations(marked, out, author="Ana", contains="tw") == 1
    assert ozaru.delete_annotations(marked, out, author="Nobody") == 0
    assert ozaru.delete_annotations(marked, out) == 4
    assert ozaru.annotations(out) == []
    # The pages' own content is untouched.
    assert ozaru.extract_text(out) == ozaru.extract_text(PDF)

    changed = ozaru.update_annotations(
        marked, out, {"contents": "revised", "subject": "topic", "hidden": True}, author="Ana"
    )
    assert changed == 2
    first = ozaru.annotations(out)[0]
    assert (first.contents, first.subject) == ("revised", "topic")
    assert first.flags & 2
    assert ozaru.render_page(out, 1) == ozaru.render_page(PDF, 1)  # hidden: not drawn
    assert ozaru.update_annotations(out, tmp_path / "again.pdf", {"hidden": False, "subject": None}, author="Ana") == 2
    again = ozaru.annotations(tmp_path / "again.pdf")[0]
    assert again.subject is None and not again.flags & 2


def test_new_annotation_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(ValueError, match="uri or a to_page"):
        ozaru.annotate(PDF, out, [ozaru.Link(1, (0, 0, 5, 5))])
    with pytest.raises(ValueError, match="uri or a to_page"):
        ozaru.annotate(PDF, out, [ozaru.Link(1, (0, 0, 5, 5), uri="https://x.org", to_page=1)])
    with pytest.raises(IndexError, match="page 9"):
        ozaru.annotate(PDF, out, [ozaru.Note(9, 0, 0, "x")])
    with pytest.raises(ValueError, match="no area"):
        ozaru.annotate(PDF, out, [ozaru.Box(1, (5, 5, 5, 50))])
    with pytest.raises(ValueError, match="strokes"):
        ozaru.annotate(PDF, out, [ozaru.Ink(1, [])])
    with pytest.raises(ValueError, match="can't draw a character"):
        ozaru.annotate(PDF, out, [ozaru.TextBox(1, (0, 0, 100, 100), "日本")])
    with pytest.raises(ValueError, match="start and end"):
        ozaru.annotate(PDF, out, [ozaru.Arrow(1, (0, 0), (1,))])
    with pytest.raises(ValueError, match="Unknown change"):
        ozaru.update_annotations(PDF, out, {"color": "red"})
    with pytest.raises(TypeError, match="text or None"):
        ozaru.update_annotations(PDF, out, {"contents": 5})
    with pytest.raises(IndexError):
        ozaru.delete_annotations(PDF, out, pages=[9])
    assert not out.exists()


def test_note_link_and_delete_commands(tmp_path):
    a, b, c = tmp_path / "a.pdf", tmp_path / "b.pdf", tmp_path / "c.pdf"

    def run(*args):
        return subprocess.run([sys.executable, "-m", "ozaru", *map(str, args)], capture_output=True, check=True)

    run("add-note", PDF, 1, 20, 30, "hello", "-o", a, "--author", "Ana")
    run("add-link", a, 1, 50, 100, 150, 120, "--uri", "https://example.com", "-o", b)
    assert [x.subtype for x in ozaru.annotations(b)] == ["Text", "Link"]
    done = run("delete-annotations", b, "-o", c, "--subtype", "Link")
    assert b"1 removed" in done.stdout
    assert [x.subtype for x in ozaru.annotations(c)] == ["Text"]
