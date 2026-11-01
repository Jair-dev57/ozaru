# First steps

In this page you will read your first PDF. It takes about a minute.

## Get a PDF

Download the [sample report](assets/report.pdf){ download } and save it as
`report.pdf`. It has three short pages, and every example in this documentation
uses it, so your output will match what you see here.

## Write the code

Create a file called `main.py` in the same folder:

```python title="main.py"
--8<-- "docs_src/quickstart.py"
```

## Run it

```bash
python main.py
```

You should see:

```text
--8<-- "docs_src/quickstart.out"
```

## What just happened

1. `ozaru.extract_text` opened `report.pdf` and returned all of its text as a `str`.
2. `ozaru.page_count` returned the number of pages as an `int`.
3. From there it's regular Python: `splitlines()` cut the text into lines, and
   `[0]` picked the first one.

!!! info
    ozaru returns plain Python values. You don't have to open or close anything,
    because ozaru takes care of the file for you.

Next: learn more about [extracting text](tutorial/extract-text.md).
