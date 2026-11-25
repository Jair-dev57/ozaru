"""Bookmarks, page labels, named destinations and attached files."""

from datetime import datetime
from pathlib import Path

import pytest

import ozaru

NAVIGATION = Path(__file__).parent.parent / "crates/engine/tests/corpus/navigation.pdf"
PLAIN = Path(__file__).parent / "factura.pdf"


def test_bookmarks():
    found = ozaru.bookmarks(NAVIGATION)
    assert [(b.level, b.title, b.page) for b in found] == [
        (1, "Chapter 1", 1),
        (2, "Section 1.1", 2),
        (2, "Section 1.2 é ü 日本", 3),
        (3, "Deep", 4),
        (1, "Chapter 2", 5),
        (1, "Chapter 3", 7),
    ]
    assert found[0] == ozaru.bookmarks(NAVIGATION)[0]
    assert len({*found}) == len(found)
    assert "Chapter 1" in repr(found[0])
    assert found[0].uri is None


def test_page_labels():
    assert ozaru.page_labels(NAVIGATION) == ["i", "ii", "1", "2", "3", "App-A", "App-B"]
    # Without labels, the page numbers.
    assert ozaru.page_labels(PLAIN) == ["1", "2", "3"]


def test_named_destinations():
    found = {d.name: d.page for d in ozaru.named_destinations(NAVIGATION)}
    assert found == {"intro": 1, "end": 7}
    assert ozaru.named_destinations(PLAIN) == []
    assert "NamedDestination(name=" in repr(ozaru.named_destinations(NAVIGATION)[0])


def test_attachments(tmp_path):
    found = {a.name: a for a in ozaru.attachments(NAVIGATION)}
    assert set(found) == {"notes.txt", "data.bin"}
    assert found["notes.txt"].data == b"hello world"
    assert found["notes.txt"].filename == "notes.txt"
    assert found["data.bin"].data == bytes(range(256))
    found["notes.txt"].save(tmp_path / "out.txt")
    assert (tmp_path / "out.txt").read_bytes() == b"hello world"
    assert ozaru.attachments(PLAIN) == []
    assert "notes.txt" in repr(found["notes.txt"])


def test_dates_are_datetimes():
    for attachment in ozaru.attachments(NAVIGATION):
        assert attachment.creation_date is None or isinstance(attachment.creation_date, datetime)


def test_document_has_them_all():
    pdf = ozaru.open(NAVIGATION)
    assert pdf.bookmarks == ozaru.bookmarks(NAVIGATION)
    assert pdf.page_labels == ozaru.page_labels(NAVIGATION)
    assert pdf.named_destinations == ozaru.named_destinations(NAVIGATION)
    assert [a.name for a in pdf.attachments] == [a.name for a in ozaru.attachments(NAVIGATION)]
    assert [page.label for page in pdf] == pdf.page_labels
    assert pdf[5].label == "App-A"


def test_bytes_input():
    assert ozaru.page_labels(NAVIGATION.read_bytes())[0] == "i"


def test_password_is_asked_for():
    with pytest.raises(ozaru.PasswordError):
        ozaru.bookmarks(Path(__file__).parent / "password.pdf")
