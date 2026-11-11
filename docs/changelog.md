# Changelog

## Unreleased

### OZARU ENGINE

- Phase 10 starts: the content of a page gives its images, image XObjects and
  inline images, each with the matrix that places it, so its box on the page.
  `images::read_image` reads what an image is, its size, bits, color space and
  whether it is a stencil mask, with the abbreviations of inline images written
  out, and its data, with every filter decoded but the image filter at the end,
  such as `/DCTDecode`, which is given with its parameters.
- `images::image_file` gives an image as a file: JPEG and JPEG 2000 images as
  the PDF has them, byte for byte, and images of gray or RGB pixels of 8 bits
  as PNG files, written by the engine. `images::document_images` gives every
  image of every page with its box and its file, reading an image painted
  several times once.

## 0.3.3 (2026-11-09)

- `extract_blocks` returns the blocks of text of a PDF, its paragraphs,
  headings and the pieces of a page that stand apart, each with its page, its
  text and its box. Every `Word` has the `block` it is in.

- `extract_text` and `extract_words` read columns painted row by row, the
  first line of each column and then the second, one column after the other,
  as a person reads them. Before, each row of the columns came as one line.
  Tables, whose cells are short or span several lines, stay row by row.
- Arabic and Hebrew come in reading order, from right to left, however the PDF
  paints their letters, and the numbers and Latin words inside them keep
  theirs.

### OZARU ENGINE

- Blocks: `text::page_blocks` and `text::document_blocks` put the lines of a
  page together into paragraphs, headings and cells, each with its box. A
  block ends where a paragraph does, after a heading in another size, where the
  text turns, and where the next line is beside the block instead of under it.
- Columns painted row by row are found by the gutter between them, the space
  that runs down through their rows, and read one after the other:
  `text::read_columns`. Only when every row has text on both sides but the last
  ones, and the text reads as prose.
- Lines with letters of scripts written right to left are read as they are
  seen, from left to right, and put in reading order with the Unicode
  Bidirectional Algorithm, with the `unicode-bidi` crate. The corpus gets
  `fonts-pango-rtl.pdf`, written by Pango.
- Vertical text is checked as lines, words and blocks: each column is a line
  read from the top down, with boxes down the page, and a column further left
  than usual starts a paragraph.

## 0.3.2 (2026-11-09)

- `extract_words` returns every word of a PDF with its page, its line and its
  box, in points from the top left corner of the page, as PyMuPDF measures
  them. See [Find words on the page](tutorial/find-words.md).
- Bullets that PDFs write with the unused codes of WinAnsiEncoding, as
  ReportLab does, are read as `•`, as the standard says. Before they were
  lost, and the text after them was placed too far left.
- Text that a PDF paints further back on the same line, as some footers do,
  is separated by a space from the text before it. Before, the two were
  joined into one word.

### OZARU ENGINE

- Phase 9 starts: each character has a box on the page, as wide as its glyph
  and as tall as its font, from the `/Ascent` and `/Descent` of the font, the
  Adobe metrics of the standard fonts, or the `/FontBBox`. Boxes turn and scale
  with the text, and run down the page in vertical writing.
- Words, with their boxes, line by line: `text::page_words` and
  `text::document_words`. A word ends at a space, painted or found in a gap.
  They are compared with PyMuPDF's on the corpus: the same words, with the
  same box for embedded fonts.

## 0.3.1 (2026-11-09)

- `extract_text` leaves out the spaces at the start of each line, as it
  already did at the end. Some PDFs paint them to indent a line.
- `page_count` and `extract_text` are faster on long documents: pages that
  share their fonts and other resources no longer copy them when the PDF is
  opened. `page_count` takes about 10% less on a PDF of 500 pages, and
  `extract_text` about 25% less.

## 0.3.0 (2026-11-09)

- `extract_text` reads every PDF with OZARU ENGINE, ozaru's own engine, and
  `pdf_oxide` is gone: ozaru no longer has any other PDF library inside. The
  text is the same as in 0.2.5 on almost every PDF we test with, but spaces
  and line breaks may change in some files.
- `extract_text` reads PDFs whose page tree is destroyed, as `page_count` and
  `page_info` already did. Before, they raised `ValueError`.
- Pages without content are empty without a `PageWarning`. Before, some of
  them made the reader crash and were reported as damaged.
- Text outside the visible area of a page, which no viewer shows, is left
  out, and `extract_text` uses the text a PDF gives for ligatures and words
  drawn as shapes, its `/ActualText`.
- A `PageWarning` also reports a page whose content could only be read in
  part.

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
- Phase 7 starts: content streams, the programs that paint each page, are read
  as operators and their operands, including inline images, and syntax that
  can't be read is skipped instead of stopping the page.
- The graphics state is tracked: `q` and `Q` save and restore it, and `cm`
  moves, scales and rotates what is painted after it.
- The text state is tracked too: the font and size, character and word
  spacing, horizontal scaling, leading, rise and rendering mode, and the text
  matrices that `Td`, `TD`, `T*` and `Tm` move.
- Content streams run in the engine: `Tj`, `TJ`, `'` and `"` give each glyph
  with its position on the page, its font and size, its text and its width,
  and a page with several content streams is read as one.
- Vertical text, as in Japanese or Chinese written top to bottom, is placed
  with the vertical metrics of its font, `/W2` and `/DW2`.
- Form XObjects, the reusable pieces of content that headers, footers, logos
  and stamps are often made of, are painted with their own matrix and
  resources. Forms that paint themselves, or are nested too deeply, are
  skipped instead of hanging the engine.
- Marked content is followed: each glyph knows the `BMC`/`BDC` sequences it
  is in, with their tags, `/MCID` and `/ActualText`, the text that a ligature
  or a drawn word stands for.
- Pages whose content is damaged, cut short, in streams that can't be decoded
  or locked by a password give what can be read, and every page of the
  repository's PDFs runs. A new fuzz target, `content`, runs random content
  streams, and the `document` target runs the content of its pages too.
- Phase 7 is complete: every glyph of the test corpus is placed with the font,
  size and position that pdfminer gives it, wherever pdfminer follows the PDF
  standard. The review is in `design/engine.md`.
- Phase 8 starts: the glyphs of a page become text, with the `/ActualText`
  of marked content in place of the glyphs it stands for, and glyphs painted
  twice to look bold kept once.
- The characters of a page are put together into lines: those on the same
  baseline are one line, superscripts and subscripts included, and so are the
  cells of a table row.
- Spaces are found where a PDF leaves a gap between words instead of painting
  a space, as many writers do with `TJ`, while kerning stays inside words.
- The text of each page and of a whole document is put together: lines in the
  order they were painted, an empty line between paragraphs, pages separated
  by a form feed as `extract_text` does, and text outside the visible area of
  the page left out, as viewers don't show it.
- The text the engine extracts is compared with ozaru 0.1.7, pypdf and PyMuPDF
  on every PDF of the repository: 39 of 41 are 98% alike or more once spaces
  are normalized, and the other two are where the engine reads the text
  right and the others don't.
- Phase 8 is complete: `extract_text` runs on the engine. The review is in
  `design/engine.md`, and the README compares its speed with ozaru 0.1.7,
  pypdf and PyMuPDF.

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
