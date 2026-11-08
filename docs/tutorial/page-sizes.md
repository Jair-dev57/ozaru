# Page sizes

`ozaru.page_info` returns a `PageInfo` for each page of a PDF, with its size and
rotation:

```python
--8<-- "docs_src/page_info.py"
```

Output:

```text
--8<-- "docs_src/page_info.out"
```

Sizes are in points, 1/72 of an inch, as in every PDF: an A4 page is about 595 ×
842 points, and a US Letter page 612 × 792.

`width` and `height` are the page as a viewer shows it. A page turned 90 degrees
has them swapped, so a portrait page rotated to landscape reads as landscape.
`rotation` tells you by how much it is turned: 0, 90, 180 or 270 degrees,
clockwise.

For more detail, `media_box` and `crop_box` give the boxes the PDF defines, as
`(left, bottom, right, top)`, before the rotation. The crop box is the visible
area; `width` and `height` come from it.

If a page is too damaged to read, it gets the size and rotation it inherits, or
US Letter, and a `PageWarning` tells you its number.

Next: [read page by page](read-page-by-page.md).
