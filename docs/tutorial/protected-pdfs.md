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

Passwords with accents or other special characters work as you type them. ozaru
encodes them the way each kind of PDF encryption expects, and normalizes them as
the standard says, so for example a no-break space copied from a web page counts
as a normal space.

## Protect a PDF

`ozaru.encrypt` saves a copy of a PDF protected with passwords, using AES-256:

```python
import ozaru

ozaru.encrypt(
    "report.pdf",
    "protected.pdf",
    user_password="open sesame",
    owner_password="boss",
    permissions=["print", "copy"],
)
```

Whoever knows the user password can open the file, but only to do what
`permissions` allows. The owner password opens it with everything allowed. With
no `user_password`, anyone can open the file, and the permissions are all the
protection: viewers respect them, but they don't stop a determined person. If you
pass no `owner_password`, the user password is used for both.

`permissions` is a list of what to allow: `print`, `modify`, `copy`, `annotate`,
`fill_forms`, `accessibility`, `assemble` and `print_high_quality`. Leave it out to
allow everything. `ozaru.permissions` tells what a PDF allows, as a list of the same
names, or `None` if it isn't protected.

`encrypt_metadata=False` leaves the XMP metadata readable, as some archives need.
To protect a PDF that already has a password, pass it as `password`; the new passwords
replace the old ones. The result is a PDF 2.0 file written whole: ozaru can't add
to an encrypted file with `incremental=True`.

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
