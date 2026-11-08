# Read the metadata

`ozaru.metadata` returns the document information of a PDF: its title, author,
subject, keywords, the programs that made it, and its dates.

```python
--8<-- "docs_src/metadata.py"
```

Output:

```text
--8<-- "docs_src/metadata.out"
```

Each attribute is `None` when the PDF doesn't have it, so check before you use
one:

```python
info = ozaru.metadata("report.pdf")
if info.title is None:
    print("This PDF has no title")
```

The dates are `datetime` objects. When the PDF says its time zone, they are
*aware* and you can compare them with any other date. When it doesn't, they are
*naive*, in the time zone the PDF was made in.

A PDF protected with a password raises `ValueError`, since its metadata is
encrypted too.

Next: [read page by page](read-page-by-page.md).
