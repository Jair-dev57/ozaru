# Sanitize a PDF

A PDF carries more than its pages: who made it and with what, scripts, attached files, bookmarks, comments.
`ozaru.sanitize` takes that out before you share the file, and **the pages look exactly the same**.

```python
import ozaru

report = ozaru.sanitize("report.pdf", "report-clean.pdf")
print(report)   # SanitizeReport(info_entries=5, metadata_streams=38, private_data=68, scripts=0, ...)
```

By default it takes out what doesn't change how the document looks or works:

- **Metadata**: the document information (title, author, dates, creator, producer...), the XMP
  metadata, the private data applications leave (`PieceInfo`, thumbnails) and the metadata of every
  page, image and drawing. Keep some entries with `keep_info=["Title"]`.
- **Scripts**: JavaScript, actions that launch files, send forms or play media, the actions that run when
  something happens in the document, a page, a field or a link, XFA forms and media annotations. Links to web
  addresses and to pages are kept.
- **Attachments**: attached files and the annotations that hold a file.

Other things change the document, so you ask for them: `bookmarks=True`, `comments=True` (notes, highlights,
shapes and stamps; links and form fields stay), `form_values=True` (the fields are emptied and drawn empty) and
`structure=True` (the tags of tagged files, which assistive software uses). `everything=True` asks for all of
them. To leave something out of the default, turn it off: `ozaru.sanitize(..., scripts=False)`.

From the terminal: `uvx ozaru sanitize report.pdf -o clean.pdf [--keep-info Title] [--comments] [--everything]`.

## After a redaction

[`redact`](redact.md) takes text out of the pages but not out of the places a PDF keeps text besides them. Sanitize
the result to clean those:

```python
ozaru.redact_text("contract.pdf", "step1.pdf", "ID 1234567")
ozaru.sanitize("step1.pdf", "contract-redacted.pdf", everything=True)
```

## What it does not reach

Text in the content of the pages (that is `redact`), the names of the fonts, and the identifier of the file.
The saved file never has what was taken out, since objects nothing points at aren't written; for the same reason
sanitizing is never saved incrementally.

Next: [compare two PDFs](compare-pdfs.md).
