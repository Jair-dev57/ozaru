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
