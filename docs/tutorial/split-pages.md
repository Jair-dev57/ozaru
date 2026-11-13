# Extract and split pages

## Extract some pages

`ozaru.extract_pages` saves the pages you choose, in the order you give them,
to a new file:

```python
--8<-- "docs_src/extract_pages.py"
```

Output:

```text
--8<-- "docs_src/extract_pages.out"
```

Pages are numbered from 1, as `page_info` numbers them. The same list lets you
reorder pages, `[3, 2, 1]`, or repeat one, `[1, 1]`. A page number the PDF
doesn't have raises an `IndexError`, and the output file isn't written.

## Split a PDF

`ozaru.split` saves each page as its own file, in a folder, and returns their
paths as `pathlib.Path` objects:

```python
--8<-- "docs_src/split.py"
```

Output:

```text
--8<-- "docs_src/split.out"
```

The folder is created if it doesn't exist. The files are named after the PDF,
with the number of each part, padded with zeros when there are ten parts or
more, `report-01.pdf`, so they sort in order. Files with the same names are
replaced.

To keep several pages in each file, pass `pages_per_file`:

```python
ozaru.split("book.pdf", "chapters", pages_per_file=20)
```

The last file has the pages left over.

## What the new files keep

Each page keeps its size, rotation, text, images, fonts and links to web
pages. Only what the pages chosen use is saved, so a page taken from a large
PDF makes a small file. The document information, such as the title, is kept
too.

What belongs to the whole document and points at its pages is left out,
because it would point at the wrong pages: the bookmarks, the forms, the named
links between pages, the page labels and the accessibility tags. A link to a
page that wasn't saved stops working.

As with `rotate`, a protected PDF is saved without its password, and a damaged
page is saved empty, with a `PageWarning`.

Next: [merge PDFs](merge-pdfs.md).
