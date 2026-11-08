# Changelog

## Unreleased

### OZARU ENGINE

- Phase 6 starts: fonts map character codes to glyph names with the standard
  encodings of PDF (Standard, MacRoman, WinAnsi, Symbol and ZapfDingbats) and
  the `/Differences` of each font.
- Glyph names map to Unicode with the Adobe Glyph List, including names such as
  `uni00F1`, `f_f_i` and `a.sc`.
- The wheels include `THIRD_PARTY_NOTICES.md`, with the notice of the Adobe
  Glyph List that the engine uses.
- ToUnicode CMaps are read, the tables that map character codes to text in
  most PDFs with embedded fonts, including codes that map to several characters.
- Simple fonts (Type 1, TrueType and Type 3) give the text and width of each
  character code.
- The standard 14 fonts, such as Helvetica or Times, have their glyph widths
  from Adobe's metrics when a PDF doesn't give them, also under the names that
  writers use for them, like Arial or Times New Roman.
- Composite fonts (Type 0), the ones used for Chinese, Japanese, Korean and most
  embedded fonts of modern PDFs, give the text and width of each code, with the
  Identity and Unicode CMaps and the CMaps embedded in the file.
- Fonts whose PDF doesn't say what their characters are get their text from the
  embedded font: the built-in encoding of Type 1 fonts, as in many PDFs made
  with LaTeX, and the tables of TrueType fonts without a ToUnicode CMap.
- Phase 6 is complete: the 48 fonts of the test corpus are read without
  problems, and they match pdfminer wherever pdfminer follows the PDF standard.
  The review is in `design/engine.md`.

## 0.2.5 (2026-11-08)

- Passwords with special characters work as they are typed: they are normalized
  with SASLprep, as the standard says for AES-256 PDFs, so that, for example, a
  no-break space is a space. Before, only `extract_text` normalized them.
- `page_count` and `page_info` read PDFs whose page tree is destroyed, which
  pypdf, Poppler and ozaru 0.2.4 can't open: the pages are rebuilt from the file.
  `extract_text` can't read them yet.
- `Metadata` and `PageInfo` can be compared with `==`, and used in sets and as
  dictionary keys. Before, two equal ones compared as different.

### OZARU ENGINE

- When the root of the page tree is destroyed, the pages are rebuilt from the
  page objects of the file, in the order they appear in it, keeping what they
  inherit from the page tree nodes that survive.

## 0.2.4 (2026-11-08)

- Add a `password` keyword argument to every function, for PDFs protected with a
  password. Either the user or the owner password works.
- Raise `ozaru.PasswordError`, a new subclass of `ValueError`, when a PDF needs
  a password and none was given, or the password is incorrect. Before, a missing
  password raised a plain `ValueError`.

### OZARU ENGINE

- The owner password of an encrypted PDF opens it too, like its user password,
  as the standard allows. Before, only the user password did.

## 0.2.3 (2026-11-08)

- Add `ozaru.page_info`, which returns the size and rotation of every page of a
  PDF as a list of `ozaru.PageInfo`. `width` and `height` are in points, as a
  viewer shows the page, rotated, and `media_box` and `crop_box` give the boxes
  the PDF defines. A page too damaged to read emits a `PageWarning`. It reads
  with OZARU ENGINE and, like `page_count`, works on most PDFs protected with a
  password.

## 0.2.2 (2026-11-08)

- Add `ozaru.metadata`, which returns the title, author, subject, keywords,
  creator, producer and dates of a PDF as an `ozaru.Metadata` object. The dates
  are `datetime` objects, with their time zone when the PDF gives one. It reads
  with OZARU ENGINE, also when the pages of the PDF are too damaged to count, and
  raises `ValueError` for PDFs protected with a password.

## 0.2.1 (2026-11-06)

- `page_count` is faster than in 0.2.0: about 17% on small files and 24 to 38%
  on files with hundreds of pages. It is many times faster than pypdf, and
  faster than ozaru 0.1.7 on small files, but about three to five times slower
  than 0.1.7 on files with hundreds of pages, because it reads every page
  instead of trusting the declared page count.
- Test the Apple Silicon wheel with Python 3.10 only, since GitHub has few of
  those machines and a test that waits too long for one stopped two releases.

## 0.2.0 (2026-11-06)

- `page_count` now uses OZARU ENGINE, ozaru's own PDF engine, instead of
  `pdf_oxide`. It counts the same pages on every PDF we test with, and it walks
  the page tree instead of trusting the page count a file declares, which is
  often wrong in damaged files. `extract_text` still uses `pdf_oxide`, until 0.3.0.
- The messages of the `ValueError` that `page_count` raises have changed. They
  now say where the file is damaged, such as `Could not open the PDF: missing
  startxref at byte 6533`. The exception types are the same.
- `page_count` raises `ValueError` with "protected with a password" for the rare
  encrypted PDFs whose pages can't be counted without the password.

### OZARU ENGINE

- **Phase 5, pages:** finds the document catalog, the entry point to the pages,
  also in files where it is missing or damaged, and lists the pages in order,
  even from page trees that are very deep, loop back on themselves or have a
  wrong page count. Each page gets its size, visible area, rotation and
  resources, including the ones it inherits from the page tree. The document
  information (title, author, dates) is read, including in encrypted files. Its
  page counts match ozaru 0.1.7, pypdf and Poppler on every test PDF, including
  new ones with deep page trees, incremental updates, AES encryption and PDF 1.0
  to 2.0.

## 0.1.7 (2026-11-06)

- Stop printing Rust's `panicked at` messages to the terminal when a damaged page
  crashes the PDF library. The crash was already handled, and is still reported
  as a `PageWarning` or a `ValueError`.
- Test the wheels for Linux, macOS and Windows on their own platforms, with
  Python 3.10 and 3.14, before every release.

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
