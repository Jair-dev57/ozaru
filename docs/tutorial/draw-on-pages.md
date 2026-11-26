# Draw on pages and add watermarks

`ozaru.draw` adds text, lines and shapes to pages of a PDF, over what they already have, and saves
the result to another file.

```python
import ozaru

ozaru.draw("invoice.pdf", "invoice-paid.pdf", [
    ozaru.Rect(1, 380, 40, 520, 90, stroke="green", width=3),
    ozaru.Text(1, 450, 72, "PAID", font="Helvetica-Bold", size=28, color="green",
               align="center", valign="middle", rotation=10),
    ozaru.Line(1, 50, 120, 545, 120, color="gray", dash=(4, 2)),
])
```

Positions are measured **from the top left corner of the page**, in points (72 to an inch), as the
boxes of words are, so the box of a word from `extract_words` or of a match from `search` can be
used directly to draw a frame around it:

```python
for match in ozaru.search("invoice.pdf", "total"):
    x0, y0, x1, y1 = match.bbox
    items.append(ozaru.Rect(match.page, x0 - 2, y0 - 2, x1 + 2, y1 + 2, stroke="red"))
```

## Watermarks

```python
ozaru.add_watermark("contract.pdf", "contract-draft.pdf", "DRAFT", color="red", opacity=0.25)
```

or from the terminal: `uvx ozaru watermark contract.pdf DRAFT -o contract-draft.pdf --color red`.

The text is centered on each page, turned 45 degrees, and as wide as most of the diagonal; give
`size`, `rotation` or `pages` to change that.

## Fonts

By default the fonts are the standard 14 (Helvetica, Times and Courier with their bold and italic,
Symbol and ZapfDingbats): every reader has them, so they add nothing to the file. They draw the
letters of Western Europe (`ñ`, `ü`, `é`, `€`...).

For any other alphabet, or a typeface of your own, give the path of a TrueType file (`.ttf`) as the
font:

```python
ozaru.draw("in.pdf", "out.pdf", [
    ozaru.Text(1, 72, 100, "Привет, мир — Ωμέγα — Łódź", font="fonts/NotoSans-Regular.ttf", size=24),
])
```

The PDF gets **only the glyphs the text uses** (a subset of the font), so it stays small, and the text
can still be selected, searched and extracted. Every text that uses the font shares it. The font
needs the characters you draw (a character it doesn't have is an error that names it), `ozaru.text_width(text, font="…ttf", size=…)`
measures a text in it, and `add_watermark` takes it too (`font="…ttf"`). From the terminal:
`uvx ozaru add-text in.pdf 1 72 100 "Łódź" -o out.pdf --font fonts/NotoSans-Regular.ttf`.

Some limits: OpenType fonts with PostScript outlines (many `.otf` files) aren't supported yet, only
TrueType outlines (`.ttf`; variable fonts draw their default style); and a font whose license forbids
embedding or subsetting is refused with an error. Text is drawn one glyph after the other:
there is no kerning, ligatures or shaping of Arabic or Indic scripts.

Next: [protected PDFs](protected-pdfs.md).
