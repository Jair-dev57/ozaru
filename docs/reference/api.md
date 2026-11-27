# API

ozaru has thirty-seven functions, the `Word`, `Block`, `Image`, `Metadata`, `PageInfo`, `Annotation` and `FormField` classes they return,
one exception and one warning class. Every function takes a PDF as its first argument: the path of a file (a `str` or a
`pathlib.Path`), its `bytes` (also a `bytearray` or a `memoryview`), a file opened in
binary mode (`open("file.pdf", "rb")`, `io.BytesIO`, an upload) or the `http://` or
`https://` URL of a PDF, which ozaru downloads. The functions that save a file
(`rotate`, `extract_pages`, `fill_form`, `encrypt`, `split` and `merge`) read their
input the same way and write to a path.

Every function releases Python's GIL while it reads, so you can call them from
several threads at the same time.

`PdfSource` stands for `str | os.PathLike[str] | bytes | bytearray | memoryview | BinaryIO`. `split` names the files of a PDF that isn't a file `part-1.pdf`, `part-2.pdf`... and `merge` says "the PDF from memory" in the message about a damaged one.

## `extract_text`

```python
ozaru.extract_text(path: PdfSource, *, password: str | None = None) -> str
```

Returns the text of the whole document. Pages are separated by a form feed
character (`\f`).

The text of each page is laid out like this:

- Lines come in the order the PDF paints them, which is the reading order of
  most documents, separated by `\n`. Spaces at the start and end of a line
  are left out.
- Columns of text come one after the other, from left to right, also when the
  PDF paints them row by row. Tables stay row by row.
- Arabic and Hebrew come in reading order, from right to left, with the
  numbers and Latin words inside them in theirs.
- Vertical text, as in Japanese and Chinese, comes column by column, each
  from the top down, in the order the PDF paints the columns: from right to
  left in most documents.
- An empty line separates paragraphs: lines further apart than the usual
  distance between the lines of the page.
- Spaces are added where the PDF leaves a gap between words instead of a
  space, and the pieces of a table row are one line, separated by spaces.
- Text outside the visible area of the page, which viewers don't show, is
  left out.
- Where the PDF says what some glyphs stand for, such as a ligature or a word
  drawn as shapes, that text is used.

A PDF whose list of pages is destroyed still gives its text: the pages are
found among the objects of the file.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`str`: the text of the document. A page that could not be read is an empty
string, and a page whose content is partly damaged gives the text that could
be read.

All the file errors are subclasses of `OSError` and have `errno` and `filename`
attributes, like the ones Python's `open()` raises.

**Warns**

`PageWarning`, once for each page that could not be read, or whose content
could only be read in part.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | The path does not exist.                                |
| `PermissionError`    | You don't have permission to read the file.             |
| `IsADirectoryError`  | The path is a folder, not a file.                       |
| `NotADirectoryError` | Part of the path is a file used as if it were a folder. |
| `OSError`            | The file could not be read for another reason.          |
| `PasswordError`      | The PDF is protected and no password was given, or the password is incorrect. |
| `ValueError`         | The file is not a valid PDF, it is too damaged to find its pages, or the path contains a null byte. |
| `TypeError`          | `path` is not a valid path type.                        |

## `extract_words`

```python
ozaru.extract_words(path: PdfSource, *, password: str | None = None) -> list[ozaru.Word]
```

Returns every word of the document, with the page and line it is on and its
box, in the order of `extract_text`: page by page, and line by line. See
[Find words on the page](../tutorial/find-words.md).

A word ends at a space, whether the PDF paints it or only leaves a gap, so
joining the words of a page with single spaces gives its text. Text outside
the visible area of a page is left out, as in `extract_text`.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`list[Word]`: the words of the document. A page that could not be read has no
words.

**Warns**

`PageWarning`, once for each page that could not be read, or whose content
could only be read in part, as `extract_text` does.

**Raises**

The same exceptions as `extract_text`.

## `Word`

```python
class ozaru.Word
```

A word of a PDF, returned by `extract_words`. Its attributes are read-only.

| Attribute | Type    | Description |
|-----------|---------|-------------|
| `page`    | `int`   | The number of its page, from 1 |
| `block`   | `int`   | The number of its block on the page, from 1, as `extract_blocks` numbers them |
| `line`    | `int`   | The number of its line on the page, from 1 |
| `text`    | `str`   | The word |
| `x0`      | `float` | The left side of its box |
| `y0`      | `float` | The top of its box |
| `x1`      | `float` | The right side of its box |
| `y1`      | `float` | The bottom of its box |
| `bbox`    | `tuple[float, float, float, float]` | `(x0, y0, x1, y1)` |

The box is in points, 1/72 of an inch, from the top left corner of the visible
area of the page, its crop box, before the page is rotated: `x` grows to the
right and `y` down. It is as wide as the word and as tall as its font, from the
font's descent to its ascent, as the PDF gives them or, for the standard fonts,
as Adobe's metrics do.

Two `Word` are equal when all their attributes are, and they can be used in
sets and as dictionary keys.

## `extract_blocks`

```python
ozaru.extract_blocks(path: PdfSource, *, password: str | None = None) -> list[ozaru.Block]
```

Returns every block of text of the document, with its page and its box, in the
order of `extract_text`. A block is a run of lines: a paragraph, a heading, or
a piece of the page that stands apart, like the name and the date of a header.
A block ends where `extract_text` leaves an empty line, after a heading in
another size, where the text turns, and where the next line is beside the
block instead of under it. See [Find words on the page](../tutorial/find-words.md#blocks).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`list[Block]`: the blocks of the document. A page that could not be read has
no blocks.

**Warns**

`PageWarning`, as `extract_text` does.

**Raises**

The same exceptions as `extract_text`.

## `Block`

```python
class ozaru.Block
```

A block of text of a PDF, returned by `extract_blocks`. Its attributes are
read-only.

| Attribute | Type    | Description |
|-----------|---------|-------------|
| `page`    | `int`   | The number of its page, from 1 |
| `number`  | `int`   | The number of the block on the page, from 1 |
| `text`    | `str`   | Its lines, separated by `\n` |
| `x0`      | `float` | The left side of its box |
| `y0`      | `float` | The top of its box |
| `x1`      | `float` | The right side of its box |
| `y1`      | `float` | The bottom of its box |
| `bbox`    | `tuple[float, float, float, float]` | `(x0, y0, x1, y1)` |

The box is measured as the box of a `Word` is, and holds every word of the
block. Two `Block` are equal when all their attributes are, and they can be
used in sets and as dictionary keys.

## `extract_images`

```python
ozaru.extract_images(path: PdfSource, *, password: str | None = None) -> list[ozaru.Image]
```

Returns every image of the document, page by page, in the order they are
painted, with its box, its size in pixels and its file. An image painted several
times comes once for each time. Images painted through a tiling pattern, as some
writers paint them, are found too. Images that only make the mask of something
else, through the soft mask of a graphics state, aren't listed. See
[Extract images](../tutorial/extract-images.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`list[Image]`: the images of the document. A page that could not be read has no
images.

**Warns**

`PageWarning`, once for each page that could not be read, or whose content
could only be read in part.

**Raises**

The same exceptions as `extract_text`.

## `Image`

```python
class ozaru.Image
```

An image of a PDF, returned by `extract_images`. Its attributes are read-only.

| Attribute | Type    | Description |
|-----------|---------|-------------|
| `page`    | `int`   | The number of its page, from 1 |
| `number`  | `int`   | The number of the image on the page, from 1, in the order they are painted |
| `x0`, `y0`, `x1`, `y1` | `float` | Its box, measured as the box of a `Word` is |
| `bbox`    | `tuple[float, float, float, float]` | `(x0, y0, x1, y1)` |
| `width`   | `int` or `None` | Its width in pixels. `None` if the image is damaged |
| `height`  | `int` or `None` | Its height in pixels. `None` if the image is damaged |
| `format`  | `str` or `None` | `"jpeg"`, `"jp2"` or `"png"`: the format of `data` |
| `data`    | `bytes` or `None` | The image as a file |

JPEG and JPEG 2000 images are given as the PDF has them, byte for byte, except
JPEG images with a mask or in CMYK, which are decoded and written as PNG like
the images of pixels. Images
of pixels are written as PNG files in gray or RGB, from every common color space:
gray, RGB, CMYK, palettes, ICC profiles, Lab and calibrated gray and RGB, with
1 to 16 bits for each component. CMYK and ICC colors are turned into RGB without
a color profile. A stencil mask is black where it paints and white elsewhere.
Black and white scans in CCITT fax encoding and in JBIG2 are written as PNG
files too, and so are images in spot inks, `/Separation` and `/DeviceN`, in the
colors their functions give. Images with a mask, `/SMask` or `/Mask`, get it as
the alpha channel of their PNG file; JPEG 2000 images come without their mask.
Damaged images, and the rare JBIG2 features ozaru doesn't read yet, have
`format` and `data` set to `None`.

### `save`

```python
Image.save(path: PdfSource) -> None
```

Writes `data` to the file `path`, replacing it if it exists. Raises
`ValueError` if `data` is `None`, and the same `OSError` subclasses as Python's
`open()` if the file can't be written.

Two `Image` are equal when all their attributes are, and they can be used in
sets and as dictionary keys.

## `page_count`

```python
ozaru.page_count(path: PdfSource, *, password: str | None = None) -> int
```

Returns the number of pages in the document. It also works on PDFs protected with
a password, except the rare ones whose list of pages is encrypted too.

The pages are counted by walking the document's page tree, so a damaged file
whose declared page count is wrong still gives the pages it really has.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`int`: the number of pages.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | The path does not exist.                                |
| `PermissionError`    | You don't have permission to read the file.             |
| `IsADirectoryError`  | The path is a folder, not a file.                       |
| `NotADirectoryError` | Part of the path is a file used as if it were a folder. |
| `OSError`            | The file could not be read for another reason.          |
| `PasswordError`      | The list of pages is encrypted and no password was given, or the password is incorrect. |
| `ValueError`         | The file is not a valid PDF, it is too damaged to find its pages, or the path contains a null byte. |
| `TypeError`          | `path` is not a valid path type.                        |

## `metadata`

```python
ozaru.metadata(path: PdfSource, *, password: str | None = None) -> ozaru.Metadata
```

Returns the document information of the PDF: its title, author and dates. See
[Read the metadata](../tutorial/read-metadata.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`Metadata`: the document information. A PDF without any has every attribute set
to `None`. It is read even when the pages of the PDF are too damaged to count.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | The path does not exist.                                |
| `PermissionError`    | You don't have permission to read the file.             |
| `IsADirectoryError`  | The path is a folder, not a file.                       |
| `NotADirectoryError` | Part of the path is a file used as if it were a folder. |
| `OSError`            | The file could not be read for another reason.          |
| `PasswordError`      | The PDF is protected and no password was given, or the password is incorrect. |
| `ValueError`         | The file is not a valid PDF, or the path contains a null byte. |
| `TypeError`          | `path` is not a valid path type.                        |

## `Metadata`

```python
class ozaru.Metadata
```

The document information of a PDF, returned by `metadata`. Its attributes are
read-only, and each one is `None` when the PDF doesn't have it.

| Attribute           | Type               | Description |
|---------------------|--------------------|-------------|
| `title`             | `str` or `None`    | The title of the document |
| `author`            | `str` or `None`    | Who wrote it |
| `subject`           | `str` or `None`    | What it is about |
| `keywords`          | `str` or `None`    | Keywords, as one string |
| `creator`           | `str` or `None`    | The application that created the original document |
| `producer`          | `str` or `None`    | The application that converted it to PDF |
| `creation_date`     | `datetime` or `None` | When it was created |
| `modification_date` | `datetime` or `None` | When it was last changed |
| `xmp`               | `str` or `None`    | The XMP metadata, as the XML the PDF has |

Each attribute comes from the document information dictionary, `/Info`, or,
when it doesn't have it, from the XMP metadata: `dc:title`, `dc:creator`,
`dc:description`, `pdf:Keywords` or `dc:subject`, `xmp:CreatorTool`,
`pdf:Producer`, `xmp:CreateDate` and `xmp:ModifyDate`. Several authors are
separated by `; `.

The dates are aware `datetime` objects when the PDF gives its time zone, and
naive ones when it doesn't. A date the PDF writes in a form that can't be read
is `None`.

Two `Metadata` are equal when all their attributes are, and they can be used in
sets and as dictionary keys.

## `page_info`

```python
ozaru.page_info(path: PdfSource, *, password: str | None = None) -> list[ozaru.PageInfo]
```

Returns the size and rotation of every page, in order. See
[Page sizes](../tutorial/page-sizes.md). Like `page_count`, it works on PDFs
protected with a password, except the rare ones whose list of pages is encrypted
too.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`list[PageInfo]`: one for each page, in order.

**Warns**

`PageWarning`, once for each page that is too damaged to read. It still gets a
`PageInfo`, with the size and rotation it inherits, or US Letter.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | The path does not exist.                                |
| `PermissionError`    | You don't have permission to read the file.             |
| `IsADirectoryError`  | The path is a folder, not a file.                       |
| `NotADirectoryError` | Part of the path is a file used as if it were a folder. |
| `OSError`            | The file could not be read for another reason.          |
| `PasswordError`      | The list of pages is encrypted and no password was given, or the password is incorrect. |
| `ValueError`         | The file is not a valid PDF, it is too damaged to find its pages, or the path contains a null byte. |
| `TypeError`          | `path` is not a valid path type.                        |

## `PageInfo`

```python
class ozaru.PageInfo
```

The size and rotation of one page, returned by `page_info`. Its attributes are
read-only. Sizes are in points, 1/72 of an inch.

| Attribute   | Type    | Description |
|-------------|---------|-------------|
| `number`    | `int`   | The page number, starting at 1 |
| `width`     | `float` | The width of the visible area, as a viewer shows it, rotated |
| `height`    | `float` | The height of the visible area, as a viewer shows it, rotated |
| `rotation`  | `int`   | How many degrees the page is turned clockwise: 0, 90, 180 or 270 |
| `media_box` | `tuple[float, float, float, float]` | The page's media box as `(left, bottom, right, top)`, not rotated |
| `crop_box`  | `tuple[float, float, float, float]` | The visible area as `(left, bottom, right, top)`, not rotated |

Two `PageInfo` are equal when all their attributes are, page number included,
and they can be used in sets and as dictionary keys.

## `render_page`

```python
ozaru.render_page(path: PdfSource, page: int, *, scale: float = 1.0, annotations: bool = True, password: str | None = None) -> bytes
```

Draws a page of the PDF as a PNG image. See
[Draw a page as an image](../tutorial/render-pages.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `page`     | `int`                       | The number of the page, starting at 1 |
| `scale`    | `float`                     | Pixels for each point of the page, as a keyword argument: 1, the default, is 72 dots per inch. It must be a positive number. |
| `annotations` | `bool`                   | Whether annotations and the widgets of form fields are drawn, as a keyword argument. `True` by default. |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`bytes`: the PNG file. The page as a viewer shows it, its crop box turned by
its rotation, on white.

**Warns**

`PageWarning` if the page is too damaged to read. It is drawn empty.

**Raises**

- `IndexError` if the PDF doesn't have the page.
- `ValueError` if the scale isn't a positive number, if the image would be
  larger than 256 MB, or if the file is damaged or not a PDF.
- `ozaru.PasswordError` (a `ValueError`) if the PDF is protected and the password
  is missing or wrong.
- `OSError` subclasses if the file can't be read.

## `annotations`

```python
ozaru.annotations(path: PdfSource, *, password: str | None = None) -> list[Annotation]
```

Returns the annotations of every page of the PDF, page by page, in the order
the PDF lists them. See [Read the annotations](../tutorial/read-annotations.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`list[Annotation]`: empty if no page has annotations.

**Warns**

`PageWarning` for each page that is too damaged to read: it has no annotations.

**Raises**

- `ValueError` if the file is damaged or not a PDF.
- `ozaru.PasswordError` (a `ValueError`) if the PDF is protected and the password
  is missing or wrong.
- `OSError` subclasses if the file can't be read.

## `Annotation`

```python
class ozaru.Annotation
```

An annotation of a page, returned by `annotations`. Its attributes are read-only.

| Attribute | Type | Description |
|-----------|------|-------------|
| `page` | `int` | The number of its page, from 1 |
| `subtype` | `str` | `Link`, `Text`, `Highlight`, `FreeText`, `Widget`... as the PDF names it |
| `rect` | `tuple[float, float, float, float]` | `(x0, y0, x1, y1)`, in points from the top left corner of the visible area, before rotation, as for `Word` |
| `contents` | `str` or `None` | The text of a note, or its description |
| `name` | `str` or `None` | The name that identifies it in its page |
| `author` | `str` or `None` | Who wrote it |
| `subject` | `str` or `None` | A short summary |
| `modified` | `datetime` or `None` | When it was last changed, aware when the PDF gives its time zone |
| `flags` | `int` | The flags of the PDF standard: 2 is hidden, 4 is print |
| `color` | `list[float]` or `None` | 1, 3 or 4 numbers from 0 to 1 |
| `quad_points` | `list[tuple]` | The four corners, as 8 numbers, of each line a highlight, underline or strike-out covers, in the same coordinates as `rect` |
| `uri` | `str` or `None` | Where a link to the web goes |
| `target_page` | `int` or `None` | The page of this PDF a link goes to, from 1 |
| `has_appearance` | `bool` | Whether it has a drawing that `render_page` shows |

Two `Annotation` are equal when all their attributes are, and they can be used
in sets and as dictionary keys.

## `form_fields`

```python
ozaru.form_fields(path: PdfSource, *, password: str | None = None) -> list[FormField]
```

Returns the fields of the form of the PDF, in the order of its field tree, with
their values. See [Read and fill forms](../tutorial/fill-forms.md).

**Returns**

`list[FormField]`: empty if the PDF has no form.

**Warns**

`PageWarning` for each page that is too damaged to read: its widgets may be missing.

**Raises**

- `ValueError` if the file is damaged or not a PDF.
- `ozaru.PasswordError` (a `ValueError`) if the PDF is protected and the password
  is missing or wrong.
- `OSError` subclasses if the file can't be read.

## `FormField`

```python
class ozaru.FormField
```

A field of a form, returned by `form_fields`. Its attributes are read-only.

| Attribute | Type | Description |
|-----------|------|-------------|
| `name` | `str` | The full name, with the names of its groups joined by dots |
| `kind` | `str` | `text`, `checkbox`, `radio`, `button`, `combo`, `list`, `signature` or `unknown` |
| `value` | `str`, `bool`, `list[str]` or `None` | A `str` for text, combos and lists, a `bool` for check boxes, the chosen button for radio groups, a list for lists that allow several |
| `default_value` | same, or `None` | The value the field goes back to when reset |
| `read_only` | `bool` | Whether the PDF marks it read-only |
| `required` | `bool` | Whether the PDF marks it required |
| `multiline` | `bool` | Whether a text field has several lines |
| `max_length` | `int` or `None` | The most characters a text field takes |
| `options` | `list[tuple[str, str]]` | The choices of a combo or list, as `(value, label)` |
| `page` | `int` or `None` | The page of its first widget, from 1 |
| `rect` | `tuple` or `None` | The rectangle of its first widget, as for `Annotation.rect` |

## `fill_form`

```python
ozaru.fill_form(path: PdfSource, output: str | os.PathLike[str], values: dict[str, str | bool | list[str] | None], *, password: str | None = None, incremental: bool = False) -> None
```

Fills fields of the form and saves the result to another file, drawing their
appearance again. See [Read and fill forms](../tutorial/fill-forms.md).

**Parameters**

| Name | Type | Description |
|------|------|-------------|
| `path` | `str` or `os.PathLike[str]` | Path to the PDF file |
| `output` | `str` or `os.PathLike[str]` | Where the filled PDF is saved |
| `values` | `dict` | Field names and the values to give them: `str`, `bool`, `list[str]` or `None` to clear |
| `password` | `str` or `None` | The user or owner password of a protected PDF. The result has no password. |
| `incremental` | `bool` | Add the changes to the end of the original file instead of writing it again |

**Warns**

`PageWarning` for each page that is too damaged to read: it is saved empty.

**Raises**

- `ValueError` if a field isn't in the PDF or can't hold its value, or the file
  is damaged or not a PDF.
- `ozaru.PasswordError` (a `ValueError`) if the PDF is protected and the password
  is missing or wrong.
- `OSError` subclasses if a file can't be read or written.

## `encrypt`

```python
ozaru.encrypt(path: PdfSource, output: str | os.PathLike[str], *, user_password: str = "", owner_password: str | None = None, permissions: Collection[str] | None = None, encrypt_metadata: bool = True, password: str | None = None) -> None
```

Protects the PDF with passwords, using AES-256, and saves the result to another
file. See [Protected PDFs](../tutorial/protected-pdfs.md#protect-a-pdf).

**Parameters**

| Name | Type | Description |
|------|------|-------------|
| `path` | `str` or `os.PathLike[str]` | Path to the PDF file |
| `output` | `str` or `os.PathLike[str]` | Where the protected PDF is saved |
| `user_password` | `str` | What opens the file, as a keyword argument. Empty by default: anyone can open it |
| `owner_password` | `str` or `None` | What opens it with every permission. The user password if left out |
| `permissions` | collection of `str` or `None` | What to allow: `print`, `modify`, `copy`, `annotate`, `fill_forms`, `accessibility`, `assemble`, `print_high_quality`. `None` allows all |
| `encrypt_metadata` | `bool` | Whether the XMP metadata is encrypted too. `True` by default |
| `password` | `str` or `None` | The password of the PDF, if it is protected already |

**Warns**

`PageWarning` for each page that is too damaged to read: it is saved empty.

**Raises**

- `ValueError` if a permission isn't one of the names, a password has characters
  PDF doesn't allow, or the file is damaged or not a PDF.
- `ozaru.PasswordError` (a `ValueError`) if the PDF is protected and the password
  is missing or wrong.
- `OSError` subclasses if a file can't be read or written.

## `permissions`

```python
ozaru.permissions(path: PdfSource, *, password: str | None = None) -> list[str] | None
```

Returns what the PDF allows, as a list of the names `encrypt` takes, or `None` if
the PDF isn't protected.

## `rotate`

```python
ozaru.rotate(path: PdfSource, output: str | os.PathLike[str], degrees: int, *, pages: Sequence[int] | None = None, password: str | None = None, incremental: bool = False) -> None
```

Turns pages of the PDF clockwise and saves the result to `output`. See
[Rotate pages](../tutorial/rotate-pages.md). Each page's rotation becomes the
one it had plus `degrees`. The new file has the whole document, without the
objects it doesn't use, and without a password. `output` can be `path` itself.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `output`   | `str` or `os.PathLike[str]` | Path of the file to write. It is replaced if it exists. |
| `degrees`  | `int`                       | How much to turn the pages clockwise: a multiple of 90, negative to turn them counterclockwise |
| `pages`    | sequence of `int` or `None` | The numbers of the pages to turn, starting at 1, as a keyword argument. `None`, the default, turns every page. |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |
| `incremental` | `bool`                   | As a keyword argument: `True` keeps every byte of the original and adds the changed pages at its end, which keeps digital signatures valid. `False`, the default, writes the whole file again. |

**Warns**

`PageWarning`, once for each page that is too damaged to read. It is saved as
an empty page of its size.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | `path` does not exist, or the folder of `output` doesn't. |
| `PermissionError`    | You don't have permission to read `path` or write `output`. |
| `IsADirectoryError`  | `path` or `output` is a folder, not a file.             |
| `NotADirectoryError` | Part of a path is a file used as if it were a folder.   |
| `OSError`            | A file could not be read or written for another reason. |
| `PasswordError`      | The PDF is protected and no password was given, or the password is incorrect. |
| `IndexError`         | A number in `pages` is not a page of the PDF.           |
| `ValueError`         | `degrees` is not a multiple of 90, the file is not a valid PDF, or it is too damaged to find its pages. With `incremental=True`, also when the PDF is protected or damaged. |
| `TypeError`          | A path is not a valid path type.                        |

## `extract_pages`

```python
ozaru.extract_pages(path: PdfSource, output: str | os.PathLike[str], pages: Sequence[int], *, password: str | None = None) -> None
```

Saves the pages numbered in `pages`, in that order, to `output`. See
[Extract and split pages](../tutorial/split-pages.md). Pages can be repeated.
Only what those pages use is saved, with the document information. The
bookmarks, forms, named destinations, page labels and structure tags of the
document are left out. The new file has no password. `output` can be `path`
itself.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `PdfSource` | The PDF: a path, bytes, a binary file or a URL |
| `output`   | `str` or `os.PathLike[str]` | Path of the file to write. It is replaced if it exists. |
| `pages`    | sequence of `int`           | The numbers of the pages to save, starting at 1, in the order to save them |
| `password` | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Warns**

`PageWarning`, once for each page that is too damaged to read. It is saved as
an empty page of its size.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | `path` does not exist, or the folder of `output` doesn't. |
| `PermissionError`    | You don't have permission to read `path` or write `output`. |
| `IsADirectoryError`  | `path` or `output` is a folder, not a file.             |
| `NotADirectoryError` | Part of a path is a file used as if it were a folder.   |
| `OSError`            | A file could not be read or written for another reason. |
| `PasswordError`      | The PDF is protected and no password was given, or the password is incorrect. |
| `IndexError`         | A number in `pages` is not a page of the PDF.           |
| `ValueError`         | `pages` is empty, the file is not a valid PDF, or it is too damaged to find its pages. |
| `TypeError`          | A path is not a valid path type, or `pages` is not a sequence of integers. |

## `split`

```python
ozaru.split(path: PdfSource, folder: str | os.PathLike[str], *, pages_per_file: int = 1, password: str | None = None) -> list[pathlib.Path]
```

Saves the PDF as several files in `folder`, one for each page, or for each
group of `pages_per_file` pages. See
[Extract and split pages](../tutorial/split-pages.md). The folder is created
if it doesn't exist. Each file is named after the PDF and the number of its
part, padded with zeros so the files sort in order: `report-1.pdf`, or
`report-01.pdf` when there are ten parts or more. Files with those names are
replaced. Each file keeps what `extract_pages` keeps.

**Parameters**

| Name             | Type                        | Description          |
|------------------|-----------------------------|----------------------|
| `path`           | `str` or `os.PathLike[str]` | Path to the PDF file |
| `folder`         | `str` or `os.PathLike[str]` | The folder to write the files in |
| `pages_per_file` | `int`                       | How many pages each file has, as a keyword argument. The last file has the pages left over. |
| `password`       | `str` or `None`             | The user or owner password of a protected PDF, as a keyword argument. Ignored if the PDF isn't protected. |

**Returns**

`list[pathlib.Path]`: the files written, in order.

**Warns**

`PageWarning`, once for each page that is too damaged to read. It is saved as
an empty page of its size.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | `path` does not exist.                                  |
| `PermissionError`    | You don't have permission to read `path`, or to write in `folder`. |
| `IsADirectoryError`  | `path` is a folder, or a folder has the name of a file to write. |
| `NotADirectoryError` | `folder`, or part of a path, is a file.                 |
| `OSError`            | A file could not be read or written for another reason. |
| `PasswordError`      | The PDF is protected and no password was given, or the password is incorrect. |
| `ValueError`         | `pages_per_file` is less than 1, the file is not a valid PDF, or it is too damaged to find its pages. |
| `TypeError`          | A path is not a valid path type.                        |

## `merge`

```python
ozaru.merge(paths: Sequence[PdfSource], output: str | os.PathLike[str], *, password: str | None = None) -> None
```

Joins the PDFs of `paths`, in that order, into `output`. See
[Merge PDFs](../tutorial/merge-pdfs.md). What the pages of a PDF share is
saved once, and the document information comes from the first PDF. The
bookmarks, forms, named destinations, page labels and structure tags of the
documents are left out. The new file has no password. `output` can be one of
`paths`.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `paths`    | sequence of `PdfSource` | The PDFs, in the order to join them |
| `output`   | `str` or `os.PathLike[str]` | Path of the file to write. It is replaced if it exists. |
| `password` | `str` or `None`             | The user or owner password of the protected PDFs, as a keyword argument. Used only for the PDFs that need one. |

**Warns**

`PageWarning`, once for each page that is too damaged to read, with its number
in the joined file. It is saved as an empty page of its size.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | A path does not exist, or the folder of `output` doesn't. |
| `PermissionError`    | You don't have permission to read a PDF or write `output`. |
| `IsADirectoryError`  | A path or `output` is a folder, not a file.             |
| `NotADirectoryError` | Part of a path is a file used as if it were a folder.   |
| `OSError`            | A file could not be read or written for another reason. |
| `PasswordError`      | A PDF is protected and no password was given, or the password is incorrect. The message starts with its path. |
| `ValueError`         | `paths` is empty, or a file is not a valid PDF or is too damaged to find its pages. The message starts with its path. |
| `TypeError`          | A path is not a valid path type, or `paths` is a single path instead of a sequence. |

## `open`, `Document` and `Page`

```python
ozaru.open(path: PdfSource, *, password: str | None = None) -> ozaru.Document
```

Reads the PDF once and gives a `Document` (`ozaru.Document(path)` is the same). It raises
what `page_count` raises, and `PasswordError` if the password is wrong.

A `Document` has the length of its pages, `pdf[i]` (from 0, negative from the end, `IndexError`
past the end), slices, iteration and `with`. Its properties, `text`, `words`, `blocks`, `images`,
`annotations`, `form_fields`, `page_info`, `bookmarks`, `page_labels`, `named_destinations`, `attachments`, `metadata`, `permissions` and `page_count`, give what
the function of the same name gives. `render(page, *, scale, annotations)`, `rotate`,
`extract_pages`, `split`, `fill_form`, `encrypt` and `save(output)` (a copy of the PDF as it was
read) take the arguments of their functions, except the PDF and its password.

A `Page` has `number` (from 1), `text`, `words`, `blocks`, `images`, `annotations`,
`form_fields` (those whose first widget is on it), `label`, `info`, `width`, `height`, `rotation` and
`render(*, scale, annotations)`.

What is read is kept by the document, once: the text, words, blocks, images and annotations of
every page are read the first time any of them is asked for, and a `PageWarning` for a page that
can't be read is raised then, once.

## `bookmarks`

```python
ozaru.bookmarks(path: PdfSource, *, password: str | None = None) -> list[ozaru.Bookmark]
```

Returns the bookmarks, the outline a viewer shows beside the pages, in the order it shows
them: each one followed by its children. Empty if the PDF has none.

A `Bookmark` has `title`, `level` (1 for the top ones, 2 for their children...), `page` (from 1,
`None` if it doesn't go to a page of the PDF), `uri` (for a bookmark that opens an address) and
`open` (whether a viewer shows its children).

## `page_labels`

```python
ozaru.page_labels(path: PdfSource, *, password: str | None = None) -> list[str]
```

Returns the label of every page as a viewer shows it: `i`, `ii`, `1`, `2`, `A-1`... A PDF
without page labels gives its page numbers as text, `["1", "2", "3"]`.

## `named_destinations`

```python
ozaru.named_destinations(path: PdfSource, *, password: str | None = None) -> list[ozaru.NamedDestination]
```

Returns the names that links can use to go to a page. A `NamedDestination` has `name` and
`page` (from 1, `None` if it isn't a page of the PDF).

## `attachments`

```python
ozaru.attachments(path: PdfSource, *, password: str | None = None) -> list[ozaru.Attachment]
```

Returns the files attached to the PDF. An `Attachment` has `name`, `filename`, `description`,
`mime_type`, `size` (as the PDF says it), `creation_date`, `modification_date` and `data` (the
contents, `None` if they can't be read), and `save(path)` writes it to a file.

!!! warning "Don't trust the file name"
    `filename` comes from the PDF and can be anything, such as `../../file`. Use only its last
    part, or a name of your own, when you save a file. `ozaru attachments -o` does.

## Writing the information, bookmarks, labels, attachments and pages

These functions take a PDF as the others do, and save the result to `output`, another
file, as `rotate` does. They take `password` and `incremental` too (see `rotate`), except
`delete_pages`. What they don't change stays as it was.

```python
ozaru.set_metadata(path, output, values: dict)
ozaru.set_bookmarks(path, output, bookmarks: Sequence[Bookmark | tuple])
ozaru.set_page_labels(path, output, labels: Sequence[dict])
ozaru.add_attachment(path, output, name: str, data: bytes | PdfSource, *, filename=None, description=None, mime_type=None)
ozaru.remove_attachment(path, output, name: str)
ozaru.crop(path, output, box: tuple[float, float, float, float], *, pages=None)
ozaru.delete_pages(path, output, pages: Sequence[int])
ozaru.add_blank_page(path, output, *, position=None, width=None, height=None)
```

**`set_metadata`**: `values` maps `title`, `author`, `subject`, `keywords`, `creator` and `producer`
to a `str`, `creation_date` and `modification_date` to a `datetime` (aware ones keep their time
zone) and `xmp` to the XML text. A `None` removes the value, and what `values` doesn't name stays.
The names are checked: `ValueError` for a name that doesn't exist, `TypeError` for a value of the
wrong type.

**`set_bookmarks`**: replaces the outline. Each item is a `Bookmark` (as `bookmarks` returns them)
or a tuple `(level, title, target)`, where `target` is a page number (from 1), an address (`str`) or
`None`. Items come in the order a viewer shows them; `level` is 1 for the top ones and at most one
more than the item before, or `ValueError`. An empty list removes the outline.

**`set_page_labels`**: replaces the labels. Each item is a `dict` with `page` (the first page of the
range, from 1; the first range must start at 1) and, if wanted, `style` (`"decimal"`, `"roman"`,
`"roman_lower"`, `"letters"` or `"letters_lower"`), `prefix` and `start` (the number of its first
page). `[{"page": 1, "style": "roman_lower"}, {"page": 3, "style": "decimal"}]` labels the pages
`i`, `ii`, `1`, `2`... An empty list removes them.

**`add_attachment`**: `data` is the contents as `bytes`, or the path of a file. `name` is what the
attachment is called in the PDF: one with that name is replaced. `filename` (by default the name of
the file given, or `name`) is what it is saved as, and `mime_type` is guessed from it if not given.

**`crop`**: `box` is `(x0, y0, x1, y1)`, measured from the top left corner of the visible area of
each page, as the boxes of words are. The text and drawings outside stay in the file, but viewers
and ozaru don't show them. `pages` are counted from 1 (all by default).

**`delete_pages`**: removes the pages, as `extract_pages` keeps some: the bookmarks, form and
named destinations are dropped. `ValueError` if no page would be left.

**`add_blank_page`**: `position` is the number the new page will have, from 1 (at the end by
default); `width` and `height` are in points, those of the first page by default. If the page isn't
added at the end, the page labels are removed, as they count pages by place.

`Document` has all of them as methods, with the PDF and its password left out:
`pdf.set_metadata(output, values)`, `pdf.crop(output, box)`...

## `search`

```python
ozaru.search(path: PdfSource, text: str, *, case_sensitive: bool = False, pages: Sequence[int] | None = None, password: str | None = None) -> list[ozaru.Match]
```

Finds `text` and returns where it is, in reading order. Letters of any case match unless
`case_sensitive`, runs of whitespace (in the page and in `text`) count as one space, and a match
can go over several lines of a block. The search is for text, not for words: `"the"` is in
`"other"`. `pages` are the pages to search, counted from 1 (all by default).

A `Match` has `page`, `text` (as the page has it), `boxes` (one for each line the match is on) and
`bbox` (around all of them), measured from the top left of the visible area, as the boxes of words
are.

## `extract_spans`

```python
ozaru.extract_spans(path: PdfSource, *, password: str | None = None) -> list[ozaru.Span]
```

Returns the pieces of text of each line that have one font, one size and one color, page by page.
A gap of more than an em (between the cells of a table row, for example) starts a new span.

A `Span` has `page`, `block` and `line` (all from 1), `text`, `font` (as the PDF names it, such as
`ABCDEF+Arial-BoldMT`), `size` in points, `color` as `(red, green, blue)`, `bold` and `italic` (as
the name of the font says), and a box (`x0`, `y0`, `x1`, `y1` and `bbox`) as the boxes of words.

## `extract_tables`

```python
ozaru.extract_tables(path: PdfSource, *, strategy: str = "both", password: str | None = None) -> list[ozaru.Table]
```

Returns the tables, page by page. `strategy` says how they are found:

- `"lines"`: grids made of the lines the page draws: strokes, rectangles and filled rectangles so
  thin that they are rules. Where lines cross, cells are made; cells that share a corner are one
  table, and a single box is not a table.
- `"text"`: text that lines up in columns and rows: three rows or more, two columns or more, short
  cells. A column of bullets or numbers is a list, and columns of prose aren't tables.
- `"both"` (the default): lines first, and text where there are no lines.

A `Table` has `page`, `rows` (the text of each cell, row by row from the top; `None` where the row
has no cell, as after a cell that spans columns), `ruled` (made of lines), `markdown` and a box.

## `to_markdown`

```python
ozaru.to_markdown(path: PdfSource, *, page_markers: bool = False, strategy: str = "both", password: str | None = None) -> str
```

Returns the PDF as Markdown, to read or to give to a language model:

- **Headings** by the size of their text compared with that of the whole document: the largest
  sizes of short blocks, up to six, are `#` to `######`.
- **Bold and italic** from the names of the fonts.
- **Lists**: lines that start with a bullet or `1.` stay list items, and a line that goes on
  belongs to its item.
- **Tables** as Markdown tables, in their place and without their text repeated.
- **Paragraphs** joined, with words cut by a hyphen at the end of a line put back together, and
  `*`, `_` and the like escaped.

With `page_markers`, each page starts with a comment, `<!-- page 2 -->`, so the page of each part
is known. `strategy` is as in `extract_tables`.

`Document` has `spans`, `tables`, `markdown`, `to_markdown(...)`, `search(...)` and
`find_tables(strategy=...)`, and each `Page` has `spans`, `tables`, `markdown` and `search(...)`;
what they read is kept.

## Drawing on pages

```python
ozaru.draw(path: PdfSource, output, items: Sequence[Text | Picture | Line | Rect | Ellipse | Polygon], *, password=None, incremental=False)
ozaru.add_watermark(path: PdfSource, output, text: str, *, pages=None, font="Helvetica-Bold", size=None, color=None, opacity=0.3, rotation=45.0, password=None, incremental=False)
ozaru.text_width(text: str, *, font="Helvetica", size=12.0) -> float
```

`draw` adds what `items` say to the pages, in order, over what each page has, and saves the result
to `output`; the pages' own content is not touched. **Positions are in points, measured from the top
left corner of the visible area of the page**, as the boxes of words are: `y` grows downwards.

| Item | Draws | Arguments after `page` |
|---|---|---|
| `Text` | text in a standard font or a TrueType file | `x, y, text, *, font="Helvetica", size=12, color, opacity=1, rotation=0, align="left", valign="baseline"` |
| `Picture` | a JPEG or PNG file | `x, y, source, *, width=None, height=None, opacity=1, rotation=0` |
| `Line` | a straight line | `x0, y0, x1, y1, *, color, width=1, dash=(), opacity=1, round_ends=False` |
| `Rect` | a rectangle | `x0, y0, x1, y1, *, fill=None, stroke=black, width=1, dash=(), opacity=1` |
| `Ellipse` | an ellipse inside that box | the same as `Rect` |
| `Polygon` | a path through points, closed by default | `points, *, fill=None, stroke=black, width=1, dash=(), opacity=1, closed=True` |

- **Colors** are `(red, green, blue)` with each from 0 to 255, `"#rrggbb"`, `"#rgb"` or a name
  (`"red"`, `"gray"`, `"orange"`...). **Opacity** goes from 0 (invisible) to 1.
- **Fonts** are the standard 14, which every reader has and take no space in the file: `Helvetica`,
  `Times-Roman` and `Courier`, each with `-Bold`, `-Italic` (or `-Oblique`) and `-BoldItalic`, and
  `Symbol` and `ZapfDingbats`. `"times"`, `"courier"`, `"serif"` and similar are accepted. They have
  the letters of Western Europe, such as `ñ ü é € « »`; a character they don't have is a
  `ValueError` that names it. **A font of your own** is the path of a TrueType file (`.ttf`, as a
  string or a `Path`) given as `font`: it is embedded with only the glyphs the drawing uses (a
  subset, so the file stays small) and the text can still be selected and extracted. The font needs
  the characters; its license must allow embedding and subsetting (`fsType` of the font), or it is a
  `ValueError`; OpenType fonts with PostScript outlines (`.otf`) aren't supported yet. There is no
  kerning, ligatures or shaping of complex scripts.
- **`Picture`**: `(x, y)` is the top left corner of the picture; `source` is the path of the file or its
  bytes. With one of `width` and `height` (points) the other keeps the shape of the picture, with
  both it is stretched, and with none each pixel is a point. JPEG files are embedded as they are
  (gray, RGB and CMYK, baseline or progressive); PNG files are decoded, with their transparency as a
  soft mask. Anything else is a `ValueError`.
- **`Text`**: `(x, y)` is where the baseline starts; with `align="center"` or `"right"`, where its
  middle or end is; with `valign="middle"`, `y` is the middle of the letters. `rotation` turns it
  counterclockwise around `(x, y)`.
- The page is not changed in the file where it was a rotation: positions are in the unrotated page,
  as the boxes of words are.

`add_watermark` writes `text` across the middle of each page (`pages`, from 1, by default all),
turned `rotation` degrees, in `color` (gray by default) and `opacity`. By default its size makes the
text as wide as 70 % of the diagonal of the page.

`text_width` is the width of a text in points, in a standard font or a `.ttf` file, to place it: `x = page_width - 72 -
ozaru.text_width(text, size=12)` ends it a margin from the right edge.

`Document` has `draw(output, items)` and `add_watermark(output, text, ...)`.

## Annotating

```python
ozaru.annotate(path: PdfSource, output, items: Sequence[Highlight | Underline | StrikeOut | Squiggly | Note | Link | Box | Oval | Arrow | Ink | TextBox], *, password=None, incremental=False)
ozaru.delete_annotations(path: PdfSource, output, *, pages=None, subtypes=None, name=None, author=None, contains=None, password=None, incremental=False) -> int
ozaru.update_annotations(path: PdfSource, output, changes: dict, *, pages=None, subtypes=None, name=None, author=None, contains=None, password=None, incremental=False) -> int
ozaru.highlight(path: PdfSource, output, text: str, *, kind="highlight", case_sensitive=False, pages=None, color=None, opacity=1.0, note=None, author=None, password=None, incremental=False) -> int
```

`annotate` adds the marks to their pages, keeping everything else, and saves the result to `output`.
Each item has `page` (from 1) and `boxes`, which is one `(x0, y0, x1, y1)` or a list of them (one for
each line), **in points from the top left corner of the visible area of the page**, as the boxes of
words, spans and search results are; and optionally `color` (as for `draw`; yellow for a `Highlight`,
red for the others), `opacity`, `note` (the comment), `author`, `subject` and `date` (a `datetime`;
now by default).

| Item | Draws |
|---|---|
| `Highlight` | a translucent band, multiplied with the page so the text under it stays dark |
| `Underline` | a line under the text |
| `StrikeOut` | a line through the text |
| `Squiggly` | a wavy line under the text |

Each mark is a standard annotation with its own appearance stream, so it shows in any reader, it is
listed by `annotations` (with `quad_points`, `color`, `contents`, `author`, `subject` and `modified`),
and it can be changed or deleted in a viewer. An `IndexError` names a page that doesn't exist, and a
`ValueError` a box that isn't four numbers.

More annotations, all with `page` first, the keyword arguments `color`, `opacity`, `note`, `author`,
`subject` and `date`, and positions from the top left of the page:

| Item | Is | Arguments after `page` |
|---|---|---|
| `Note` | a sticky note, whose `text` opens when clicked | `x, y, text, *, open=False` (the icon is yellow by default) |
| `Link` | an area that goes somewhere | `box, *, uri=None, to_page=None, border=0` (a `uri` or a `to_page` from 1, not both; blue frame if `border` > 0) |
| `Box`, `Oval` | a rectangle, an ellipse | `box, *, fill=None, width=1.5` (outlined in `color`, red by default) |
| `Arrow` | a line, with an arrowhead at its end | `start, end, *, width=1.5, arrow=True` |
| `Ink` | a freehand drawing | `strokes, *, width=1.5` (a list of strokes, each a list of `(x, y)`) |
| `TextBox` | text in Helvetica wrapped in a box | `box, text, *, size=12, fill=None, border=0` (`color` is the text's, black by default) |

Each is a standard annotation with its own appearance. A text the font can't draw, a box with no area
or an ink drawing without points is a `ValueError`.

`delete_annotations` removes the annotations on `pages` that match every filter given, and returns
how many it removed: `subtypes` (`["Highlight", "Link"]`), the `name` (`/NM`) or `author`, or text the
comment `contains`; with none, all go, except form fields, which are never removed. `update_annotations`
sets `contents`, `author` and `subject` (text, or `None` to remove) and `hidden` on those that match
and returns how many it changed; their drawing isn't redone. `Document` has `annotate`,
`delete_annotations` and `update_annotations`.

`highlight` searches the PDF as `search` does and marks each match (one box for each line it is on)
with the `kind` given; it returns how many places it marked. `Document` has `annotate(output, items)`.

## Redacting

```python
ozaru.redact(path: PdfSource, output, items: Sequence[Redaction], *, images="pixels", graphics="covered", annotations=True, password=None) -> RedactionReport
ozaru.redact_text(path: PdfSource, output, text: str, *, case_sensitive=False, pages=None, fill="black", images="pixels", graphics="covered", annotations=True, password=None) -> int
```

`Redaction(page, box, *, fill="black")`: `box` is `(x0, y0, x1, y1)` in points from the top left of the
visible area of the page, as the boxes of words and search results are. `fill` is the color of the box
drawn over the area (`None` for none).

`redact` takes **out of the file** what is under each area and saves the result to `output`:

- text: the letters are removed from the content of the page and the rest of the line keeps its
  place; a letter goes if the center of its box is in the area or a quarter of its box is;
- `images`: `"pixels"` blanks the pixels under the area and keeps the rest of the picture (an image
  whose pixels can't be read is removed), `"remove"` takes out the whole image, `"keep"` leaves it;
- `graphics`: `"covered"` removes the lines and shapes that are all inside the area, `"touched"` those
  that touch it, `"keep"` none;
- `annotations`: remove those that touch the area; form fields are never removed, and a `UserWarning`
  says how many are under an area.

It follows the forms (reusable drawings) the page paints, copying them for that page. It doesn't
reach bookmarks, form fields, the structure of tagged files, the metadata, attachments, the glyphs of
Type 3 fonts or patterns. It never saves incrementally. A page too complex to be checked to the end
is a `ValueError` and nothing is saved.

It returns a `RedactionReport` with `glyphs`, `images_removed`, `images_blanked`, `paths`, `forms`,
`annotations` and `widgets_left`, and `empty` (nothing was under the areas). `redact_text` finds the
text as `search` does and redacts one box for each line of each match; it returns how many places it
redacted.

## `PasswordError`

```python
class ozaru.PasswordError(ValueError)
```

Raised when a PDF is protected and no password was given, or the password given
is incorrect. See [Protected PDFs](../tutorial/protected-pdfs.md).

It is a subclass of `ValueError`, so code that catches `ValueError` catches it
too.

## `PageWarning`

```python
class ozaru.PageWarning(UserWarning)
```

Emitted by `extract_text`, `annotations`, `page_info`, `render_page`, `rotate`, `extract_pages`, `split` and `merge` for each page that could not be read.
The other pages are still returned or saved. See [Handle errors](../tutorial/handle-errors.md#pages-that-cant-be-read).

**Attributes**

| Name     | Type  | Description                                   |
|----------|-------|-----------------------------------------------|
| `page`   | `int` | Number of the page that failed, starting at 1 |
| `reason` | `str` | Why the page could not be read                |
