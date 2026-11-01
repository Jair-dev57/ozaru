# Limitations

ozaru is in early development, so its API may change between releases. These are
the current limitations:

| Limitation | What to do for now |
|------------|--------------------|
| No OCR, so scanned documents may return little or no text. | Run an OCR tool first. |
| `path` must be a `str`. `pathlib.Path` is not supported yet. | Convert it with `str(path)`. |
| Only files on disk can be read, not bytes in memory. | Save the data to a file first. |
| Error messages are in Spanish. | Rely on the exception type, not the message. |

Is one of these blocking you? Tell us in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).
