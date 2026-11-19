# Read and fill forms

`ozaru.form_fields` lists the fields of a PDF's form, and `ozaru.fill_form` fills
some of them and saves the result to another file:

```python
--8<-- "docs_src/forms.py"
```

Output:

```text
--8<-- "docs_src/forms.out"
```

## The fields

Each `FormField` has the `name` that identifies it, with the names of the groups
it is in joined by dots, and its `kind`: `text`, `checkbox`, `radio`, `button`,
`combo` (a drop-down list), `list` or `signature`. Its `value` is:

- a `str` for text and for the option chosen in a combo or list,
- a `bool` for a check box,
- the name of the chosen button for a radio group, or `None` if none is,
- a list of `str` for a list that allows several, and
- `None` when the field has no value.

`options` gives the choices of combos and lists, each as `(value, label)`;
`max_length`, `multiline`, `read_only` and `required` tell what the field
accepts; and `page` and `rect` say where it is, as for an `Annotation`.

## Filling

`fill_form(path, output, values)` takes a dictionary of field names to values:
a `str` for text, combos and lists, a `bool` for check boxes, the name of a button
for radio groups, a list of `str` for lists that allow several, and `None` to
clear a field. Fields that aren't in the dictionary are left as they are.

The fields' appearance is drawn again, with the font and size of the field's
default appearance (the size is chosen to fit when it is 0) and its colors, so
the values show in every reader. Text that doesn't fit a single-line field is
cut; a multiline field wraps its words. Characters outside Western European
text show as `?`, until the engine can draw with other fonts.

A `ValueError` says which field can't hold the value: one that isn't in the
PDF, text longer than `max_length`, an option a list doesn't have, a `bool`
for a text field, or a button or signature, which hold no value. Nothing is
saved then.

`incremental=True` adds the changes to the end of the original file, keeping
its bytes, as for `rotate`. Pass `password` for a protected PDF; the result is
saved without it.

`render_page` draws the widgets with these appearances, as in
[Read the annotations](read-annotations.md#drawing-them).
