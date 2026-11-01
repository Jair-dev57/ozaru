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

Next: [process a folder](process-a-folder.md).
