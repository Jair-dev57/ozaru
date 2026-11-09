# API

ozaru has four functions, the `Metadata` and `PageInfo` classes they return,
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
  most documents, separated by `\n`.
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

Emitted by `extract_text` and `page_info` for each page that could not be read.
The other pages are still returned. See [Handle errors](../tutorial/handle-errors.md#pages-that-cant-be-read).

**Attributes**

| Name     | Type  | Description                                   |
|----------|-------|-----------------------------------------------|
| `page`   | `int` | Number of the page that failed, starting at 1 |
| `reason` | `str` | Why the page could not be read                |
