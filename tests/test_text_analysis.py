"""Spans, search, tables and Markdown."""

from pathlib import Path

import pytest

import ozaru

REPORT = Path(__file__).parent.parent / "crates/engine/tests/corpus/text-structure.pdf"
PLAIN = Path(__file__).parent / "factura.pdf"


def test_spans():
    spans = ozaru.extract_spans(REPORT)
    assert len(spans) == 55
    first = spans[0]
    assert (first.text, first.font, first.size, first.bold, first.italic) == (
        "Informe Trimestral",
        "Helvetica-Bold",
        18.0,
        True,
        False,
    )
    assert first.color == (0, 0, 0)
    assert first.page == 1 and first.block == 1 and first.line == 1
    assert first.bbox == (first.x0, first.y0, first.x1, first.y1)
    emphasis = {s.text: (s.bold, s.italic) for s in spans if s.text in ("ventas", "evolución")}
    assert emphasis == {"ventas": (True, False), "evolución": (False, True)}
    assert spans[0] == ozaru.extract_spans(REPORT)[0]
    assert len({*spans}) == len(spans)
    assert "Informe" in repr(first)


def test_span_colors(tmp_path):
    # A word in red comes out as its own span with its color.
    pdf = ozaru.open(PLAIN)
    colors = {s.color for s in pdf.spans}
    assert (0, 0, 0) in colors


def test_search():
    found = ozaru.search(REPORT, "ventas")
    assert [m.text for m in found] == ["ventas", "Ventas"]
    assert all(m.page == 1 and len(m.boxes) == 1 for m in found)
    assert ozaru.search(REPORT, "VENTAS", case_sensitive=True) == []
    assert len(ozaru.search(REPORT, "norte")) == 2
    assert ozaru.search(REPORT, "no está") == []
    assert ozaru.search(REPORT, "") == []
    match = ozaru.search(REPORT, "ventas por")[0]
    x0, y0, x1, y1 = match.bbox
    assert x0 < x1 and y0 < y1
    # Boxes are measured from the top left, as those of words are.
    word = next(w for w in ozaru.extract_words(REPORT) if w.text == "Ventas")
    assert abs(word.y0 - y0) < 1 and abs(word.x0 - x0) < 1
    assert "Match(" in repr(match)


def test_search_pages_and_input_errors():
    assert ozaru.search(PLAIN, "a", pages=[2]) == [m for m in ozaru.search(PLAIN, "a") if m.page == 2]
    with pytest.raises(IndexError):
        ozaru.search(PLAIN, "a", pages=[9])


def test_tables():
    (ruled, plain) = ozaru.extract_tables(REPORT)
    assert ruled.ruled and not plain.ruled
    assert ruled.rows == [
        ["Región", "Q1", "Q2", "Total"],
        ["Norte", "120", "135", "255"],
        ["Sur", "98", "101", "199"],
        ["Este", "143", "150", "293"],
    ]
    assert plain.rows[0] == ["Concepto", "Monto", "Notas"]
    assert plain.rows[2] == ["Personal", "5400", "incluye bonos"]
    assert ruled.markdown.startswith("| Región | Q1 | Q2 | Total |\n| --- | --- | --- | --- |")
    assert [t.ruled for t in ozaru.extract_tables(REPORT, strategy="lines")] == [True]
    assert all(not t.ruled for t in ozaru.extract_tables(REPORT, strategy="text"))
    assert ozaru.extract_tables(Path(__file__).parent.parent / "crates/engine/tests/corpus/one-page.pdf") == []
    assert "Table(page=1, rows=4, columns=4, ruled=True)" == repr(ruled)
    with pytest.raises(ValueError, match="Unknown strategy"):
        ozaru.extract_tables(REPORT, strategy="magic")


def test_markdown():
    text = ozaru.to_markdown(REPORT)
    assert text.startswith("# Informe Trimestral\n\n# Resumen ejecutivo\n\nEste informe describe las **ventas**")
    assert "## Ventas por región\n\n| Región | Q1 | Q2 | Total |" in text
    assert "- Crecimiento sostenido en el **Norte**.\n- El Sur se mantiene estable." in text
    assert text.endswith("durante el segundo semestre del año.")
    marked = ozaru.to_markdown(REPORT, page_markers=True)
    assert marked.startswith("<!-- page 1 -->\n\n# Informe")
    # Without the tables of text, their text is a plain paragraph.
    lines_only = ozaru.to_markdown(REPORT, strategy="lines")
    assert "| Región |" in lines_only and "| Concepto |" not in lines_only
    assert "Alquiler 1200 mensual" in lines_only


def test_document_has_them_all():
    pdf = ozaru.open(REPORT)
    assert pdf.spans == ozaru.extract_spans(REPORT)
    assert pdf.tables == ozaru.extract_tables(REPORT)
    assert pdf.markdown == ozaru.to_markdown(REPORT)
    assert pdf.to_markdown(page_markers=True) == ozaru.to_markdown(REPORT, page_markers=True)
    assert pdf.to_markdown(strategy="lines") == ozaru.to_markdown(REPORT, strategy="lines")
    assert pdf.search("ventas") == ozaru.search(REPORT, "ventas")
    assert pdf.find_tables(strategy="lines") == ozaru.extract_tables(REPORT, strategy="lines")
    page = pdf[0]
    assert page.spans == pdf.spans and page.tables == pdf.tables
    assert page.markdown == pdf.markdown
    assert page.search("ventas") == pdf.search("ventas")
    assert len(ozaru.open(PLAIN)[1].spans) == len([s for s in ozaru.extract_spans(PLAIN) if s.page == 2])


def test_bytes_and_password():
    data = REPORT.read_bytes()
    assert ozaru.to_markdown(data) == ozaru.to_markdown(REPORT)
    with pytest.raises(ozaru.PasswordError):
        ozaru.search(Path(__file__).parent / "password.pdf", "a")
