# Compare two PDFs

`ozaru.compare` tells what changed between a PDF and another version of it:

```python
import ozaru

result = ozaru.compare("contract-v1.pdf", "contract-v2.pdf")
print(result)            # a summary for people
print(result.identical)  # False
```

```
Pages: 11 same, 1 changed, 1 added, 0 removed
  page 4
    '30 days' -> '45 days'
    - 'Termination fee applies.'
  page 12 of the second PDF was added
Information Title: 'Contract' -> 'Contract (draft 2)'
```

## What it compares

- **Pages**, paired by what they say: a page added or removed in the middle doesn't make the pages after it look
  changed. A page rewritten between pages that stay is the same page, changed. Documents of more than 2,000
  pages are paired by number.
- **Words** of each pair, one by one, so text that wraps differently isn't a change. Each `TextChange` has what
  was `removed` and what was `added`, with their boxes (`removed_boxes` on the page of the first PDF, `added_boxes`
  on the page of the second), in points from the top left, as the boxes of words are.
- **Information** (title, author, dates...), **bookmarks**, **attachments** and **form field values**.

```python
for page in result.changed_pages:
    print(page.status, page.page_a, page.page_b)    # "changed", "added" or "removed"
    for change in page.changes:
        print(change.removed, "->", change.added)
```

## Changes you can see but are not words

Images, drawings and colors don't change the words. With `visual=True` the pages that stay are also drawn and compared
as pictures:

```python
result = ozaru.compare("a.pdf", "b.pdf", visual=True)
for page in result.pages:
    if page.difference:                      # the share of the page that looks different
        print(page.page_a, f"{page.difference:.1%}", page.regions)   # and where, as boxes
```

A pixel differs if a color changes by more than `threshold` (32 of 255 by default), and `scale` is the size of the
drawings (1 is 72 dots per inch). Pages that draw exactly the same, as their contents say, are not drawn, so
comparing a file with a new version of it that changed a few pages is fast. A page of another size is changed
(`size_changed`) and isn't compared as a picture.

`ozaru.diff_page("a.pdf", 3, "b.pdf", 3)` returns a PNG of how page 3 differs: what both have is faded, what only
the first has is red and what only the second has is green.

## See the differences in the PDFs

```python
result.highlight("a-marked.pdf", "b-marked.pdf")
```

saves copies of both with the removed words highlighted in red in the first, the added words in green in the second (with
a note saying what), and a frame where a page only looks different. From the terminal:

```
uvx ozaru compare old.pdf new.pdf                     # prints the summary; exit status 1 if they differ, as diff does
uvx ozaru compare old.pdf new.pdf --visual --json     # as JSON, also looking at pictures
uvx ozaru compare old.pdf new.pdf --highlight diff    # saves diff-old.pdf and diff-new.pdf
```

Next: [sanitize a PDF](sanitize.md).
