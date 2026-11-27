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

## Notes, links, shapes and text boxes

`ozaru.annotate` adds more than marks on text. Positions are from the top left of the page, and
colors, `opacity`, `note`, `author`, `subject` and `date` work for all of them:

```python
ozaru.annotate("report.pdf", "report-reviewed.pdf", [
    ozaru.Note(1, 480, 60, "Check this total", author="Ana"),             # a sticky note
    ozaru.Link(1, (72, 700, 200, 716), uri="https://example.com"),         # a web link
    ozaru.Link(1, (72, 720, 200, 736), to_page=5, border=1),               # a link to page 5, framed
    ozaru.Box(1, (300, 400, 420, 460), fill="#ffeeaa", color="orange"),    # a rectangle
    ozaru.Oval(1, (440, 400, 540, 460), color="green"),
    ozaru.Arrow(1, (320, 520), (520, 480), color="purple", width=2.5),     # a line with an arrowhead
    ozaru.Ink(1, [(100, 520), (120, 490), (140, 520)], color="blue"),      # a freehand drawing
    ozaru.TextBox(1, (100, 560, 260, 640), "Text wrapped in a box", size=11, fill="#ffffcc", border=1),
])
```

A `Note` has its top left corner at `(x, y)`, and its `text` opens when it is clicked. A `Link` goes to
a `uri` or to a `to_page` (from 1). A `TextBox` writes in Helvetica, wrapped to the box (the letters of
Western Europe), and its `color` is the color of the text. From the terminal: `ozaru add-note`
and `ozaru add-link`.

## Remove and edit annotations

```python
# every highlight and underline, on all pages
removed = ozaru.delete_annotations("in.pdf", "out.pdf", subtypes=["Highlight", "Underline"])

# everything Ana wrote on page 2, or every annotation with a word in its comment
ozaru.delete_annotations("in.pdf", "out.pdf", pages=[2], author="Ana")
ozaru.delete_annotations("in.pdf", "out.pdf", contains="draft")

# change the comment and hide them (True) or show them again (False)
ozaru.update_annotations("in.pdf", "out.pdf", {"contents": "reviewed", "hidden": True}, author="Ana")
```

The annotations reached are those that match **every** argument given (`pages`, `subtypes`, `name`,
`author`, `contains`); with none, all of them. Both return how many they reached. Form fields are
never removed. Changing `contents`, `author` or `subject` doesn't redraw the annotation, so a color
can't be changed: remove the annotation and add another. The command is
`ozaru delete-annotations in.pdf -o out.pdf --subtype Highlight`.

Next: [read and fill forms](fill-forms.md).
