# Open a document once

The functions of ozaru take a PDF and read it every time. When you want several things
from the same file, `ozaru.open` reads it once and gives you its pages:

```python
--8<-- "docs_src/open_document.py"
```

Output:

```text
--8<-- "docs_src/open_document.out"
```

A `Document` has the length, the indexes and the slices of a list of pages
(`pdf[0]`, `pdf[-1]`, `pdf[1:3]`), and each `Page` has its `text`, `words`, `blocks`,
`images`, `annotations`, `form_fields`, `width`, `height`, `rotation` and `render()`.
What it reads is kept, so going through every page doesn't read the file again.

It also has what the functions do, as methods: `pdf.metadata`, `pdf.text`,
`pdf.rotate(output, 90)`, `pdf.split(folder)`, `pdf.fill_form(output, values)`...

`ozaru.open` takes the same as every function: a path, bytes, a file opened in binary
mode or a URL.

Next: [read page by page](read-page-by-page.md).
