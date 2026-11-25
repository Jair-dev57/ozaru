# Edit the information and the pages

The functions that change a PDF save the result to another file, and leave the original as it was.

```python
import ozaru

ozaru.set_metadata("report.pdf", "report-2.pdf", {"title": "Quarterly report", "author": "Ana"})

ozaru.set_bookmarks("report-2.pdf", "report-3.pdf", [
    (1, "Summary", 1),
    (1, "Sales", 2),
    (2, "By region", 3),
    (1, "Source", "https://example.com/data"),
])

ozaru.add_attachment("report-3.pdf", "report-4.pdf", "data", "sales.csv")
ozaru.add_blank_page("report-4.pdf", "report-5.pdf", position=1)
ozaru.crop("report-5.pdf", "report-6.pdf", (0, 0, 595, 400), pages=[2])
ozaru.delete_pages("report-6.pdf", "final.pdf", [4])
```

With a `Document` you leave out the PDF and its password: `pdf.set_metadata("out.pdf", {...})`.

Every call reads the file again and writes a whole new one. To change several things at once,
chain the files as above, or give `incremental=True` to add the changes at the end of the file
instead of writing it again.

For the text, bookmarks and page labels it writes, ozaru takes care of the encoding: accents and
other alphabets work.

Next: [protected PDFs](protected-pdfs.md).
