# Bookmarks, page labels and attached files

Long documents have an outline, pages numbered `i`, `ii`, `1`, and sometimes files
attached to them. ozaru reads all of that.

```python
import ozaru

for bookmark in ozaru.bookmarks("book.pdf"):
    print("  " * (bookmark.level - 1) + bookmark.title, bookmark.page)

print(ozaru.page_labels("book.pdf"))      # ['i', 'ii', '1', '2', ...]

for attachment in ozaru.attachments("book.pdf"):
    print(attachment.name, attachment.mime_type, len(attachment.data))
```

`bookmark.page` counts from 1, as every page number in ozaru does, and is `None` for a
bookmark that opens an address (`bookmark.uri`) instead of going to a page.

With a `Document` they are `pdf.bookmarks`, `pdf.page_labels`, `pdf.attachments` and, for a
page, `page.label`.

!!! warning "Don't trust the file name of an attachment"
    The name comes from the PDF, so it can be `../../etc/passwd`. Save the file under a name
    you choose, or use only the last part of its name. From the terminal,
    `uvx ozaru attachments book.pdf -o folder` does that.

Next: [read page by page](read-page-by-page.md).
