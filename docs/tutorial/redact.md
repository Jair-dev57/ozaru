# Redact: remove text and images for good

Drawing a black rectangle over a secret hides it, but the text is still in the file: anyone can select
it, search it or extract it. `ozaru.redact` **takes it out**.

```python
import ozaru

# every place a text is found
count = ozaru.redact_text("contract.pdf", "contract-redacted.pdf", "ID 1234567")

# or the areas you choose (points from the top left of the page, as the boxes of words are)
report = ozaru.redact("contract.pdf", "contract-redacted.pdf", [
    ozaru.Redaction(1, (72, 200, 300, 220)),
    ozaru.Redaction(2, (72, 100, 200, 130), fill="white"),
])
print(report)   # RedactionReport(glyphs=31, images_blanked=1, ...)
```

What happens under each area:

- **Text**: the letters are removed from the page's content, one by one. The rest of the line stays
  exactly where it was (the removed letters leave their space), so nothing around it moves. The
  letters aren't in the saved file: not in the page content, not in a hidden layer. A letter goes if
  the center of its box is in the area or a quarter of it is.
- **Images**: by default (`images="pixels"`) the pixels under the area are blanked and the rest of the
  picture stays (it is written again as gray or RGB, so spot and CMYK colors may shift a little). `images="remove"` takes out whole images that touch the area, and `"keep"` leaves
  them (only the box hides them, and the data is still in the file).
- **Lines and shapes**: those that are all inside the area are removed (`graphics="covered"`), or those
  that touch it (`"touched"`), or none (`"keep"`).
- **Annotations** under the area are removed (turn it off with `annotations=False`).
- A **box** of `fill` (black by default; `fill=None` draws nothing) covers the area.

It works inside the reusable drawings (form XObjects) a page uses, which are copied and rewritten
for that page only, so another page that shares them is left as it was.

`ozaru.redact_text` takes the same options and finds the text for you as `search` does (a match
over several lines is several boxes). From the terminal:
`uvx ozaru redact contract.pdf "ID 1234567" -o redacted.pdf`.

## What it does not reach

A PDF holds text in other places, which redaction doesn't clean: **bookmarks**, **form fields** and
their values (a warning says how many are under an area), the structure of **tagged** files, the
**metadata** (title, author, XMP) and **attachments**. The glyphs of Type 3 fonts and what
tiling patterns paint aren't reached either. Check the result with `extract_text`, and clean those
places if they matter. If a page is so complex that its content can't be checked to the end,
`redact` raises an error and saves nothing, rather than saving a page that is only partly redacted.

The result is never saved incrementally, since that would keep the old content in the file.

Next: [create fields and flatten forms](create-and-flatten-forms.md).
