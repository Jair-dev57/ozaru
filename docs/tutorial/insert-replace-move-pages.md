# Insert, replace and move pages

Pages are counted from 1, and every function saves the result to another file.

## Insert pages of another PDF

```python
import ozaru

# all the pages of cover.pdf, at the start
ozaru.insert_pages("report.pdf", "report-new.pdf", "cover.pdf", at=1)

# pages 2 and 5 of appendix.pdf, before page 10 of the report
ozaru.insert_pages("report.pdf", "report-new.pdf", "appendix.pdf", at=10, pages=[2, 5])

# at the end (the default)
ozaru.insert_pages("report.pdf", "report-new.pdf", "appendix.pdf")
```

`at` is the page the new ones go before; one past the last page is the end. The pages of the report keep
their bookmarks, forms and destinations, which follow them to their new places. Only the page labels
(`i, ii, iii...`), which count pages by their place, are removed if the new pages come before others. The
inserted pages bring everything they use (fonts, images, annotations); the fields of their forms stay as
drawings and are not fields of the report.

## Replace pages

```python
# page 3 of the report is replaced by page 1 of fixed.pdf
ozaru.replace_pages("report.pdf", "report-new.pdf", "fixed.pdf", at=3, pages=[1])

# pages 3 to 5 are replaced by all the pages of fixed.pdf (any number)
ozaru.replace_pages("report.pdf", "report-new.pdf", "fixed.pdf", at=3, count=3)
```

`count` pages from `at` go (as many as are put in by default) and `pages` of the other PDF come in their
place. The pages that go are **not in the saved file**, nor what only they used. Since bookmarks and
destinations may point at them, those of the PDF are removed. With `count=0` it only inserts.

## Move pages

```python
ozaru.move_pages("report.pdf", "report-new.pdf", [7], to=1)        # page 7 to the start
ozaru.move_pages("report.pdf", "report-new.pdf", [1, 2])           # pages 1 and 2 to the end
```

`pages` go before page `to` (the end by default), in the order given, and the others keep theirs. Nothing
is left out, so bookmarks and destinations follow their pages; only the page labels are removed.

All of them take `password=` for the PDF and `source_password=` for the other (the same one by default).
They are also methods of `Document`, and commands: `uvx ozaru insert report.pdf cover.pdf -o new.pdf --at 1`,
`ozaru replace report.pdf fixed.pdf -o new.pdf --at 3 --source-pages 1` and
`ozaru move report.pdf 7 -o new.pdf --to 1`.

Next: [bookmarks and attachments](bookmarks-and-attachments.md).
