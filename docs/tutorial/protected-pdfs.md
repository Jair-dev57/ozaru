# Protected PDFs

A PDF can be protected with two passwords:

- the **user password**, which a viewer asks for to open it;
- the **owner password**, which also opens it, and sets what may be done with
  it, such as printing.

Every function of ozaru takes either one as the `password` keyword argument:

```python
import ozaru

text = ozaru.extract_text("protected.pdf", password="1234")
pages = ozaru.page_count("protected.pdf", password="1234")
info = ozaru.metadata("protected.pdf", password="1234")
```

The password is ignored if the PDF isn't protected, so you can pass it to every
file of a folder.

## When the password is missing or wrong

ozaru raises `ozaru.PasswordError`, with a message that says which:

```python
import ozaru

try:
    text = ozaru.extract_text("protected.pdf", password="wrong")
except ozaru.PasswordError as error:
    print(f"Can't open it: {error}")
```

`PasswordError` is a subclass of `ValueError`, so code that catches `ValueError`
keeps working.

## What works without the password

Many protected PDFs only encrypt their text, not the list of their pages, so
`page_count` and `page_info` usually work without the password. `extract_text`
and `metadata` always need it, since the text and the metadata are encrypted.

Next: [read page by page](read-page-by-page.md).
