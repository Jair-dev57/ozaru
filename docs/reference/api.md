# API

ozaru has two functions. Both take the path of a PDF file.

## `extract_text`

```python
ozaru.extract_text(path: str) -> str
```

Returns the text of the whole document. Pages are separated by a form feed
character (`\f`).

**Parameters**

| Name   | Type  | Description          |
|--------|-------|----------------------|
| `path` | `str` | Path to the PDF file |

**Returns**

`str`: the text of the document.

**Raises**

| Exception           | When it happens                                |
|---------------------|------------------------------------------------|
| `FileNotFoundError` | The path does not exist.                       |
| `ValueError`        | The file could not be opened or read as a PDF. |
| `TypeError`         | `path` is not a `str`.                         |

## `page_count`

```python
ozaru.page_count(path: str) -> int
```

Returns the number of pages in the document.

**Parameters**

| Name   | Type  | Description          |
|--------|-------|----------------------|
| `path` | `str` | Path to the PDF file |

**Returns**

`int`: the number of pages.

**Raises**

| Exception           | When it happens                                |
|---------------------|------------------------------------------------|
| `FileNotFoundError` | The path does not exist.                       |
| `ValueError`        | The file could not be opened or read as a PDF. |
| `TypeError`         | `path` is not a `str`.                         |
