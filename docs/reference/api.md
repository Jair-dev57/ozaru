# API

ozaru has two functions and one warning class. Both functions take the path of a PDF file, as a `str` or a
`pathlib.Path`.

Both functions release Python's GIL while they read, so you can call them from
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
