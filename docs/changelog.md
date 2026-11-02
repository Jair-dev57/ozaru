# Changelog

## Unreleased

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
