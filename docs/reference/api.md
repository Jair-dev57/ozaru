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

**Warns**

`PageWarning`, once for each page that could not be read.

**Raises**

| Exception           | When it happens                                |
|---------------------|------------------------------------------------|
| `FileNotFoundError` | The path does not exist.                       |
| `ValueError`        | The file could not be opened or read as a PDF, or it is protected with a password. |
| `TypeError`         | `path` is not a valid path type.               |

## `page_count`

```python
ozaru.page_count(path: str | os.PathLike[str]) -> int
```

Returns the number of pages in the document. It also works on PDFs protected with
a password.

**Parameters**

| Name   | Type                        | Description          |
|--------|-----------------------------|----------------------|
| `path` | `str` or `os.PathLike[str]` | Path to the PDF file |

**Returns**

`int`: the number of pages.

**Raises**

| Exception           | When it happens                                |
|---------------------|------------------------------------------------|
| `FileNotFoundError` | The path does not exist.                       |
| `ValueError`        | The file could not be opened or read as a PDF. |
| `TypeError`         | `path` is not a valid path type.               |

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
