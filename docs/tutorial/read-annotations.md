# Read the annotations

`ozaru.annotations` returns the annotations of every page of a PDF as
`Annotation` objects: the links, the notes and comments, the highlights, and the
widgets of form fields.

```python
--8<-- "docs_src/annotations.py"
```

Output:

```text
--8<-- "docs_src/annotations.out"
```

## What an annotation tells

- `subtype` is what kind it is, as the PDF names it: `Link`, `Text` for a note,
  `Highlight`, `FreeText`, `Square`, `Widget`...
- `rect` is where it is, as `(x0, y0, x1, y1)` in points from the top left corner
  of the page, the same as the box of a `Word`. That makes it easy to ask which
  words a link covers.
- `contents`, `author`, `subject`, `name` and `modified` are the text of a note
  and who wrote it, when the PDF has them, and `None` when it doesn't.
- `color` is its color, as 1, 3 or 4 numbers from 0 to 1, and `flags` the bits
  of the PDF standard: 2 is hidden and 4 is print.
- `quad_points` are the four corners of each line a highlight covers.
- `uri` is where a link to the web goes, and `target_page` the page of the same
  PDF a link goes to, counted from 1. A link to a named destination is followed to
  its page.

## Drawing them

`render_page` draws the annotations that have an appearance, `has_appearance`,
and the widgets of form fields, as a viewer does. Pass `annotations=False` to
draw the page without them. Hidden annotations, and pop-ups, aren't drawn.

## Protected and damaged PDFs

Pass `password` for a protected PDF. A page that can't be read has no
annotations, and a `PageWarning` tells you its number.
