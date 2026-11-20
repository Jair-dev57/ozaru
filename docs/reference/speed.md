# Speed

The median time of one call, in milliseconds, on a laptop with an Intel Core
i5-11400H. pypdf and PyMuPDF are timed doing the same work: the text of every
page, and the page count.

`extract_text`:

| File | ozaru | ozaru 0.1.7 | PyMuPDF 2.1 | pypdf 7.0 |
|---|---|---|---|---|
| 3 pages | 0.25 ms | 0.64 ms | 2.0 ms | 3.3 ms |
| An invoice, 3 pages | 0.95 ms | 2.1 ms | 2.7 ms | 7.7 ms |
| Chinese, Japanese and Korean, 1 page | 0.46 ms | 2.1 ms | 1.6 ms | 8.5 ms |
| 500 pages | 9.5 ms | 38 ms | 56 ms | 347 ms |
| 1000 pages | 7.1 ms | 47 ms | 107 ms | 419 ms |

`page_count`:

| File | ozaru | ozaru 0.1.7 | PyMuPDF 2.1 | pypdf 7.0 |
|---|---|---|---|---|
| 3 pages | 0.057 ms | 0.080 ms | 0.16 ms | 0.53 ms |
| 500 pages | 1.9 ms | 0.77 ms | 0.30 ms | 55 ms |
| 1000 pages | 1.9 ms | 0.75 ms | 1.8 ms | 57 ms |

`render_page` at scale 1, as PNG bytes, opening the file each time:

| File | ozaru | PyMuPDF 2.1 |
|---|---|---|
| An invoice, 1 page | 6.9 ms | 15 ms |
| A photo cover (1080 x 1920 pixels) | 217 ms | 217 ms |
| A smaller photo cover | 16.5 ms | 22 ms |
| A page of text of 2,380 glyphs | 41 ms | 41 ms |

`page_count` reads every page of the PDF, so that damaged files are counted
right, which takes longer on files with hundreds of pages. To measure on your
machine, run `crates/engine/benches/compare_libraries.py`.

The bars on the README are the same measurements.

