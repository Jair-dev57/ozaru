"""Redaction: what is under an area is gone from the file."""

import re
import subprocess
import sys
import zlib
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages
PNG = FILES.parent / "crates/engine/tests/files/png/rgb8.png"


def everything_in(path: Path) -> bytes:
    """The bytes of a file and of every stream in it, decompressed: where a word could hide."""
    data = path.read_bytes()
    found = [data]
    for match in re.finditer(rb"stream\r?\n(.*?)endstream", data, re.S):
        try:
            found.append(zlib.decompress(match.group(1)))
        except zlib.error:
            pass
    return b"\n".join(found)


def test_redact_text_removes_it_for_good(tmp_path):
    out = tmp_path / "out.pdf"
    before = ozaru.search(PDF, "Muñoz")
    assert before
    count = ozaru.redact_text(PDF, out, "Muñoz")
    assert count == len(before)
    text = ozaru.extract_text(out)
    assert "Muñoz" not in text and "Mu" not in text.split("José")[1].split("Peña")[0]
    # The rest of the line is where it was.
    assert "José" in text and "Peña" in text
    words = {w.text: w.bbox for w in ozaru.extract_words(PDF)}
    after = {w.text: w.bbox for w in ozaru.extract_words(out)}
    for word in ("José", "Peña.", "Cliente:"):
        assert after[word] == pytest.approx(words[word], abs=0.01)
    # It isn't in the file in any form: not raw, not in a decompressed stream.
    hidden = everything_in(out)
    assert b"Mu\xf1oz" not in hidden and "Muñoz".encode() not in hidden
    # The page has a black box there.
    assert ozaru.render_page(out, 1) != ozaru.render_page(PDF, 1)


def test_redact_areas_and_the_report(tmp_path):
    out = tmp_path / "out.pdf"
    report = ozaru.redact(PDF, out, [ozaru.Redaction(1, (100, 100, 300, 125))])
    assert isinstance(report, ozaru.RedactionReport)
    assert report.glyphs > 10 and not report.empty
    assert (report.images_removed, report.forms, report.annotations) == (0, 0, 0)
    assert "Cliente" not in ozaru.extract_text(out).split("\x0c")[0]
    # Other pages are as they were.
    assert ozaru.extract_text(out).split("\x0c")[1:] == ozaru.extract_text(PDF).split("\x0c")[1:]


def test_an_area_with_nothing_under_it_removes_nothing(tmp_path):
    out = tmp_path / "out.pdf"
    report = ozaru.redact(PDF, out, [ozaru.Redaction(1, (400, 700, 500, 750), fill=None)])
    assert report.empty
    assert ozaru.extract_text(out) == ozaru.extract_text(PDF)
    assert ozaru.redact_text(PDF, tmp_path / "none.pdf", "text that isn't there") == 0
    assert ozaru.extract_text(tmp_path / "none.pdf") == ozaru.extract_text(PDF)


def test_no_fill_draws_no_box(tmp_path):
    black, none = tmp_path / "black.pdf", tmp_path / "none.pdf"
    area = [ozaru.Redaction(1, (100, 100, 300, 125))]
    ozaru.redact(PDF, black, area)
    ozaru.redact(PDF, none, [ozaru.Redaction(1, (100, 100, 300, 125), fill=None)])
    assert ozaru.render_page(black, 1) != ozaru.render_page(none, 1)
    assert ozaru.extract_text(black) == ozaru.extract_text(none)


def test_images_pixels_blanked_removed_or_kept(tmp_path):
    drawn = tmp_path / "drawn.pdf"
    ozaru.draw(PDF, drawn, [ozaru.Picture(1, 100, 300, PNG, width=130)])
    (image,) = [i for i in ozaru.extract_images(drawn) if i.page == 1]
    x0, y0, x1, y1 = image.bbox
    half = ozaru.Redaction(1, (x0, y0, (x0 + x1) / 2, y1), fill=None)

    blanked = tmp_path / "blanked.pdf"
    report = ozaru.redact(drawn, blanked, [half])
    assert (report.images_blanked, report.images_removed) == (1, 0)
    (after,) = [i for i in ozaru.extract_images(blanked) if i.page == 1]
    assert after.bbox == pytest.approx(image.bbox, abs=0.01)  # same size and place
    assert after.data != image.data

    removed = tmp_path / "removed.pdf"
    report = ozaru.redact(drawn, removed, [half], images="remove")
    assert report.images_removed == 1
    assert [i for i in ozaru.extract_images(removed) if i.page == 1] == []

    kept = tmp_path / "kept.pdf"
    assert ozaru.redact(drawn, kept, [half], images="keep").empty
    assert [i.data for i in ozaru.extract_images(kept) if i.page == 1] == [image.data]


def test_what_it_does_not_reach_is_said(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.annotate(PDF, tmp_path / "noted.pdf", [ozaru.Note(1, 150, 105, "a secret note")])
    report = ozaru.redact(tmp_path / "noted.pdf", out, [ozaru.Redaction(1, (100, 100, 300, 125))])
    assert report.annotations == 1
    assert ozaru.annotations(out) == []
    # Asked not to touch them, they stay.
    report = ozaru.redact(
        tmp_path / "noted.pdf", out, [ozaru.Redaction(1, (100, 100, 300, 125))], annotations=False
    )
    assert report.annotations == 0 and len(ozaru.annotations(out)) == 1


def test_a_page_pdf_with_a_password(tmp_path):
    protected, out = tmp_path / "locked.pdf", tmp_path / "out.pdf"
    ozaru.encrypt(PDF, protected, user_password="pw")
    ozaru.redact_text(protected, out, "Muñoz", password="pw")
    assert "Muñoz" not in ozaru.extract_text(out)


def test_errors(tmp_path):
    out = tmp_path / "out.pdf"
    with pytest.raises(IndexError, match="no page 9"):
        ozaru.redact(PDF, out, [ozaru.Redaction(9, (0, 0, 5, 5))])
    with pytest.raises(ValueError, match="x0, y0, x1, y1"):
        ozaru.redact(PDF, out, [ozaru.Redaction(1, (0, 0, 5))])
    with pytest.raises(ValueError, match="not a color"):
        ozaru.redact(PDF, out, [ozaru.Redaction(1, (0, 0, 5, 5), fill="blurple")])
    with pytest.raises(ValueError, match="images must be"):
        ozaru.redact(PDF, out, [ozaru.Redaction(1, (0, 0, 5, 5))], images="all")
    with pytest.raises(ValueError, match="graphics must be"):
        ozaru.redact(PDF, out, [ozaru.Redaction(1, (0, 0, 5, 5))], graphics="all")
    with pytest.raises(TypeError, match="Redaction"):
        ozaru.redact(PDF, out, [(1, 2, 3, 4)])
    assert not out.exists()


def test_command(tmp_path):
    out = tmp_path / "out.pdf"
    done = subprocess.run(
        [sys.executable, "-m", "ozaru", "redact", str(PDF), "Muñoz", "-o", str(out), "--fill", "red"],
        capture_output=True,
        check=True,
    )
    assert b"redacted" in done.stdout
    assert "Muñoz" not in ozaru.extract_text(out)
