"""`ozaru.open` reads a PDF once and gives its pages."""

import warnings
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"


def test_open_and_pages():
    pdf = ozaru.open(PDF)
    assert len(pdf) == pdf.page_count == 3
    assert repr(pdf) == "Document(3 pages)"
    assert [page.number for page in pdf] == [1, 2, 3]
    assert pdf[-1].number == 3
    assert [page.number for page in pdf[1:]] == [2, 3]
    assert [page.number for page in pdf[::-1]] == [3, 2, 1]
    assert repr(pdf[0]) == "Page(1)"


def test_constructor_and_context_manager():
    with ozaru.Document(PDF.read_bytes()) as pdf:
        assert len(pdf) == 3


def test_index_errors():
    pdf = ozaru.open(PDF)
    with pytest.raises(IndexError, match="no page 3"):
        pdf[3]
    with pytest.raises(IndexError):
        pdf[-4]
    with pytest.raises(TypeError):
        pdf["1"]


def test_text_matches_the_functions():
    pdf = ozaru.open(PDF)
    assert pdf.text == ozaru.extract_text(PDF)
    assert "\x0c".join(page.text for page in pdf) == pdf.text


def test_words_blocks_images_by_page():
    pdf = ozaru.open(PDF)
    assert pdf.words == ozaru.extract_words(PDF)
    assert pdf.blocks == ozaru.extract_blocks(PDF)
    assert len(pdf.images) == len(ozaru.extract_images(PDF))
    assert [w for page in pdf for w in page.words] == pdf.words
    assert [b for page in pdf for b in page.blocks] == pdf.blocks
    assert all(word.page == 2 for word in pdf[1].words)
    assert pdf[0].words is not pdf[0].words


def test_sizes_and_metadata():
    pdf = ozaru.open(PDF)
    infos = ozaru.page_info(PDF)
    assert pdf.page_info == infos
    assert pdf[0].info == infos[0]
    assert (pdf[0].width, pdf[0].height, pdf[0].rotation) == (
        infos[0].width,
        infos[0].height,
        infos[0].rotation,
    )
    assert pdf.metadata == ozaru.metadata(PDF)
    assert pdf.permissions == ozaru.permissions(PDF)
    assert pdf.annotations == ozaru.annotations(PDF)
    assert pdf.form_fields == ozaru.form_fields(PDF)
    assert pdf[0].annotations == [a for a in pdf.annotations if a.page == 1]


def test_render():
    pdf = ozaru.open(PDF)
    assert pdf.render(1) == ozaru.render_page(PDF, 1)
    assert pdf[1].render(scale=0.5) == ozaru.render_page(PDF, 2, scale=0.5)
    with pytest.raises(ValueError):
        pdf[0].render(scale=0)


def test_save_and_edit(tmp_path):
    pdf = ozaru.open(PDF)
    pdf.save(tmp_path / "copy.pdf")
    assert (tmp_path / "copy.pdf").read_bytes() == PDF.read_bytes()
    pdf.rotate(tmp_path / "r.pdf", 90, pages=[1])
    assert ozaru.page_info(tmp_path / "r.pdf")[0].rotation == 90
    pdf.extract_pages(tmp_path / "e.pdf", [3, 1])
    assert ozaru.page_count(tmp_path / "e.pdf") == 2
    assert len(pdf.split(tmp_path / "parts")) == 3
    pdf.fill_form(tmp_path / "f.pdf", {})
    pdf.encrypt(tmp_path / "x.pdf", user_password="a")
    assert ozaru.open(tmp_path / "x.pdf", password="a").text == pdf.text


def test_password():
    protected = FILES / "password.pdf"
    with pytest.raises(ozaru.PasswordError):
        ozaru.open(protected).text
    pdf = ozaru.open(protected, password="secret")
    assert pdf.text == ozaru.extract_text(protected, password="secret")
    with pytest.raises(ozaru.PasswordError):
        ozaru.open(protected, password="wrong")


def test_damaged_page_warns_once():
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        pdf = ozaru.open(FILES / "page_error.pdf")
        pdf.text
        pdf.text
        pdf[0].text
    assert len([w for w in caught if issubclass(w.category, ozaru.PageWarning)]) == 1


def test_errors_on_open():
    with pytest.raises(ValueError):
        ozaru.open(b"not a pdf")
    with pytest.raises(FileNotFoundError):
        ozaru.open("missing.pdf")
