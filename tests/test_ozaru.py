from pathlib import Path

import pytest

import ozaru

PDF = str(Path(__file__).parent / "factura.pdf")


def test_page_count():
    assert ozaru.page_count(PDF) == 3


def test_extract_text_conserva_acentos():
    texto = ozaru.extract_text(PDF)
    assert "Factura de prueba" in texto
    assert "Señor José Muñoz Peña" in texto


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text])
def test_archivo_inexistente(funcion):
    with pytest.raises(FileNotFoundError):
        funcion("no_existe.pdf")


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text])
def test_archivo_que_no_es_pdf(funcion):
    with pytest.raises(ValueError):
        funcion(__file__)
