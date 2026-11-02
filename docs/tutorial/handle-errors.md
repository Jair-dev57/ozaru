# Handle errors

Files go missing and not every file is a valid PDF. ozaru reports these problems
with standard Python exceptions:

| Exception            | When it happens                                              |
|----------------------|--------------------------------------------------------------|
| `FileNotFoundError`  | The path does not exist.                                     |
| `PermissionError`    | You don't have permission to read the file.                  |
| `IsADirectoryError`  | The path is a folder, not a file.                            |
| `NotADirectoryError` | Part of the path is a file used as if it were a folder.      |
| `OSError`            | The file could not be read for another reason.               |
| `ValueError`         | The file is not a valid PDF, or the path contains a null byte. |

`extract_text` also raises `ValueError` for a PDF that needs a password.

Catch them with `try` and `except`:

```python
--8<-- "docs_src/handle_errors.py"
```

Output:

```text
--8<-- "docs_src/handle_errors.out"
```

!!! tip
    The text of an exception may change between releases. Rely on the exception
    **type**, as this example does, and not on the message.

## File errors

`FileNotFoundError`, `PermissionError`, `IsADirectoryError` and
`NotADirectoryError` are the same exceptions Python's `open()` raises. They are all
subclasses of `OSError`, so one `except OSError` catches any problem with the file
itself, and `error.filename` tells you which path failed:

```python
--8<-- "docs_src/file_errors.py"
```

Output:

```text
--8<-- "docs_src/file_errors.out"
```

## Pages that can't be read

Sometimes a PDF opens but one of its pages is damaged. ozaru doesn't throw away
the whole document for that: `extract_text` returns the text of every page it
could read, leaves the damaged page empty, and emits an `ozaru.PageWarning` for it.

The page stays in the result as an empty string, so `split("\f")` still gives one
item per page and the page numbers don't shift.

Download the [damaged report](../assets/damaged-page.pdf){ download }, whose
third page is broken, and catch the warnings with the standard
[`warnings`](https://docs.python.org/3/library/warnings.html) module:

```python
--8<-- "docs_src/page_warnings.py"
```

Output:

```text
--8<-- "docs_src/page_warnings.out"
```

Each warning has two attributes:

| Attribute | Type  | Description                                  |
|-----------|-------|----------------------------------------------|
| `page`    | `int` | Number of the page that failed, starting at 1 |
| `reason`  | `str` | Why the page could not be read               |

If you don't catch them, Python prints the warnings to the terminal and your
program keeps running.

### Stop at the first damaged page

When a partial result is not acceptable, turn the warning into an error. Then
`extract_text` raises `ozaru.PageWarning` instead of returning:

```python
--8<-- "docs_src/page_warnings_strict.py"
```

Output:

```text
--8<-- "docs_src/page_warnings_strict.out"
```

Next: [process a folder](process-a-folder.md).
