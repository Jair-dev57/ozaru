# API

ozaru has fifteen functions, the `Word`, `Block`, `Image`, `Metadata`, `PageInfo`, `Annotation` and `FormField` classes they return,
one exception and one warning class. Every function takes the path of a PDF file, as a `str` or a
`pathlib.Path`.

Every function releases Python's GIL while it reads, so you can call them from
several threads at the same time.

## `extract_text`

```python
ozaru.extract_text(path: str | os.PathLike[str], *, password: str | None = None) -> str
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
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.extract_words(path: str | os.PathLike[str], *, password: str | None = None) -> list[ozaru.Word]
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
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.extract_blocks(path: str | os.PathLike[str], *, password: str | None = None) -> list[ozaru.Block]
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
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.extract_images(path: str | os.PathLike[str], *, password: str | None = None) -> list[ozaru.Image]
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
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
Image.save(path: str | os.PathLike[str]) -> None
```

Writes `data` to the file `path`, replacing it if it exists. Raises
`ValueError` if `data` is `None`, and the same `OSError` subclasses as Python's
`open()` if the file can't be written.

Two `Image` are equal when all their attributes are, and they can be used in
sets and as dictionary keys.

## `page_count`

```python
ozaru.page_count(path: str | os.PathLike[str], *, password: str | None = None) -> int
```

Returns the number of pages in the document. It also works on PDFs protected with
a password, except the rare ones whose list of pages is encrypted too.

The pages are counted by walking the document's page tree, so a damaged file
whose declared page count is wrong still gives the pages it really has.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.metadata(path: str | os.PathLike[str], *, password: str | None = None) -> ozaru.Metadata
```

Returns the document information of the PDF: its title, author and dates. See
[Read the metadata](../tutorial/read-metadata.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.page_info(path: str | os.PathLike[str], *, password: str | None = None) -> list[ozaru.PageInfo]
```

Returns the size and rotation of every page, in order. See
[Page sizes](../tutorial/page-sizes.md). Like `page_count`, it works on PDFs
protected with a password, except the rare ones whose list of pages is encrypted
too.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.render_page(path: str | os.PathLike[str], page: int, *, scale: float = 1.0, annotations: bool = True, password: str | None = None) -> bytes
```

Draws a page of the PDF as a PNG image. See
[Draw a page as an image](../tutorial/render-pages.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.annotations(path: str | os.PathLike[str], *, password: str | None = None) -> list[Annotation]
```

Returns the annotations of every page of the PDF, page by page, in the order
the PDF lists them. See [Read the annotations](../tutorial/read-annotations.md).

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.form_fields(path: str | os.PathLike[str], *, password: str | None = None) -> list[FormField]
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
ozaru.fill_form(path: str | os.PathLike[str], output: str | os.PathLike[str], values: dict[str, str | bool | list[str] | None], *, password: str | None = None, incremental: bool = False) -> None
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

## `rotate`

```python
ozaru.rotate(path: str | os.PathLike[str], output: str | os.PathLike[str], degrees: int, *, pages: Sequence[int] | None = None, password: str | None = None, incremental: bool = False) -> None
```

Turns pages of the PDF clockwise and saves the result to `output`. See
[Rotate pages](../tutorial/rotate-pages.md). Each page's rotation becomes the
one it had plus `degrees`. The new file has the whole document, without the
objects it doesn't use, and without a password. `output` can be `path` itself.

**Parameters**

| Name       | Type                        | Description          |
|------------|-----------------------------|----------------------|
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.extract_pages(path: str | os.PathLike[str], output: str | os.PathLike[str], pages: Sequence[int], *, password: str | None = None) -> None
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
| `path`     | `str` or `os.PathLike[str]` | Path to the PDF file |
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
ozaru.split(path: str | os.PathLike[str], folder: str | os.PathLike[str], *, pages_per_file: int = 1, password: str | None = None) -> list[pathlib.Path]
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
ozaru.merge(paths: Sequence[str | os.PathLike[str]], output: str | os.PathLike[str], *, password: str | None = None) -> None
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
| `paths`    | sequence of `str` or `os.PathLike[str]` | Paths to the PDF files, in the order to join them |
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
