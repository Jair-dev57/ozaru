# Extract images

`ozaru.extract_images` returns every image of a PDF as an `Image`: the page it
is on, its box, its size in pixels and the image itself, as a file you can save.

The examples use the [sample brochure](../assets/brochure.pdf){ download }, a
page with a photo and a logo.

## Save every image

```python
--8<-- "docs_src/extract_images.py"
```

Output:

```text
--8<-- "docs_src/extract_images.out"
```

`format` tells you what kind of file `data` holds, so you know which extension to
give it:

| `format` | What it is |
|----------|------------|
| `"jpeg"` | A JPEG photo, exactly as it is stored in the PDF, without losing quality. |
| `"jp2"`  | A JPEG 2000 image, also as it is stored in the PDF. |
| `"png"`  | An image ozaru writes as a PNG file from its pixels. |

`save(path)` writes `data` to a file, and `data` gives you the same bytes, for
example to send them somewhere without saving them first.

## Where an image is

The box of an image, `x0`, `y0`, `x1` and `y1`, is measured as the box of a word
is: in points from the top left corner of the page, before the page is rotated.
An image painted several times, like a logo on every page, comes once for each
time it is painted.

`width` and `height` are the size of the image in pixels, which has nothing to do
with the size it has on the page: a large photo can be painted small.

## Images ozaru can't give yet

Images with transparency, like a logo cut out of its background, come as PNG
files with an alpha channel, so the background stays transparent. Black and
white scans in fax encoding come as PNG files too.

Some images come with their page, box and size, but with `format` and `data`
set to `None`: ozaru finds them but can't write them as a file yet. That is the
case of scans in JBIG2, and of images in special inks, `/Separation` and
`/DeviceN`.
`save` raises `ValueError` for them. They are coming in the next versions; see
[Limitations](../reference/limitations.md).

Next: [count pages](count-pages.md).
