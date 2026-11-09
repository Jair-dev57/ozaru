# Limitations

ozaru is in early development, so its API may change between releases. These are
the current limitations:

| Limitation | What to do for now |
|------------|--------------------|
| No OCR, so scanned documents may return little or no text. | Run an OCR tool first. |
| Only files on disk can be read, not bytes in memory. | Save the data to a file first. |
| Columns are read one after the other when the PDF paints them one after the other or row by row, but not in other orders. | Most documents paint their columns one after another, so their text keeps its order. |

Is one of these blocking you? Tell us in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).
