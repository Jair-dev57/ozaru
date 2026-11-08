import errno
from datetime import datetime, timedelta, timezone
import os
import subprocess
import sys
import warnings
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


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text, ozaru.metadata])
def test_archivo_inexistente(funcion):
    with pytest.raises(FileNotFoundError) as error:
        funcion("no_existe.pdf")

    assert error.value.errno == errno.ENOENT
    assert error.value.filename == "no_existe.pdf"


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text, ozaru.metadata])
def test_archivo_que_no_es_pdf(funcion):
    with pytest.raises(ValueError, match="Could not open the PDF"):
        funcion(__file__)


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text, ozaru.metadata])
def test_ruta_de_tipo_invalido(funcion):
    with pytest.raises(TypeError):
        funcion(123)


DAMAGED_PDF = str(Path(__file__).parent / "damaged.pdf")


# metadata doesn't need the page tree: see
# test_metadata_is_read_even_if_the_page_tree_is_destroyed.
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


ENGINE_CORPUS = Path(__file__).parent.parent / "crates" / "ozaru-engine" / "tests" / "corpus"


def test_page_count_needs_the_password_when_the_page_tree_is_encrypted():
    # Its page tree is inside an encrypted object stream.
    with pytest.raises(ValueError, match="protected with a password"):
        ozaru.page_count(ENGINE_CORPUS / "aes-256-user-password.pdf")


def test_page_count_reads_a_pdf_with_a_damaged_catalog():
    # Its catalog and its third page are damaged on purpose.
    assert ozaru.page_count(Path(__file__).parent.parent / "docs" / "assets" / "damaged-page.pdf") == 3


def test_pdf_with_only_an_owner_password_can_be_read():
    assert "visible text" in ozaru.extract_text(OWNER_PASSWORD_PDF)


PAGE_ERROR_PDF = str(Path(__file__).parent / "page_error.pdf")
PAGE_PANIC_PDF = str(Path(__file__).parent / "page_panic.pdf")


def test_failed_page_warns_and_keeps_the_other_pages():
    expected = ozaru.extract_text(PDF).split("\f")

    with pytest.warns(ozaru.PageWarning) as caught:
        pages = ozaru.extract_text(PAGE_ERROR_PDF).split("\f")

    assert [warning.message.page for warning in caught] == [3]
    assert caught[0].message.reason
    assert "page 3" in str(caught[0].message)
    assert pages == [expected[0], expected[1], ""]


def test_pages_after_a_crashing_page_are_still_read():
    with pytest.warns(ozaru.PageWarning) as caught:
        pages = ozaru.extract_text(PAGE_PANIC_PDF).split("\f")

    assert [warning.message.page for warning in caught] == [1, 2]
    assert pages[:2] == ["", ""]
    assert pages[2]


def test_crashing_pages_print_nothing_to_the_terminal():
    # The crash inside pdf_oxide is handled and becomes a PageWarning, so its
    # "panicked at" message shouldn't reach the user's terminal.
    script = (
        "import warnings, ozaru\n"
        "warnings.simplefilter('ignore')\n"
        f"ozaru.extract_text({PAGE_PANIC_PDF!r})\n"
    )

    result = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, check=True
    )

    assert "panicked" not in result.stderr, result.stderr
    assert result.stderr == ""


def test_page_warnings_can_be_turned_into_errors():
    with warnings.catch_warnings():
        warnings.simplefilter("error", ozaru.PageWarning)
        with pytest.raises(ozaru.PageWarning) as error:
            ozaru.extract_text(PAGE_ERROR_PDF)

    assert error.value.page == 3


def test_page_warning_is_a_user_warning():
    assert issubclass(ozaru.PageWarning, UserWarning)


def test_valid_pdf_does_not_warn():
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        ozaru.extract_text(PDF)


FUNCTIONS = [ozaru.page_count, ozaru.extract_text, ozaru.metadata]


@pytest.mark.parametrize("function", FUNCTIONS)
def test_directory_raises_is_a_directory_error(function, tmp_path):
    with pytest.raises(IsADirectoryError) as error:
        function(tmp_path)

    assert error.value.errno == errno.EISDIR
    assert error.value.filename == str(tmp_path)


@pytest.mark.skipif(os.name == "nt", reason="chmod can't remove read access on Windows")
@pytest.mark.skipif(hasattr(os, "geteuid") and os.geteuid() == 0, reason="root can read any file")
@pytest.mark.parametrize("function", FUNCTIONS)
def test_unreadable_file_raises_permission_error(function, tmp_path):
    pdf = tmp_path / "locked.pdf"
    pdf.write_bytes(Path(PDF).read_bytes())
    pdf.chmod(0)

    try:
        with pytest.raises(PermissionError) as error:
            function(pdf)
    finally:
        pdf.chmod(0o600)

    assert error.value.errno == errno.EACCES
    assert error.value.filename == str(pdf)


@pytest.mark.skipif(os.name == "nt", reason="Windows reports a missing file instead")
@pytest.mark.parametrize("function", FUNCTIONS)
def test_path_through_a_file_raises_not_a_directory_error(function):
    with pytest.raises(NotADirectoryError) as error:
        function(PDF + "/inside.pdf")

    assert error.value.errno == errno.ENOTDIR


@pytest.mark.parametrize("function", FUNCTIONS)
def test_path_with_null_byte_raises_value_error(function):
    with pytest.raises(ValueError, match="null byte"):
        function("bad\0name.pdf")


@pytest.mark.parametrize(
    "error_type", [FileNotFoundError, PermissionError, IsADirectoryError, NotADirectoryError]
)
def test_file_errors_are_os_errors(error_type):
    assert issubclass(error_type, OSError)


# Metadata

def test_metadata_reads_the_document_information():
    info = ozaru.metadata(PDF)

    assert info.title == "Documento de prueba"
    assert info.author == "Ozaru"
    assert info.producer == "ReportLab PDF Library - (opensource)"
    assert info.creation_date == datetime(2026, 10, 7, 15, 28, 30, tzinfo=timezone(timedelta(hours=-5)))
    assert info.modification_date == info.creation_date


def test_metadata_decodes_accents_and_symbols():
    info = ozaru.metadata(ENGINE_CORPUS / "unicode-info.pdf")

    assert info.title == "Informe año 2026 — ñandú ✓"
    assert info.author == "José Núñez"


def test_metadata_is_decrypted_when_no_password_is_needed():
    info = ozaru.metadata(OWNER_PASSWORD_PDF)

    assert info.title == "untitled"
    assert info.author == "anonymous"


def test_metadata_of_a_pdf_without_information_is_all_none():
    info = ozaru.metadata(ENGINE_CORPUS / "pdf-1.0.pdf")

    attributes = ["title", "author", "subject", "keywords", "creator", "producer"]
    assert [getattr(info, name) for name in attributes] == [None] * 6
    assert info.creation_date is None
    assert info.modification_date is None


def test_metadata_is_read_even_if_the_page_tree_is_destroyed():
    # Its page tree object is damaged, so page_count and extract_text fail.
    info = ozaru.metadata(DAMAGED_PDF)

    assert info.title == "untitled"
    assert info.producer == "ReportLab PDF Library - (opensource)"


def test_metadata_of_a_password_protected_pdf_raises_value_error():
    with pytest.raises(ValueError, match="protected with a password"):
        ozaru.metadata(PASSWORD_PDF)


def minimal_pdf(info: str) -> bytes:
    """A one-page PDF whose information dictionary is `info`."""
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        info.encode("latin-1"),
    ]
    out = bytearray(b"%PDF-1.7\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n%s\nendobj\n" % (number, body)
    start = len(out)
    out += b"xref\n0 5\n0000000000 65535 f \n"
    for offset in offsets:
        out += b"%010d 00000 n \n" % offset
    out += b"trailer\n<< /Size 5 /Root 1 0 R /Info 4 0 R >>\nstartxref\n%d\n%%%%EOF\n" % start
    return bytes(out)


def test_a_date_without_a_time_zone_is_naive(tmp_path):
    pdf = tmp_path / "naive.pdf"
    pdf.write_bytes(minimal_pdf("<< /CreationDate (D:20240131143005) /ModDate (yesterday) >>"))

    info = ozaru.metadata(pdf)

    assert info.creation_date == datetime(2024, 1, 31, 14, 30, 5)
    assert info.creation_date.tzinfo is None
    # A date that can't be read is None.
    assert info.modification_date is None


def test_metadata_is_read_only():
    info = ozaru.metadata(PDF)

    with pytest.raises(AttributeError):
        info.title = "Another title"


def test_metadata_repr_shows_every_attribute():
    text = repr(ozaru.metadata(PDF))

    assert text.startswith("Metadata(title='Documento de prueba', author='Ozaru',")
    assert "creation_date=datetime.datetime(2026, 10, 7, 15, 28, 30" in text
