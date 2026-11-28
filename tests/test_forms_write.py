"""Adding form fields and flattening forms and annotations."""

import subprocess
import sys
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"  # three A4 pages


def everything():
    return [
        ozaru.TextField(1, "name", (100, 300, 300, 320), value="Ana María", background="#ffffcc"),
        ozaru.CheckBox(1, "agree", (100, 340, 116, 356), checked=True),
        ozaru.RadioGroup(
            1, "plan", [((100, 380, 116, 396), "basic"), ((140, 380, 156, 396), "pro")], selected="pro"
        ),
        ozaru.Dropdown(1, "lang", (100, 420, 250, 440), options=[("es", "Spanish"), "English"], value="es"),
        ozaru.ListBox(1, "likes", (100, 460, 250, 520), options=["a", "b", "c"], selected=["b"], multiple=True),
        ozaru.Button(1, "go", (100, 540, 200, 565), caption="Open", uri="https://example.com"),
        ozaru.SignatureField(2, "signature", (100, 600, 300, 650)),
    ]


def test_every_kind_of_field_is_added_and_read_back(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.add_fields(PDF, out, everything())
    fields = {f.name: f for f in ozaru.form_fields(out)}
    assert list(fields) == ["name", "agree", "plan", "lang", "likes", "go", "signature"]
    assert fields["name"].kind == "text" and fields["name"].value == "Ana María"
    assert fields["agree"].kind == "checkbox" and fields["agree"].value is True
    assert fields["plan"].kind == "radio" and fields["plan"].value == "pro"
    assert fields["plan"].page == 1 and fields["signature"].page == 2
    assert len([a for a in ozaru.annotations(out) if a.subtype == "Widget"]) == 8
    assert fields["lang"].options == [("es", "Spanish"), ("English", "English")]
    assert fields["likes"].value == ["b"]
    assert fields["go"].kind == "button" and fields["signature"].kind == "signature"
    # The positions are from the top left, as the rest.
    assert fields["name"].rect == pytest.approx((100, 300, 300, 320), abs=0.01)
    # They are drawn, and the text of the page is as it was.
    assert ozaru.render_page(out, 1) != ozaru.render_page(PDF, 1)
    assert ozaru.extract_text(out) == ozaru.extract_text(PDF)


def test_a_new_field_can_be_filled_with_fill_form(tmp_path):
    made, filled = tmp_path / "made.pdf", tmp_path / "filled.pdf"
    ozaru.add_fields(PDF, made, everything())
    ozaru.fill_form(made, filled, {"name": "Luis", "agree": False, "plan": "basic", "lang": "English"})
    fields = {f.name: f for f in ozaru.form_fields(filled)}
    assert fields["name"].value == "Luis" and fields["agree"].value is False
    assert fields["plan"].value == "basic" and fields["lang"].value == "English"


def test_form_options(tmp_path):
    out = tmp_path / "out.pdf"
    ozaru.add_fields(
        PDF,
        out,
        [
            ozaru.TextField(1, "bio", (50, 300, 250, 400), multiline=True, font_size=10, align="right", max_length=50),
            ozaru.TextField(1, "pin", (50, 420, 150, 440), password=True, value="1234", read_only=True, required=True),
        ],
    )
    bio, pin = ozaru.form_fields(out)
    assert bio.multiline and bio.max_length == 50 and not bio.read_only
    assert pin.read_only and pin.required and not pin.multiline


def test_flatten_form_makes_the_fields_part_of_the_page(tmp_path):
    made, flat = tmp_path / "made.pdf", tmp_path / "flat.pdf"
    ozaru.add_fields(PDF, made, everything())
    before = ozaru.render_page(made, 1)
    count = ozaru.flatten_form(made, flat)
    assert count == 8  # the radio group has two widgets
    assert ozaru.form_fields(flat) == []
    assert [a for a in ozaru.annotations(flat) if a.subtype == "Widget"] == []
    # It looks the same, and now the values are text of the page.
    assert ozaru.render_page(flat, 1) == before
    text = ozaru.extract_text(flat)
    assert "Ana María" in text and "Spanish" in text


def test_flatten_some_fields_or_pages(tmp_path):
    made, out = tmp_path / "made.pdf", tmp_path / "out.pdf"
    ozaru.add_fields(PDF, made, everything())
    assert ozaru.flatten_form(made, out, fields=["name", "plan"]) == 3
    assert [f.name for f in ozaru.form_fields(out)] == ["agree", "lang", "likes", "go", "signature"]
    assert ozaru.flatten_form(made, out, pages=[2]) == 1
    assert "signature" not in [f.name for f in ozaru.form_fields(out)]
    assert ozaru.flatten_form(made, out, fields=["nothing"]) == 0
    assert len(ozaru.form_fields(out)) == 7


def test_flatten_a_form_that_was_filled(tmp_path):
    made, filled, flat = tmp_path / "made.pdf", tmp_path / "filled.pdf", tmp_path / "flat.pdf"
    ozaru.add_fields(PDF, made, [ozaru.TextField(1, "city", (100, 300, 300, 320))])
    ozaru.fill_form(made, filled, {"city": "Bogotá"})
    ozaru.flatten_form(filled, flat)
    assert "Bogotá" in ozaru.extract_text(flat)
    assert ozaru.form_fields(flat) == []


def test_flatten_annotations_keeps_links_and_fields(tmp_path):
    marked, out = tmp_path / "marked.pdf", tmp_path / "out.pdf"
    ozaru.annotate(
        PDF,
        marked,
        [
            ozaru.Highlight(1, (100, 100, 200, 120)),
            ozaru.Box(1, (300, 100, 400, 140), color="blue"),
            ozaru.Link(1, (50, 50, 90, 70), uri="https://example.com"),
        ],
    )
    before = ozaru.render_page(marked, 1)
    assert ozaru.flatten_annotations(marked, out) == 2
    assert [a.subtype for a in ozaru.annotations(out)] == ["Link"]
    assert ozaru.render_page(out, 1) == before
    assert ozaru.render_page(out, 1, annotations=False) == before
    assert ozaru.flatten_annotations(marked, out, subtypes=["Link"]) == 1
    assert ozaru.flatten_annotations(marked, out, subtypes=["Highlight"]) == 1
    assert sorted(a.subtype for a in ozaru.annotations(out)) == ["Link", "Square"]


def test_document_methods(tmp_path):
    doc = ozaru.open(PDF)
    doc.add_fields(tmp_path / "a.pdf", [ozaru.CheckBox(1, "c", (10, 10, 26, 26))])
    assert [f.name for f in ozaru.form_fields(tmp_path / "a.pdf")] == ["c"]
    assert ozaru.open(tmp_path / "a.pdf").flatten_form(tmp_path / "b.pdf") == 1


def test_errors(tmp_path):
    made, out = tmp_path / "made.pdf", tmp_path / "out.pdf"
    ozaru.add_fields(PDF, made, [ozaru.TextField(1, "taken", (10, 10, 100, 30))])
    box = (10, 100, 100, 120)
    with pytest.raises(ValueError, match="new in the form and have no dots"):
        ozaru.add_fields(made, out, [ozaru.TextField(1, "taken", box)])
    with pytest.raises(ValueError, match="no dots"):
        ozaru.add_fields(made, out, [ozaru.TextField(1, "a.b", box)])
    with pytest.raises(ValueError, match="name of a field"):
        ozaru.add_fields(made, out, [ozaru.TextField(1, "", box)])
    with pytest.raises(ValueError, match="different corners"):
        ozaru.add_fields(made, out, [ozaru.TextField(1, "flat", (5, 5, 5, 50))])
    with pytest.raises(ValueError, match="isn't one it can hold"):
        ozaru.add_fields(made, out, [ozaru.Dropdown(1, "d", box, options=["a"], value="zzz")])
    with pytest.raises(ValueError, match="isn't one it can hold"):
        ozaru.add_fields(made, out, [ozaru.RadioGroup(1, "r", [(box, "a")], selected="b")])
    with pytest.raises(ValueError, match="can't draw"):
        ozaru.add_fields(made, out, [ozaru.Button(1, "b", box, caption="日本")])
    with pytest.raises(ValueError, match="x0, y0, x1, y1"):
        ozaru.add_fields(made, out, [ozaru.TextField(1, "x", (1, 2, 3))])
    with pytest.raises(ValueError, match="align"):
        ozaru.add_fields(made, out, [ozaru.TextField(1, "x", box, align="top")])
    with pytest.raises(IndexError, match="no page 9"):
        ozaru.add_fields(made, out, [ozaru.TextField(9, "x", box)])
    with pytest.raises(TypeError, match="Add TextField"):
        ozaru.add_fields(made, out, ["x"])
    with pytest.raises(IndexError):
        ozaru.flatten_form(made, out, pages=[9])
    assert not out.exists()


def test_commands(tmp_path):
    made, flat = tmp_path / "made.pdf", tmp_path / "flat.pdf"
    ozaru.add_fields(PDF, made, everything())

    def run(*args):
        return subprocess.run([sys.executable, "-m", "ozaru", *map(str, args)], capture_output=True, check=True)

    done = run("flatten", made, "-o", flat, "--field", "name")
    assert b"1 flattened" in done.stdout
    assert "name" not in [f.name for f in ozaru.form_fields(flat)]
    marked = tmp_path / "marked.pdf"
    ozaru.annotate(PDF, marked, [ozaru.Highlight(1, (10, 10, 60, 30))])
    run("flatten", marked, "-o", flat, "--annotations")
    assert ozaru.annotations(flat) == []
