import os
import subprocess
import sys
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


RELEASES_GIL = """
import sys
import threading

import ozaru

fifo, pdf = sys.argv[1:]
thread = threading.Thread(target=ozaru.page_count, args=(fifo,))
thread.start()
with open(fifo, "wb") as writer:
    writer.write(open(pdf, "rb").read())
thread.join()
"""


@pytest.mark.skipif(not hasattr(os, "mkfifo"), reason="requires named pipes")
def test_releases_the_gil_while_reading(tmp_path):
    # ozaru waits for data that only Python can write, so holding the GIL would hang.
    fifo = tmp_path / "input.pdf"
    os.mkfifo(fifo)

    subprocess.run([sys.executable, "-c", RELEASES_GIL, fifo, PDF], timeout=10, check=True)


PASSWORD_PDF = str(Path(__file__).parent / "password.pdf")
OWNER_PASSWORD_PDF = str(Path(__file__).parent / "owner_password.pdf")


def test_extract_text_rejects_password_protected_pdf():
    with pytest.raises(ValueError, match="password"):
        ozaru.extract_text(PASSWORD_PDF)


def test_page_count_works_on_password_protected_pdf():
    assert ozaru.page_count(PASSWORD_PDF) == 1


def test_pdf_with_only_an_owner_password_can_be_read():
    assert "visible text" in ozaru.extract_text(OWNER_PASSWORD_PDF)
