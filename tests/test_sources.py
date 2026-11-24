"""Every function reads a PDF from a path, bytes, a file object or a URL."""

import functools
import http.server
import io
import threading
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"
DATA = PDF.read_bytes()


def test_bytes_bytearray_and_memoryview():
    expected = ozaru.extract_text(PDF)
    assert ozaru.extract_text(DATA) == expected
    assert ozaru.extract_text(bytearray(DATA)) == expected
    assert ozaru.extract_text(memoryview(DATA)) == expected
    assert ozaru.page_count(DATA) == 3


def test_file_objects():
    with open(PDF, "rb") as file:
        assert ozaru.page_count(file) == 3
    assert ozaru.page_count(io.BytesIO(DATA)) == 3
    assert ozaru.metadata(io.BytesIO(DATA)) == ozaru.metadata(PDF)


def test_text_mode_file_is_explained():
    with open(PDF, encoding="latin-1") as file:
        with pytest.raises(ValueError, match="binary mode"):
            ozaru.page_count(file)


def test_wrong_type_is_a_type_error():
    with pytest.raises(TypeError, match="bytes"):
        ozaru.page_count(42)


def test_every_reader_takes_bytes():
    assert ozaru.extract_words(DATA) == ozaru.extract_words(PDF)
    assert ozaru.extract_blocks(DATA) == ozaru.extract_blocks(PDF)
    assert ozaru.page_info(DATA) == ozaru.page_info(PDF)
    assert ozaru.annotations(DATA) == ozaru.annotations(PDF)
    assert ozaru.form_fields(DATA) == ozaru.form_fields(PDF)
    assert ozaru.permissions(DATA) == ozaru.permissions(PDF)
    assert len(ozaru.extract_images(DATA)) == len(ozaru.extract_images(PDF))
    assert ozaru.render_page(DATA, 1) == ozaru.render_page(PDF, 1)


def test_writers_take_bytes(tmp_path):
    ozaru.rotate(DATA, tmp_path / "r.pdf", 90, pages=[1])
    assert ozaru.page_info(tmp_path / "r.pdf")[0].rotation == 90
    ozaru.extract_pages(DATA, tmp_path / "e.pdf", [3, 1])
    assert ozaru.page_count(tmp_path / "e.pdf") == 2
    ozaru.fill_form(DATA, tmp_path / "f.pdf", {})
    ozaru.encrypt(DATA, tmp_path / "x.pdf", user_password="a")
    with pytest.raises(ozaru.PasswordError):
        ozaru.extract_text(tmp_path / "x.pdf")
    assert ozaru.page_count(tmp_path / "x.pdf", password="a") == 3


def test_split_and_merge_with_bytes(tmp_path):
    parts = ozaru.split(DATA, tmp_path / "parts")
    assert [part.name for part in parts] == ["part-1.pdf", "part-2.pdf", "part-3.pdf"]
    ozaru.merge([DATA, io.BytesIO(DATA), PDF], tmp_path / "all.pdf")
    assert ozaru.page_count(tmp_path / "all.pdf") == 9


def test_merge_names_the_damaged_file():
    with pytest.raises(ValueError, match="from memory"):
        ozaru.merge([DATA, b"not a pdf"], "unused.pdf")


def test_url():
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(FILES))
    handler.log_message = lambda *args: None
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        url = f"http://127.0.0.1:{server.server_port}/factura.pdf"
        assert ozaru.page_count(url) == 3
        with pytest.raises(OSError):
            ozaru.page_count(f"http://127.0.0.1:{server.server_port}/missing.pdf")
    finally:
        server.shutdown()
