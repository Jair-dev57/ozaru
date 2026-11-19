# Draw a page as an image

`ozaru.render_page` draws a page of a PDF as a PNG image:

```python
--8<-- "docs_src/render_page.py"
```

Output:

```text
--8<-- "docs_src/render_page.out"
```

The arguments are the PDF and the number of the page, counted from 1, as
`page_info` numbers them. A page the PDF doesn't have raises an `IndexError`.
The result is the bytes of the PNG file: write them to a file, as above, or send
them somewhere without saving them.

## The size of the image

`scale` is how many pixels each point of the page takes. A point is 1/72 of an
inch, so the default, 1, draws 72 dots per inch, and 2 draws 144. The size in
pixels is rounded up: an A4 page, 595.28 by 841.89 points, is 596 by 842 pixels
at scale 1 and 1191 by 1684 at scale 2.

The image is of the page as a viewer shows it: its crop box, turned by the
rotation the page has. A scale that would make an image of more than 256 MB
raises a `ValueError`: use a smaller one.

## What is drawn

Text, lines and shapes with their colors, transparency and clipping, and images,
including those with a mask. The page is drawn on white paper, as the writer
of the PDF saw it.

The fonts are the ones embedded in the PDF: TrueType, CFF, Type 1 and Type 3
fonts, simple and composite. A font that isn't embedded, such as Helvetica, is
drawn with a font of similar shape and the same measures.

A few things are left out for now, and listed in [Limitations](../reference/limitations.md):
gradients, patterns that repeat, transparency groups and blend modes, and the
annotations of a page, such as form fields.

## Protected and damaged PDFs

Pass `password` for a protected PDF, as with every function. A damaged page is
drawn empty, and a `PageWarning` tells you its number, as in
[Handle errors](handle-errors.md#pages-that-cant-be-read).
