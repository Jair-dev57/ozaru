# Limitations

ozaru is in early development, so its API may change between releases. These are
the current limitations:

| Limitation | What to do for now |
|------------|--------------------|
| No OCR, so scanned documents may return little or no text. | Run an OCR tool first. |
| Only files on disk can be read, not bytes in memory. | Save the data to a file first. |
| Text can't be extracted from PDFs that need a password to open. `extract_text` raises `ValueError`. | Remove the password with another tool first. |

Is one of these blocking you? Tell us in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).
