"""Sanitizing: taking out metadata, scripts, attachments and more."""

import re
import subprocess
import sys
import zlib
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages


def with_everything(tmp_path):
    """The invoice with metadata, an attachment, bookmarks, comments, a field and a script."""
    stages = [tmp_path / f"s{n}.pdf" for n in range(6)]
    ozaru.set_metadata(PDF, stages[0], {"title": "title-secret", "author": "author-name", "xmp": "<x:xmpmeta>xmp-secret</x:xmpmeta>"})
    ozaru.add_attachment(stages[0], stages[1], "notes.txt", b"attached-secret")
    ozaru.set_bookmarks(stages[1], stages[2], [(1, "bookmark-secret", 1)])
    ozaru.annotate(
        stages[2],
        stages[3],
        [ozaru.Note(1, 300, 40, "note-secret", author="note-author"), ozaru.Link(1, (50, 50, 90, 70), uri="https://keep.me")],
    )
    ozaru.add_fields(stages[3], stages[4], [ozaru.TextField(1, "name", (100, 300, 300, 320), value="field-secret")])
    # A script an action runs when the document opens: written into the catalog.
    data = stages[4].read_bytes()
    marker = b"/Type/Catalog"
    assert marker in data or b"/Type /Catalog" in data
    data = data.replace(b"/Type/Catalog", b"/Type/Catalog/OpenAction<</S/JavaScript/JS(script-secret)>>", 1)
    data = data.replace(b"/Type /Catalog", b"/Type /Catalog /OpenAction << /S /JavaScript /JS (script-secret) >>", 1)
    stages[5].write_bytes(data)
    return stages[5]


def has(path, word):
    """Whether the word is in the file or in any stream of it, decompressed."""
    data = path.read_bytes()
    if word.encode() in data:
        return True
    for match in re.finditer(rb"stream\r?\n(.*?)endstream", data, re.S):
        try:
            if word.encode() in zlib.decompress(match.group(1)):
                return True
        except zlib.error:
            pass
    return False


def test_defaults_take_out_metadata_scripts_and_attachments(tmp_path):
    full = with_everything(tmp_path)
    for word in ("title-secret", "author-name", "xmp-secret", "attached-secret", "script-secret"):
        assert has(full, word), word
    out = tmp_path / "out.pdf"
    report = ozaru.sanitize(full, out)
    assert isinstance(report, ozaru.SanitizeReport) and not report.empty
    assert report.info_entries >= 2 and report.metadata_streams == 1 and report.scripts >= 1 and report.attachments >= 1
    for word in ("title-secret", "author-name", "xmp-secret", "attached-secret", "script-secret"):
        assert not has(out, word), word
    # The rest stays, and the pages are as they were.
    for word in ("bookmark-secret", "note-secret", "field-secret", "https://keep.me"):
        assert has(out, word), word
    assert ozaru.render_page(out, 1) == ozaru.render_page(full, 1)
    assert ozaru.extract_text(out) == ozaru.extract_text(full)
    assert ozaru.metadata(out).title is None
    assert ozaru.attachments(out) == []
    assert [b.title for b in ozaru.bookmarks(out)] == ["bookmark-secret"]


def test_everything(tmp_path):
    full = with_everything(tmp_path)
    out = tmp_path / "out.pdf"
    report = ozaru.sanitize(full, out, everything=True)
    assert (report.bookmarks, report.comments, report.form_values) == (1, 1, 1)
    for word in ("bookmark-secret", "note-secret", "note-author", "field-secret", "script-secret"):
        assert not has(out, word), word
    assert ozaru.bookmarks(out) == []
    # The link and the field are still there.
    assert [a.uri for a in ozaru.annotations(out) if a.subtype == "Link"] == ["https://keep.me"]
    fields = ozaru.form_fields(out)
    assert [f.name for f in fields] == ["name"] and fields[0].value in (None, "")


def test_options_choose_what_goes(tmp_path):
    full = with_everything(tmp_path)
    out = tmp_path / "out.pdf"
    ozaru.sanitize(full, out, metadata=False, scripts=False, attachments=True)
    assert not has(out, "attached-secret") and has(out, "title-secret") and has(out, "script-secret")
    ozaru.sanitize(full, out, metadata=False, scripts=True, attachments=False)
    assert not has(out, "script-secret") and has(out, "attached-secret")
    ozaru.sanitize(full, out, keep_info=["Title"])
    assert has(out, "title-secret") and not has(out, "author-name")
    ozaru.sanitize(full, out, keep_info="Title")  # a single name works too
    assert has(out, "title-secret")
    ozaru.sanitize(full, out, metadata=False, scripts=False, attachments=False, comments=True)
    assert not has(out, "note-secret") and has(out, "title-secret")


def test_nothing_to_take_out(tmp_path):
    out = tmp_path / "out.pdf"
    clean = tmp_path / "clean.pdf"
    ozaru.sanitize(PDF, clean)
    report = ozaru.sanitize(clean, out, everything=True)
    assert report.empty
    assert ozaru.extract_text(out) == ozaru.extract_text(PDF)


def test_a_protected_file(tmp_path):
    locked, out = tmp_path / "locked.pdf", tmp_path / "out.pdf"
    ozaru.encrypt(with_everything(tmp_path), locked, user_password="pw")
    with pytest.raises(ozaru.PasswordError):
        ozaru.sanitize(locked, out)
    ozaru.sanitize(locked, out, password="pw")
    assert not has(out, "title-secret")


def test_command(tmp_path):
    full = with_everything(tmp_path)
    out = tmp_path / "out.pdf"
    done = subprocess.run(
        [sys.executable, "-m", "ozaru", "sanitize", str(full), "-o", str(out), "--keep-info", "Title", "--comments"],
        capture_output=True,
        check=True,
    )
    assert b"sanitized" in done.stdout
    assert has(out, "title-secret") and not has(out, "author-name") and not has(out, "note-secret")
