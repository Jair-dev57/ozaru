# Limitations

ozaru is in early development, so its API may change between releases. These are
the current limitations:

| Limitation | What to do for now |
|------------|--------------------|
| No OCR, so scanned documents may return little or no text. | Run an OCR tool first. |
| Only files on disk can be read, not bytes in memory. | Save the data to a file first. |
| `extract_images` doesn't give yet JBIG2 scans that use halftones coded with MMR, or symbols refined in Huffman coded dictionaries, both rare: they come with their box and size, without `data`. CMYK colors are turned into RGB without a color profile, and JPEG 2000 images come without their transparency mask. JPEG images with arithmetic coding or 12-bit samples, which are very rare, are given as they are, even if they have a mask. | Coming in later versions. |
| `render_page` doesn't draw mesh gradients (shading types 4 to 7), or images in JPEG 2000. Fonts that aren't embedded are drawn with a font of similar shape. | Coming in later versions. |
| `fill_form` draws text in the field's own font only with Western European characters (others show as `?`), doesn't format numbers or dates by their scripts, doesn't sign, and doesn't recalculate fields. | Coming in later versions. |
| `extract_pages`, `split` and `merge` leave out the bookmarks, forms, named links, page labels and accessibility tags of the document. | Coming in later versions. |
| `rotate`, `extract_pages`, `split` and `merge` save a protected PDF without its password, and write the whole file again, which breaks digital signatures and the fast web view of a PDF. | For `rotate`, pass `incremental=True` to keep the original bytes. |
| `incremental=True` doesn't work on protected or damaged PDFs. | Coming with passwords and permissions, in a later version. |
| Columns are read one after the other when the PDF paints them one after the other or row by row, but not in other orders. | Most documents paint their columns one after another, so their text keeps its order. |

Is one of these blocking you? Tell us in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).
