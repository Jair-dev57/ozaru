# Read page by page

`extract_text` returns the whole document in one string, but you can still work
with each page.

ozaru separates pages with a form feed character, written `\f` in Python. Split
on it to get a list with one item per page:

```python hl_lines="4"
--8<-- "docs_src/split_pages.py"
```

Output:

```text
--8<-- "docs_src/split_pages.out"
```

!!! note "Use `split`, not `splitlines`"
    `splitlines()` also treats `\f` as a line break, so it can't tell you where a
    page ends. Use `split("\f")` to get pages, and `splitlines()` on each page to
    get its lines.

Next: [handle errors](handle-errors.md).
