from pathlib import Path

import pytest

import ozaru

PDF = str(Path(__file__).parent / "factura.pdf")


def test_page_count():
    assert ozaru.page_count(PDF) == 3


def test_accepts_pathlib_path():
    assert ozaru.page_count(Path(PDF)) == 3
    assert ozaru.extract_text(Path(PDF)) == ozaru.extract_text(PDF)


def test_extract_text_conserva_acentos():
    texto = ozaru.extract_text(PDF)
    assert "Factura de prueba" in texto
    assert "Señor José Muñoz Peña" in texto


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text])
def test_archivo_inexistente(funcion):
    with pytest.raises(FileNotFoundError, match="File not found: no_existe.pdf"):
        funcion("no_existe.pdf")


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text])
def test_archivo_que_no_es_pdf(funcion):
    with pytest.raises(ValueError, match="Could not open the PDF"):
        funcion(__file__)


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text])
def test_ruta_de_tipo_invalido(funcion):
    with pytest.raises(TypeError):
        funcion(123)


DAMAGED_PDF = str(Path(__file__).parent / "damaged.pdf")


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text])
def test_damaged_pdf_raises_value_error(funcion):
    with pytest.raises(ValueError, match="Could not read the PDF"):
        funcion(DAMAGED_PDF)


def test_damaged_pdf_does_not_affect_later_files():
    expected = ozaru.extract_text(PDF)

    with pytest.raises(ValueError):
        ozaru.page_count(DAMAGED_PDF)

    assert ozaru.extract_text(PDF) == expected
