# Engine test corpus

Every PDF here was made by `generate.py`, from scratch, for this project, and is
under the project's MIT license. The only third-party content is in the
`fonts-*` files: subsets of the fonts their text is written in, embedded as any
document written with those fonts embeds them. See [Fonts](#fonts). It lives in
the engine's tests, not at the root of the repository, so it isn't packaged in
the `ozaru` source distribution.

`compare.py` reads these files, and the other PDFs of the repository, with
ozaru 0.1.7 (pdf_oxide), pypdf and Poppler's `pdfinfo`, and writes what they
find to `expected.tsv` (page counts) and `pages.tsv` (media box, crop box and
rotation of the first 50 pages, from pypdf) and, with pdfminer, `tounicode.tsv`
(the ToUnicode CMap of each font). `tests/corpus.rs` checks the engine
against both files.

The user password of the encrypted files is `user`, when they have one, except
for the three `password-*` files, whose passwords are in the table. The owner
password is always `owner`.

| File | Made with | What it covers |
|---|---|---|
| `one-page.pdf` | ReportLab | The smallest ordinary file. |
| `many-pages.pdf` | ReportLab | 500 pages in a flat page tree. |
| `many-pages-gs.pdf` | Ghostscript, from `many-pages.pdf` | The same pages, as another writer arranges them. |
| `sizes-and-rotations.pdf` | ReportLab | A4, Letter, landscape and A5 pages, rotated 0, 90, 180 and 270 degrees. |
| `unicode-info.pdf` | ReportLab | A title and author with accents and symbols, as UTF-16 text strings. |
| `deep-tree.pdf` | `generate.py`'s writer | A chain of 40 page tree nodes, with pages at depths 1, 20 and 40. |
| `balanced-tree.pdf` | `generate.py`'s writer | 1000 pages in nodes of 10, three levels deep, as large writers make them. |
| `balanced-tree-compressed.pdf` | qpdf, from `balanced-tree.pdf` | The same tree in object streams. |
| `inherited-attributes.pdf` | `generate.py`'s writer | `MediaBox`, `Rotate` and `Resources` inherited from page tree nodes, and replaced by some pages. |
| `incremental-updates.pdf` | `generate.py`'s writer | Two incremental updates: one adds a page, the next removes another and frees its object. |
| `pdf-1.0.pdf` | `generate.py`'s writer | A PDF 1.0 file. |
| `pdf-1.2.pdf` | Ghostscript, from `sizes-and-rotations.pdf` | A PDF 1.2 file. Ghostscript applies the rotations to the content. |
| `pdf-2.0.pdf` | Ghostscript, from `sizes-and-rotations.pdf` | A PDF 2.0 file. |
| `object-streams.pdf` | qpdf, from `sizes-and-rotations.pdf` | Objects in object streams and a cross-reference stream. |
| `linearized.pdf` | qpdf, from `sizes-and-rotations.pdf` | A linearized ("fast web view") file. |
| `aes-128.pdf` | qpdf, from `sizes-and-rotations.pdf` | AES-128, revision 4, with an empty user password. |
| `aes-256.pdf` | qpdf, from `sizes-and-rotations.pdf` | AES-256, revision 6, with an empty user password. |
| `aes-256-user-password.pdf` | qpdf, from `sizes-and-rotations.pdf` | AES-256 with the user password `user`, and the page tree in an encrypted object stream. |
| `aes-256-password-ix.pdf` | qpdf, from `one-page.pdf` | AES-256 with the user password `IX`, which the Roman numeral `Ⅸ` gives after SASLprep. |
| `aes-256-password-space.pdf` | qpdf, from `one-page.pdf` | AES-256 with the user password `a b`, which `a`, a no-break space and `b` give after SASLprep. |
| `aes-128-password-accent.pdf` | qpdf, from `one-page.pdf` | AES-128, revision 4, with the user password `año`, encoded in a single byte per character. |
| `fonts-reportlab-ttf.pdf` | ReportLab | DejaVu Sans and Serif as TrueType subsets with ToUnicode CMaps, and text in Spanish, Greek and Russian, with typographic symbols. |
| `fonts-cairo-latin.pdf` | cairo | The same text, in a CID font (Type 0) and TrueType subsets, as cairo writes them. |
| `fonts-cairo-cjk.pdf` | cairo | The same text and Chinese, Japanese and Korean, in Droid Sans Fallback. |
| `fonts-cairo-rtl.pdf` | cairo | The same text and Hebrew and Arabic, written right to left. |
| `fonts-gs.pdf` | Ghostscript, from `fonts-reportlab-ttf.pdf` | The TrueType fonts as Ghostscript rewrites them, with its own ToUnicode CMaps. |
| `fonts-poppler.pdf` | Poppler's `pdftocairo`, from `fonts-reportlab-ttf.pdf` | The same fonts, some turned into CID fonts. |
| `fonts-standard-14.pdf` | ReportLab | Text with accents and symbols in the standard 14 fonts, which are never embedded, including Symbol and ZapfDingbats. |
| `fonts-no-tounicode.pdf` | `generate.py`'s writer and fontTools | No ToUnicode CMaps: a TrueType CIDFont whose program has a Unicode cmap, and a symbolic TrueType font without `/Encoding` whose codes go through a (3, 0) cmap to named glyphs. `no-tounicode.tsv` has the real text of each code. |

## Fonts

The `fonts-*` files embed subsets of these fonts, which allow it:

- **DejaVu Sans and DejaVu Serif**, under the Bitstream Vera Fonts license, with
  the DejaVu changes in the public domain: <https://dejavu-fonts.github.io/License.html>.
- **Droid Sans Fallback**, under the Apache License 2.0.

`fonts-standard-14.pdf` embeds no font: the standard 14 fonts are named, and
each reader supplies them.

## Regenerating

With Python 3.10+, reportlab, pikepdf, pycairo and fonttools, Ghostscript 10 as `gs`,
Poppler's `pdftocairo`, and the DejaVu and Droid Sans Fallback fonts installed:

```bash
pip install reportlab pikepdf pycairo fonttools
python generate.py
pip install ozaru==0.1.7 pypdf cryptography pdfminer.six   # and Poppler's pdfinfo
python compare.py
```

ReportLab, `generate.py`'s writer and qpdf write the same bytes each time.
Ghostscript stamps the current date, and AES uses random initialization vectors,
so those files change. `compare.py` exits with 1 if the engine disagrees with
references that agree with each other.

The files were made with ReportLab 5.1.0, pikepdf 10.17.0 (qpdf 12.4.2),
Ghostscript 10.06.0, pycairo 1.29.2, fontTools 4.62.0 and pdftocairo 26.01.0, and compared with
pypdf 7.0.0, pdfinfo 26.01.0 and pdfminer.six 20260107.
