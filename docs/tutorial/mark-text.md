# Highlight, underline and strike out text

`ozaru.highlight` marks every place a text is found, the way a highlighter does, and saves the
result to another file:

```python
import ozaru

count = ozaru.highlight("contract.pdf", "contract-marked.pdf", "termination",
                        note="check the notice period", author="Ana")
print(count, "places marked")
```

By default it is a yellow highlight; `kind="underline"`, `"strikeout"` or `"squiggly"` draw a line
under, through or a wavy line under the text, and `color`, `opacity`, `pages` and `case_sensitive`
work as you expect. `note` is the comment that opens when the mark is clicked in a viewer. From the
terminal: `uvx ozaru highlight contract.pdf termination -o marked.pdf --kind underline --color red`.

## Marks at places you choose

`ozaru.annotate` takes the marks themselves, and their boxes are measured **from the top left corner of
the page**, as the boxes of words and of search results are, so those can be given as they are:

```python
items = []
for match in ozaru.search("contract.pdf", "termination"):
    items.append(ozaru.Highlight(match.page, match.boxes, color="#ffe066"))   # one box for each line
for word in ozaru.extract_words("contract.pdf"):
    if word.text.isupper():
        items.append(ozaru.Underline(word.page, word.bbox))
ozaru.annotate("contract.pdf", "marked.pdf", items)
```

The marks are `Highlight`, `Underline`, `StrikeOut` and `Squiggly`, each with `page` and `boxes` (one
box, or a list), and optional `color`, `opacity`, `note`, `author`, `subject` and `date` (now by
default). Each mark carries its own drawing, so every viewer shows it the same way, and it can be
removed or edited from the viewer. A highlight multiplies its color with the page, so the text under
it stays dark.

Pass `incremental=True` to keep the original bytes of the file and add the marks at its end (useful
for files that are signed). The marks you add are listed by [`ozaru.annotations`](read-annotations.md).

Next: [read and fill forms](fill-forms.md).
