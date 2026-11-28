# Create form fields and flatten forms

## Add fields to a page

`ozaru.add_fields` puts form fields on pages, over what they have. Boxes are measured **from the top
left corner of the page**, as the boxes of words are.

```python
import ozaru

ozaru.add_fields("blank.pdf", "form.pdf", [
    ozaru.TextField(1, "name", (100, 200, 300, 220), value="", tooltip="Your name"),
    ozaru.TextField(1, "notes", (100, 240, 300, 320), multiline=True, font_size=10),
    ozaru.CheckBox(1, "agree", (100, 340, 116, 356)),
    ozaru.RadioGroup(1, "plan", [((100, 380, 116, 396), "basic"), ((160, 380, 176, 396), "pro")],
                     selected="basic"),
    ozaru.Dropdown(1, "country", (100, 420, 250, 440), options=[("co", "Colombia"), ("pe", "Peru")]),
    ozaru.ListBox(1, "likes", (100, 460, 250, 520), options=["tea", "coffee"], multiple=True),
    ozaru.Button(1, "site", (100, 540, 200, 565), caption="Open", uri="https://example.com"),
    ozaru.SignatureField(1, "signature", (100, 600, 300, 650)),
])
```

Each field has a **name** (new in the form, and without dots), a look (`color` of the text, `border`,
`background`, `border_width`, `font_size`, `align`) and flags (`read_only`, `required`). Every kind has its own
options: `TextField` takes `value`, `multiline`, `password` and `max_length`; `Dropdown` and `ListBox`
take `options` (texts, or `(value, text)` when the value they give is another) and `value` or
`selected`; a `RadioGroup` is one field made of several round buttons, whose value is the one that
is on; a `Button` opens a `uri`; a `SignatureField` is a place left empty to sign.

The fields are real: every reader shows them, and a new field is read by `ozaru.form_fields` and
filled with [`ozaru.fill_form`](fill-forms.md) like any other. The fields use Helvetica (the letters
of Western Europe). The form of the document is made if it has none, and the fields are added to it
if it has.

## Flatten a form

Flattening makes what a form shows part of the page, and removes the fields:

```python
count = ozaru.flatten_form("filled.pdf", "final.pdf")              # every field
ozaru.flatten_form("filled.pdf", "final.pdf", fields=["name", "plan"])  # some of them
ozaru.flatten_form("filled.pdf", "final.pdf", pages=[2])             # one page
```

After it, the values can't be changed and they are text of the page: `extract_text` finds them. A field
that had no drawing gets one from its value first, and hidden fields are removed without being drawn.
When no field is left, the form itself is removed. It returns how many widgets it flattened.
From the terminal: `uvx ozaru flatten filled.pdf -o final.pdf [--field name]`.

## Flatten annotations

```python
ozaru.flatten_annotations("reviewed.pdf", "final.pdf")                     # highlights, notes, shapes...
ozaru.flatten_annotations("reviewed.pdf", "final.pdf", subtypes=["Highlight"])
```

The drawing of each annotation goes into the page and the annotation is removed, so a viewer that
hides annotations still shows it, and a reader can't edit it. Form fields and links stay (they
are for `flatten_form` and they still work); name `Link` in `subtypes` to flatten links too (they
draw nothing). The command is `ozaru flatten reviewed.pdf -o final.pdf --annotations`.

Next: [redact text and images](redact.md).
