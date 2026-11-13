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


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text, ozaru.extract_words, ozaru.extract_blocks, ozaru.extract_images, ozaru.metadata, ozaru.page_info])
def test_archivo_inexistente(funcion):
    with pytest.raises(FileNotFoundError) as error:
        funcion("no_existe.pdf")

    assert error.value.errno == errno.ENOENT
    assert error.value.filename == "no_existe.pdf"


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text, ozaru.extract_words, ozaru.extract_blocks, ozaru.extract_images, ozaru.metadata, ozaru.page_info])
def test_archivo_que_no_es_pdf(funcion):
    with pytest.raises(ValueError, match="Could not open the PDF"):
        funcion(__file__)


@pytest.mark.parametrize("funcion", [ozaru.page_count, ozaru.extract_text, ozaru.extract_words, ozaru.extract_blocks, ozaru.extract_images, ozaru.metadata, ozaru.page_info])
def test_ruta_de_tipo_invalido(funcion):
    with pytest.raises(TypeError):
        funcion(123)


DAMAGED_PDF = str(Path(__file__).parent / "damaged.pdf")


def test_extract_text_rebuilds_a_destroyed_page_tree_and_warns_about_the_damaged_page():
    # Its page tree object is damaged, and so is its second page.
    with pytest.warns(ozaru.PageWarning) as caught:
        pages = ozaru.extract_text(DAMAGED_PDF).split("\f")

    assert [warning.message.page for warning in caught] == [2]
    assert pages[0] == "one"
    assert pages[1] == ""
    assert len(pages) == 3


def test_page_count_rebuilds_a_destroyed_page_tree():
    # Its page tree object is damaged, and so is its second page.
    assert ozaru.page_count(DAMAGED_PDF) == 3


def test_page_info_rebuilds_a_destroyed_page_tree_and_warns_about_the_damaged_page():
    with pytest.warns(ozaru.PageWarning) as caught:
        pages = ozaru.page_info(DAMAGED_PDF)

    assert [page.number for page in pages] == [1, 2, 3]
    assert [warning.message.page for warning in caught] == [2]


def test_damaged_pdf_does_not_affect_later_files():
    expected = ozaru.extract_text(PDF)

    with pytest.warns(ozaru.PageWarning):
        ozaru.extract_text(DAMAGED_PDF)

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


def test_pages_without_content_are_empty_and_do_not_warn():
    # Its first two pages have no content stream, which made ozaru 0.2 crash.
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        pages = ozaru.extract_text(PAGE_PANIC_PDF).split("\f")

    assert pages[:2] == ["", ""]
    assert pages[2]


def test_pages_without_content_print_nothing_to_the_terminal():
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


FUNCTIONS = [ozaru.page_count, ozaru.extract_text, ozaru.extract_words, ozaru.extract_blocks, ozaru.extract_images, ozaru.metadata, ozaru.page_info]


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
    # Its page tree object is damaged.
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


# Page info

A4 = (595.2756, 841.8898)
A5 = (419.5276, 595.2756)
LETTER = (612.0, 792.0)


def sizes(path):
    return [(page.number, round(page.width, 4), round(page.height, 4), page.rotation) for page in ozaru.page_info(path)]


def test_page_info_gives_each_page_in_order():
    assert sizes(PDF) == [(number, *A4, 0) for number in (1, 2, 3)]


def test_page_info_gives_the_size_as_a_viewer_shows_it():
    # A4, Letter, landscape A4, A5 turned 90, A4 turned 180, Letter turned 270.
    assert sizes(ENGINE_CORPUS / "sizes-and-rotations.pdf") == [
        (1, *A4, 0),
        (2, *LETTER, 0),
        (3, A4[1], A4[0], 0),
        (4, *A5, 90),
        (5, *A4, 180),
        (6, *LETTER, 270),
    ]


def test_page_info_keeps_the_boxes_of_the_file_unrotated():
    page = ozaru.page_info(ENGINE_CORPUS / "inherited-attributes.pdf")[2]

    assert page.media_box == (0.0, 0.0, 595.0, 842.0)
    assert page.crop_box == (0.0, 0.0, 420.0, 595.0)
    assert page.rotation == 270
    assert (page.width, page.height) == (595.0, 420.0)


def test_page_info_has_one_entry_per_page():
    assert len(ozaru.page_info(ENGINE_CORPUS / "many-pages.pdf")) == ozaru.page_count(ENGINE_CORPUS / "many-pages.pdf")


def test_page_info_works_on_password_protected_pdf():
    assert len(ozaru.page_info(PASSWORD_PDF)) == 1


def test_page_info_warns_about_a_damaged_page():
    damaged_page = Path(__file__).parent.parent / "docs" / "assets" / "damaged-page.pdf"

    with pytest.warns(ozaru.PageWarning) as caught:
        pages = ozaru.page_info(damaged_page)

    assert len(pages) == 3
    assert [warning.message.page for warning in caught] == [3]
    assert "size and rotation" in caught[0].message.reason


def test_page_info_of_a_valid_pdf_does_not_warn():
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        ozaru.page_info(PDF)


def test_page_info_is_read_only():
    page = ozaru.page_info(PDF)[0]

    with pytest.raises(AttributeError):
        page.width = 100.0


def test_page_info_repr():
    assert repr(ozaru.page_info(ENGINE_CORPUS / "sizes-and-rotations.pdf")[1]) == (
        "PageInfo(number=2, width=612.0, height=792.0, rotation=0)"
    )


# Words

REPORT_PDF = Path(__file__).parent.parent / "docs" / "assets" / "report.pdf"


def test_extract_words_gives_each_word_with_its_page_line_and_box():
    words = ozaru.extract_words(REPORT_PDF)

    assert [w.text for w in words[:3]] == ["Quarterly", "Report", "Prepared"]
    first = words[0]
    assert (first.page, first.line) == (1, 1)
    assert first.bbox == (first.x0, first.y0, first.x1, first.y1)
    # From the top left corner: x0 = 72 points from the left, and the title is
    # near the top of the page.
    assert first.x0 == pytest.approx(72.0)
    assert 0 < first.y0 < first.y1 < 100
    assert words[2].line == 2


def test_extract_words_has_the_text_of_extract_text():
    text = ozaru.extract_text(PDF)
    words = ozaru.extract_words(PDF)

    assert " ".join(w.text for w in words) == " ".join(text.split())


def test_extract_words_counts_pages_and_lines_from_one():
    words = ozaru.extract_words(ENGINE_CORPUS / "many-pages.pdf")

    assert {w.page for w in words} == set(range(1, 501))
    assert min(w.line for w in words) == 1


def test_extract_words_warns_about_a_damaged_page_and_keeps_the_others():
    with pytest.warns(ozaru.PageWarning) as caught:
        words = ozaru.extract_words(PAGE_ERROR_PDF)

    assert [w.message.page for w in caught] == [3]
    assert {w.page for w in words} == {1, 2}


def test_a_word_is_read_only_and_compares_by_value():
    first, again = ozaru.extract_words(PDF)[0], ozaru.extract_words(PDF)[0]

    assert first == again and hash(first) == hash(again)
    with pytest.raises(AttributeError):
        first.text = "other"


def test_word_repr():
    assert repr(ozaru.extract_words(REPORT_PDF)[0]) == (
        "Word(page=1, block=1, line=1, text='Quarterly', x0=72.00, y0=68.97, x1=152.03, y1=85.62)"
    )


def test_extract_blocks_gives_each_block_with_its_text_and_box():
    blocks = ozaru.extract_blocks(REPORT_PDF)

    assert [(b.page, b.number) for b in blocks[:3]] == [(1, 1), (1, 2), (2, 1)]
    first = blocks[0]
    assert first.text == "Quarterly Report"
    assert first.bbox == (first.x0, first.y0, first.x1, first.y1)
    assert first.x0 == pytest.approx(72.0)
    assert blocks[1].text == "Prepared by the ozaru team.\nThis sample document has three pages."


def test_the_words_of_a_block_are_inside_it():
    blocks = {(b.page, b.number): b for b in ozaru.extract_blocks(PDF)}
    for word in ozaru.extract_words(PDF):
        block = blocks[(word.page, word.block)]
        assert word.text in block.text
        assert block.x0 <= word.x0 and word.x1 <= block.x1
        assert block.y0 <= word.y0 and word.y1 <= block.y1


def test_extract_blocks_warns_about_a_damaged_page_and_keeps_the_others():
    with pytest.warns(ozaru.PageWarning) as caught:
        blocks = ozaru.extract_blocks(PAGE_ERROR_PDF)

    assert [w.message.page for w in caught] == [3]
    assert {b.page for b in blocks} == {1, 2}


def test_block_repr():
    assert repr(ozaru.extract_blocks(REPORT_PDF)[0]) == (
        "Block(page=1, number=1, text='Quarterly Report', x0=72.00, y0=68.97, x1=215.03, y1=85.62)"
    )


# XMP metadata

XMP = """<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:xmp="http://ns.adobe.com/xap/1.0/"
  xmlns:pdf="http://ns.adobe.com/pdf/1.3/" pdf:Producer="Writer 2">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Informe año 2026</rdf:li></rdf:Alt></dc:title>
<dc:creator><rdf:Seq><rdf:li>José Núñez</rdf:li></rdf:Seq></dc:creator>
<xmp:CreateDate>2026-03-04T10:20:30-05:00</xmp:CreateDate>
</rdf:Description></rdf:RDF></x:xmpmeta>""".encode()


def test_metadata_takes_what_the_information_dictionary_leaves_out_from_xmp(tmp_path):
    pdf = tmp_path / "xmp.pdf"
    pdf.write_bytes(_one_page_pdf(b"", xmp=XMP))
    metadata = ozaru.metadata(pdf)

    assert metadata.title == "Informe año 2026"
    assert metadata.author == "José Núñez"
    assert metadata.producer == "Writer 2"
    assert metadata.creation_date == datetime(2026, 3, 4, 10, 20, 30, tzinfo=timezone(timedelta(hours=-5)))
    assert metadata.xmp == XMP.decode()


def test_the_information_dictionary_comes_before_xmp(tmp_path):
    pdf = tmp_path / "both.pdf"
    pdf.write_bytes(_one_page_pdf(b"", xmp=XMP, info=b"<< /Title (From Info) >>"))
    metadata = ozaru.metadata(pdf)

    assert metadata.title == "From Info"
    assert metadata.author == "José Núñez"


def test_a_pdf_without_xmp_has_none():
    assert ozaru.metadata(PDF).xmp is None


# Images

BROCHURE_PDF = Path(__file__).parent.parent / "docs" / "assets" / "brochure.pdf"


def test_extract_images_gives_each_image_with_its_box_and_file():
    photo, logo = ozaru.extract_images(BROCHURE_PDF)

    assert (photo.page, photo.number, photo.width, photo.height) == (1, 1, 240, 160)
    assert photo.format == "jpeg"
    assert photo.data[:2] == b"\xff\xd8"
    assert photo.bbox == pytest.approx((72.0, 122.0, 312.0, 282.0))
    assert (logo.number, logo.format, logo.width, logo.height) == (2, "png", 32, 32)
    assert logo.data.startswith(b"\x89PNG\r\n\x1a\n")
    assert logo.bbox == pytest.approx((480.0, 44.0, 528.0, 92.0))


def test_the_jpeg_of_an_image_is_the_one_in_the_pdf():
    pdf = BROCHURE_PDF.read_bytes()
    photo = ozaru.extract_images(BROCHURE_PDF)[0]

    assert photo.data in pdf


def test_an_image_is_saved_to_a_file(tmp_path):
    for image in ozaru.extract_images(BROCHURE_PDF):
        target = tmp_path / f"image.{image.format}"
        image.save(target)
        assert target.read_bytes() == image.data


def test_an_image_without_a_file_can_not_be_saved(tmp_path):
    # An image in a special ink: ozaru doesn't give its file yet.
    pdf = tmp_path / "ink.pdf"
    ink = b"[/Separation /Gold /DeviceCMYK << /FunctionType 2 /Domain [0 1] /N 1 >>]"
    content = b"q 10 0 0 10 0 0 cm BI /W 8 /H 1 /CS " + ink + b" /BPC 8 /F /AHx ID 0011223344556677> EI Q"
    pdf.write_bytes(_one_page_pdf(content))
    (image,) = ozaru.extract_images(pdf)

    assert (image.width, image.height, image.format, image.data) == (8, 1, None, None)
    with pytest.raises(ValueError, match="can't read this kind of image yet"):
        image.save(tmp_path / "ink.png")


def test_a_pdf_without_images_has_none():
    assert ozaru.extract_images(PDF) == []


def test_extract_images_warns_about_a_damaged_page():
    with pytest.warns(ozaru.PageWarning, match="Could not extract the images of page 3"):
        ozaru.extract_images(PAGE_ERROR_PDF)


def test_image_repr():
    assert repr(ozaru.extract_images(BROCHURE_PDF)[0]) == (
        "Image(page=1, number=1, size=240x160, format='jpeg', x0=72.00, y0=122.00, x1=312.00, y1=282.00)"
    )


def _one_page_pdf(content: bytes, xmp: bytes | None = None, info: bytes | None = None) -> bytes:
    catalog = b"<< /Type /Catalog /Pages 2 0 R%s >>" % (b" /Metadata 5 0 R" if xmp else b"")
    objects = [
        catalog,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
    ]
    if xmp:
        objects.append(b"<< /Type /Metadata /Subtype /XML /Length %d >>\nstream\n" % len(xmp) + xmp + b"\nendstream")
    out = b"%PDF-1.7\n"
    offsets = []
    for number, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1) + b"".join(b"%010d 00000 n \n" % o for o in offsets)
    trailer = b"/Size %d /Root 1 0 R%s" % (len(objects) + 1, b" /Info " + info if info else b"")
    return out + b"trailer << %s >>\nstartxref\n%d\n%%%%EOF\n" % (trailer, xref)


# Passwords

USER_PASSWORD_PDFS = [
    Path(__file__).parent.parent / "crates" / "ozaru-engine" / "tests" / "files" / "report-user-password.pdf",
    ENGINE_CORPUS / "aes-256-user-password.pdf",
]
ALL_FUNCTIONS = [ozaru.extract_text, ozaru.extract_words, ozaru.extract_blocks, ozaru.extract_images, ozaru.page_count, ozaru.metadata, ozaru.page_info]


def test_password_error_is_a_value_error():
    assert issubclass(ozaru.PasswordError, ValueError)


@pytest.mark.parametrize("pdf", USER_PASSWORD_PDFS, ids=lambda path: path.name)
@pytest.mark.parametrize("password", ["user", "owner"])
def test_the_user_or_owner_password_opens_a_protected_pdf(pdf, password):
    pages = ozaru.page_count(pdf, password=password)

    assert len(ozaru.extract_text(pdf, password=password).split("\f")) == pages
    assert len(ozaru.page_info(pdf, password=password)) == pages
    assert ozaru.metadata(pdf, password=password).title is not None


@pytest.mark.parametrize("pdf", USER_PASSWORD_PDFS, ids=lambda path: path.name)
@pytest.mark.parametrize("function", ALL_FUNCTIONS, ids=lambda f: f.__name__)
def test_a_wrong_password_raises_password_error(pdf, function):
    with pytest.raises(ozaru.PasswordError, match="incorrect"):
        function(pdf, password="wrong")


@pytest.mark.parametrize("pdf", USER_PASSWORD_PDFS, ids=lambda path: path.name)
@pytest.mark.parametrize("function", [ozaru.extract_text, ozaru.extract_words, ozaru.extract_blocks, ozaru.extract_images, ozaru.metadata], ids=lambda f: f.__name__)
def test_a_missing_password_raises_password_error(pdf, function):
    with pytest.raises(ozaru.PasswordError, match="password argument"):
        function(pdf)


def test_the_text_with_the_password_is_the_text_of_the_unprotected_pdf():
    # report-user-password.pdf is a Ghostscript rewrite of the sample report.
    protected = ozaru.extract_text(USER_PASSWORD_PDFS[0], password="user")
    rewritten = ozaru.extract_text(USER_PASSWORD_PDFS[0].parent / "report-flate.pdf")

    assert protected == rewritten


@pytest.mark.parametrize("function", ALL_FUNCTIONS, ids=lambda f: f.__name__)
def test_a_password_is_ignored_for_an_unprotected_pdf(function):
    assert function(PDF, password="anything") == function(PDF)


@pytest.mark.parametrize("function", ALL_FUNCTIONS, ids=lambda f: f.__name__)
def test_the_password_is_a_keyword_argument(function):
    with pytest.raises(TypeError):
        function(PDF, "user")


# Comparing results

def test_metadata_of_the_same_pdf_are_equal_and_hash_alike():
    first, second = ozaru.metadata(PDF), ozaru.metadata(PDF)

    assert first == second
    assert hash(first) == hash(second)
    assert len({first, second}) == 1


def test_metadata_of_different_pdfs_are_not_equal():
    assert ozaru.metadata(PDF) != ozaru.metadata(ENGINE_CORPUS / "unicode-info.pdf")
    assert ozaru.metadata(PDF) != "Documento de prueba"


def test_page_info_can_be_compared_and_hashed():
    pages = ozaru.page_info(ENGINE_CORPUS / "sizes-and-rotations.pdf")

    assert pages == ozaru.page_info(ENGINE_CORPUS / "sizes-and-rotations.pdf")
    # Every page has its own number, so no two are equal.
    assert len(set(pages)) == len(pages)
    assert {pages[0]: "first"}[ozaru.page_info(ENGINE_CORPUS / "sizes-and-rotations.pdf")[0]] == "first"


def test_metadata_without_information_can_be_hashed():
    assert hash(ozaru.metadata(ENGINE_CORPUS / "pdf-1.0.pdf")) == hash(ozaru.metadata(ENGINE_CORPUS / "pdf-1.0.pdf"))


# Passwords as they are typed

@pytest.mark.parametrize(
    ("name", "typed"),
    [
        # SASLprep turns the Roman numeral into the letters IX.
        ("aes-256-password-ix.pdf", "Ⅸ"),
        ("aes-256-password-ix.pdf", "IX"),
        # A no-break space, as copied from a web page, is a space.
        ("aes-256-password-space.pdf", "a\u00a0b"),
        ("aes-256-password-space.pdf", "a b"),
        # Revision 4 stores one byte per character.
        ("aes-128-password-accent.pdf", "año"),
    ],
)
@pytest.mark.parametrize("function", ALL_FUNCTIONS, ids=lambda f: f.__name__)
def test_passwords_open_their_pdf_as_they_are_typed(name, typed, function):
    function(ENGINE_CORPUS / name, password=typed)


@pytest.mark.parametrize(
    ("name", "typed"),
    [
        ("aes-256-password-ix.pdf", "ix"),
        ("aes-256-password-space.pdf", "ab"),
        ("aes-128-password-accent.pdf", "ano"),
    ],
)
def test_similar_passwords_are_still_wrong(name, typed):
    with pytest.raises(ozaru.PasswordError, match="incorrect"):
        ozaru.metadata(ENGINE_CORPUS / name, password=typed)
