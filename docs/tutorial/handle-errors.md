# Handle errors

Files go missing and not every file is a valid PDF. ozaru reports these problems
with standard Python exceptions:

| Exception           | When it happens                                    |
|---------------------|----------------------------------------------------|
| `FileNotFoundError` | The path does not exist.                           |
| `ValueError`        | The file could not be read as a PDF, or `extract_text` was given a password-protected PDF. |

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
