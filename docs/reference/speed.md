# Speed

The median time of one call, in milliseconds, on a laptop with an Intel Core
i5-11400H, with Python 3.14. The files are made by the script that measures them,
`crates/engine/benches/compare_libraries.py`: pages full of text, 38 lines of 14
words each, and pages that are photographs. Each library does the same work:
the text of every page, the words and their boxes, the page count, and page 1 as a
PNG image, opening the file each time. pypdf can't draw pages or give the boxes of
words, and its text of 1000 pages takes too long to measure here.

`extract_text`:

| File | ozaru 0.8.2 | PyMuPDF 2.1 | pypdf 7.1.1 |
|---|---|---|---|
| 10 pages of text | 12 ms | 18 ms | 93 ms |
| 200 pages of text | 238 ms | 324 ms | 2049 ms |
| 1000 pages of text | 1253 ms | 1648 ms | - |
| 12 pages of photographs | 0.61 ms | 3.18 ms | 7.32 ms |

`extract_words`:

| File | ozaru 0.8.2 | PyMuPDF 2.1 | pypdf 7.1.1 |
|---|---|---|---|
| 10 pages of text | 20 ms | 22 ms | - |
| 200 pages of text | 320 ms | 432 ms | - |
| 1000 pages of text | 1632 ms | 2142 ms | - |
| 12 pages of photographs | 0.63 ms | 2.72 ms | - |

`page_count`:

| File | ozaru 0.8.2 | PyMuPDF 2.1 | pypdf 7.1.1 |
|---|---|---|---|
| 10 pages of text | 0.17 ms | 0.22 ms | 3.11 ms |
| 200 pages of text | 3.53 ms | 0.61 ms | 65 ms |
| 1000 pages of text | 20 ms | 2.29 ms | 331 ms |
| 12 pages of photographs | 0.69 ms | 0.20 ms | 1.70 ms |

`render_page, page 1`:

| File | ozaru 0.8.2 | PyMuPDF 2.1 | pypdf 7.1.1 |
|---|---|---|---|
| 10 pages of text | 16 ms | 19 ms | - |
| 200 pages of text | 14 ms | 22 ms | - |
| 1000 pages of text | 25 ms | 35 ms | - |
| 12 pages of photographs | 48 ms | 104 ms | - |

## What to read in the numbers

- **Text and words:** ozaru is faster than PyMuPDF, from 1.3 to 1.5 times on pages full of
  text, and much faster on pages that are images, where there is nothing to read
  and ozaru sees it at once. pypdf takes about 8 times longer.
- **Drawing pages:** ozaru is 1.2 to 2 times faster than PyMuPDF, and 2 times on
  photographs, which it decodes at a fraction of their size when they are drawn
  small.
- **Counting pages:** PyMuPDF is faster, from 3 to 9 times on files of hundreds of
  pages. ozaru reads every page, so a file with damaged pages is counted right,
  and pypdf, which does too, is 16 to 18 times slower than ozaru.

To measure on your machine, run the script: `uv run --with pypdf --with pymupdf
--with pillow --with numpy --with ozaru python crates/engine/benches/compare_libraries.py`.

The bars of the README are the same measurements, drawn by `speed_chart.py`.
