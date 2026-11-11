# Find words on the page

`ozaru.extract_words` returns every word of a PDF as a `Word`, with the page,
block and line it is on and the box around it. It is how you find where something is
written, not only whether it is there.

## Where is a word?

```python
--8<-- "docs_src/extract_words.py"
```

Output:

```text
--8<-- "docs_src/extract_words.out"
```

`x0`, `y0`, `x1` and `y1` are the sides of the box, in points (1/72 of an inch),
measured from the top left corner of the page: `x0` is its left side and `x1`
its right side, `y0` its top and `y1` its bottom. `bbox` gives the four as a
tuple. The box is as wide as the word and as tall as its font, from the lowest
point of letters like `g` to the top of the tallest ones.

## Words line by line

The words come in the order of the text: page by page and, on each page, line by
line, as `extract_text` gives them. `line` counts the lines of each page from 1,
so you can put them back together:

```python
--8<-- "docs_src/extract_words_lines.py"
```

Output:

```text
--8<-- "docs_src/extract_words_lines.out"
```

A word ends where there is a space. Like `extract_text`, ozaru finds the spaces
that the PDF leaves out, where it only leaves a gap between words, so
`" ".join(word.text for word in words)` gives the text of a page with single
spaces.

## Blocks

`ozaru.extract_blocks` puts the lines together into blocks: paragraphs,
headings and the pieces of a page that stand apart, like the name and the date
of a header. Each `Block` has its page, its `number` on the page, its `text`,
with a line feed between its lines, and its box:

```python
--8<-- "docs_src/extract_blocks.py"
```

Output:

```text
--8<-- "docs_src/extract_blocks.out"
```

Every `Word` has the `block` it is in, so you can find the words of a block,
or the block of a word.

## Rotated pages

The boxes are measured on the page before it is rotated, as it is stored in
the PDF. On a page with a `rotation` of 90, 180 or 270 (see
[page sizes](page-sizes.md)), a viewer shows it turned, and the boxes turn with
it.

Damaged pages are reported with a `PageWarning`, as with `extract_text`, and the
words of the other pages are still returned.

Next: [extract images](extract-images.md).
