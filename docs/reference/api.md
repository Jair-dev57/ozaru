# API

ozaru has two functions. Both take the path of a PDF file, as a `str` or a
`pathlib.Path`.

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

`str`: the text of the document.

**Raises**

| Exception           | When it happens                                |
|---------------------|------------------------------------------------|
| `FileNotFoundError` | The path does not exist.                       |
| `ValueError`        | The file could not be opened or read as a PDF. |
| `TypeError`         | `path` is not a valid path type.               |

## `page_count`

```python
ozaru.page_count(path: str | os.PathLike[str]) -> int
```

Returns the number of pages in the document.

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
