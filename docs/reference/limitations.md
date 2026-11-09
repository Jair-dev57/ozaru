# Limitations

ozaru is in early development, so its API may change between releases. These are
the current limitations:

| Limitation | What to do for now |
|------------|--------------------|
| No OCR, so scanned documents may return little or no text. | Run an OCR tool first. |
| Only files on disk can be read, not bytes in memory. | Save the data to a file first. |
| Text comes in the order the PDF paints it, so pages with several columns, or written right to left as in Arabic and Hebrew, may not come in reading order. | Most documents paint their columns one after another, so their text keeps its order. |

Is one of these blocking you? Tell us in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).
