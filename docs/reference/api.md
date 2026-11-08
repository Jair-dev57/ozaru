# API

ozaru has three functions, the `Metadata` class they return, and one warning
class. Every function takes the path of a PDF file, as a `str` or a
`pathlib.Path`.

Every function releases Python's GIL while it reads, so you can call them from
several threads at the same time.

## `extract_text`

```python
ozaru.extract_text(path: str | os.PathLike[str]) -> str
```

Returns the text of the whole document. Pages are separated by a form feed
character (`\f`).

**Parameters**

| Name   | Type                        | Description          |
|--------|-----------------------------|----------------------|
| `path` | `str` or `os.PathLike[str]` | Path to the PDF file |

**Returns**

`str`: the text of the document. A page that could not be read is an empty
string.

All the file errors are subclasses of `OSError` and have `errno` and `filename`
attributes, like the ones Python's `open()` raises.

**Warns**

`PageWarning`, once for each page that could not be read.

**Raises**

| Exception            | When it happens                                         |
|----------------------|---------------------------------------------------------|
| `FileNotFoundError`  | The path does not exist.                                |
| `PermissionError`    | You don't have permission to read the file.             |
| `IsADirectoryError`  | The path is a folder, not a file.                       |
| `NotADirectoryError` | Part of the path is a file used as if it were a folder. |
| `OSError`            | The file could not be read for another reason.          |
| `ValueError`         | The file is not a valid PDF, it is protected with a password, or the path contains a null byte. |
| `TypeError`          | `path` is not a valid path type.                        |

## `page_count`

```python
ozaru.page_count(path: str | os.PathLike[str]) -> int
```

Returns the number of pages in the document. It also works on PDFs protected with
a password, except the rare ones whose list of pages is encrypted too.

The pages are counted by walking the document's page tree, so a damaged file
whose declared page count is wrong still gives the pages it really has.

**Parameters**

| Name   | Type                        | Description          |
|--------|-----------------------------|----------------------|
| `path` | `str` or `os.PathLike[str]` | Path to the PDF file |

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
| `ValueError`         | The file is not a valid PDF, it is too damaged to find its pages, its list of pages needs the password, or the path contains a null byte. |
| `TypeError`          | `path` is not a valid path type.                        |

## `metadata`

```python
ozaru.metadata(path: str | os.PathLike[str]) -> ozaru.Metadata
```

Returns the document information of the PDF: its title, author and dates. See
[Read the metadata](../tutorial/read-metadata.md).

**Parameters**

| Name   | Type                        | Description          |
|--------|-----------------------------|----------------------|
| `path` | `str` or `os.PathLike[str]` | Path to the PDF file |

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
| `ValueError`         | The file is not a valid PDF, it is protected with a password, or the path contains a null byte. |
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

## `PageWarning`

```python
class ozaru.PageWarning(UserWarning)
```

Emitted by `extract_text` for each page that could not be read. The other pages
are still returned. See [Handle errors](../tutorial/handle-errors.md#pages-that-cant-be-read).

**Attributes**

| Name     | Type  | Description                                   |
|----------|-------|-----------------------------------------------|
| `page`   | `int` | Number of the page that failed, starting at 1 |
| `reason` | `str` | Why the page could not be read                |
