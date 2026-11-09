# Extract text

`ozaru.extract_text` returns the text of the whole document as a single `str`.

All the examples in this tutorial use the
[sample report](../assets/report.pdf){ download }.

## Search the text

The result is a normal Python string, so you can use everything you already know.
For example, `in` tells you whether a word appears in the document:

```python
--8<-- "docs_src/extract_text.py"
```

Output:

```text
--8<-- "docs_src/extract_text.out"
```

Accents and other non-English characters are preserved, so `São Paulo` is found
exactly as it was written.

## Save the text to a file

To keep the text, write it to a file:

```python
--8<-- "docs_src/save_text.py"
```

Output:

```text
--8<-- "docs_src/save_text.out"
```

!!! tip
    Always pass `encoding="utf-8"` when you save text. Otherwise Python uses the
    default encoding of your system, and some characters may fail to save.

## How the text is laid out

Each line of the page is a line of the text, without spaces at its start or
end, and an empty line separates paragraphs. ozaru adds the spaces that many
PDFs leave out, where they only leave a gap between words, and keeps the cells
of a table row on one line. Text
that viewers don't show, because it's outside the page, is left out. The
[API reference](../reference/api.md#extract_text) has the details.

## Scanned documents

ozaru reads the text stored inside the PDF. A scanned document is really a set of
images, so it may return little or no text. See
[Limitations](../reference/limitations.md).

Next: [count pages](count-pages.md).
