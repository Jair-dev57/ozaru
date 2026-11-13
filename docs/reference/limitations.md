# Limitations

ozaru is in early development, so its API may change between releases. These are
the current limitations:

| Limitation | What to do for now |
|------------|--------------------|
| No OCR, so scanned documents may return little or no text. | Run an OCR tool first. |
| Only files on disk can be read, not bytes in memory. | Save the data to a file first. |
| `extract_images` doesn't give yet scans in JBIG2, or images in special inks (`/Separation`, `/DeviceN`): they come with their box and size, without `data`. CMYK colors are turned into RGB without a color profile, and JPEG images come without their transparency mask. | Coming in later versions. |
| `extract_pages`, `split` and `merge` leave out the bookmarks, forms, named links, page labels and accessibility tags of the document. | Coming in later versions. |
| `rotate`, `extract_pages`, `split` and `merge` save a protected PDF without its password, and write the whole file again, which breaks digital signatures and the fast web view of a PDF. | Coming in later versions: saving only the changes, at the end of the file. |
| Columns are read one after the other when the PDF paints them one after the other or row by row, but not in other orders. | Most documents paint their columns one after another, so their text keeps its order. |

Is one of these blocking you? Tell us in the
[issue tracker](https://github.com/Jair-dev57/ozaru/issues).
