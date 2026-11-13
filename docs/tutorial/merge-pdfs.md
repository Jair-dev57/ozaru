# Merge PDFs

`ozaru.merge` joins several PDFs into one file, in the order you give them:

```python
--8<-- "docs_src/merge.py"
```

Output:

```text
--8<-- "docs_src/merge.out"
```

Each page keeps its size, rotation, text, images and fonts. What several pages
of the same PDF share, such as a font or a logo, is saved once. The document
information, such as the title, comes from the first PDF.

To join only some pages, take them out first with
[`extract_pages`](split-pages.md#extract-some-pages), or join the PDFs and then
take the pages you want from the result.

## Protected PDFs

The `password` argument is used for the PDFs that need one, so you can join a
protected PDF with others that aren't. The joined file has no password. If a
PDF can't be opened, the error says which one:

```python
try:
    ozaru.merge(["report.pdf", "locked.pdf"], "joined.pdf")
except ozaru.PasswordError as error:
    print(error)  # locked.pdf: The PDF is protected with a password: ...
```

## What the new file keeps

As with [`extract_pages`](split-pages.md#what-the-new-files-keep), what belongs
to a whole document and points at its pages is left out: the bookmarks, the
forms, the named links between pages, the page labels and the accessibility
tags. A damaged page is saved empty, and a `PageWarning` gives its number in
the joined file.

Next: [protected PDFs](protected-pdfs.md).
