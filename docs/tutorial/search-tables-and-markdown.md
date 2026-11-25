# Search, tables and Markdown

## Find text and where it is

```python
import ozaru

for match in ozaru.search("report.pdf", "net income"):
    print(match.page, match.bbox, match.text)
```

`match.bbox` is measured from the top left of the page, as the boxes of words are, so it
can be used to draw over the page or to cut an image of the place with `render_page`.

## Fonts, sizes and colors

```python
for span in ozaru.extract_spans("report.pdf"):
    if span.bold and span.size > 14:
        print(span.page, span.text)
```

## Tables

```python
for table in ozaru.extract_tables("report.pdf"):
    print(table.page, len(table.rows), "rows")
    for row in table.rows:
        print(row)
```

Tables are found from the lines a page draws and, where there are none, from text that lines up in
columns. Use `strategy="lines"` to keep only the ones with lines, which are the most reliable.

## Markdown, for language models

```python
text = ozaru.to_markdown("report.pdf", page_markers=True)
```

The headings, bold and italic, lists and tables come out as Markdown, with a comment before each
page. It is a good input for a model: it keeps the structure that `extract_text` loses.

From the terminal: `uvx ozaru markdown report.pdf`.

Next: [bookmarks and attachments](bookmarks-and-attachments.md).
