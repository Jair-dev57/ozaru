# Rotate pages

`ozaru.rotate` turns pages of a PDF clockwise and saves the result to another
file:

```python
--8<-- "docs_src/rotate_pages.py"
```

Output:

```text
--8<-- "docs_src/rotate_pages.out"
```

The arguments are the PDF to read, the file to write and the degrees to turn,
a multiple of 90. Use a negative number to turn pages counterclockwise: `-90`
is the same as `270`.

Without `pages`, every page is turned. With it, only the pages listed are,
counted from 1, as `page_info` numbers them. A page number the PDF doesn't
have raises an `IndexError`, and the output file isn't written.

Turning adds to the rotation a page already has: turning by 90 a page shown at
90 degrees leaves it at 180. Only how viewers show the page changes; its
content, text and images stay as they are.

The output can be the same file as the input: ozaru reads the whole PDF before
writing.

## What the new file keeps

The new file has the whole document: its pages, bookmarks, links, forms and
metadata. ozaru writes it again from the objects the document uses, so objects
left over from earlier edits are not copied, and the file can come out smaller.

There are two exceptions:

- A **protected PDF** is saved without its password, so pass the `password`
  argument to rotate it. Anyone can then open the new file.
- A **damaged page** that can't be read is saved as an empty page of its size,
  and a `PageWarning` tells you its number, as in
  [Handle errors](handle-errors.md#pages-that-cant-be-read).

## Keep the original bytes

By default ozaru writes the whole file again. With `incremental=True`, it
keeps every byte of the original and adds only the pages it changed at the
end, as PDF editors do when they save:

```python
ozaru.rotate("signed.pdf", "signed.pdf", 90, incremental=True)
```

Use it for PDFs with a digital signature: a signature covers the bytes of the
file, so rewriting it breaks the signature, while adding to it doesn't. The
earlier version of the document stays in the file too.

A protected PDF and a damaged one can't be updated this way: ozaru raises a
`ValueError` and writes nothing. Save them without `incremental=True`.

Next: [extract and split pages](split-pages.md).
