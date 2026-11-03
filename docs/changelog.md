# Changelog

## Unreleased

### OZARU ENGINE

ozaru is getting its own PDF engine, written in Rust, to replace `pdf_oxide`. It
isn't used by `extract_text` or `page_count` yet: they will switch to it one at a
time, starting with `page_count` in 0.2.0. Its plan is in
[design/engine.md](https://github.com/Jair-dev57/ozaru/blob/main/design/engine.md).

- **Phase 1, syntax:** reads the objects a PDF is made of: numbers, strings,
  names, arrays and dictionaries.
- **Phase 2, indirect objects and streams:** reads numbered objects
  (`4 0 obj ... endobj`) and the data of streams.
- **Phase 3, document loading:** finds every object of a file through its
  cross-reference data, including incremental updates and compressed object
  streams, decompresses Flate data, and repairs files whose cross-reference data
  is broken.
- **Phase 4, filters and decryption:** decodes every compression filter used for
  text (Flate, LZW, RunLength, ASCIIHex and ASCII85) and decrypts encrypted PDFs:
  RC4 and AES-128 and AES-256, from the oldest revision of the standard to the
  newest. Files that open without a password are decrypted on the fly.

## 0.1.6 (2026-11-02)

- Raise the same exceptions as Python's `open()` for file problems:
  `PermissionError` and `IsADirectoryError` instead of `ValueError`, plus
  `NotADirectoryError` and `OSError`. They include `errno` and `filename`.
- Raise `ValueError` for a path that contains a null byte, instead of
  `FileNotFoundError`.
- Change the `FileNotFoundError` message to match Python's: `[Errno 2] No such
  file or directory: 'report.pdf'`.

## 0.1.5 (2026-11-02)

- Keep the text of the other pages when one page can't be read, and emit an
  `ozaru.PageWarning` with the page number and the reason. The page used to be
  dropped silently, and a crash in one page lost the whole document.

## 0.1.4 (2026-11-01)

- Raise `ValueError` in `extract_text` for PDFs that need a password, instead of
  silently returning an empty string.
- Create a GitHub release for each new version.

## 0.1.3 (2026-11-01)

- Release the GIL while reading a PDF, so other Python threads keep running and
  several PDFs can be read in parallel.

## 0.1.2 (2026-11-01)

- Fix a bug where a damaged PDF could make later PDFs silently lose text in the
  same process.
- Raise `ValueError` for damaged PDFs that used to raise `PanicException`, which
  `except Exception` can't catch.

## 0.1.1 (2026-11-01)

- Accept `pathlib.Path` and other path-like objects in `extract_text` and
  `page_count`.
- Translate error messages to English.
- Rewrite the README in English.
- Add this documentation site.

## 0.1.0 (2026-11-01)

First release.

- Add `extract_text` to get the text of a PDF.
- Add `page_count` to get the number of pages of a PDF.
