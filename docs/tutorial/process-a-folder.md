# Process a folder

Now put it all together. This script goes through every PDF in a folder, reports
its page count, and keeps going if one file is broken:

```python
--8<-- "docs_src/process_folder.py"
```

Output:

```text
--8<-- "docs_src/process_folder.out"
```

Two details matter here:

- `continue` moves on to the next file, so one broken PDF doesn't stop the whole
  run.
- `pdf` is a `pathlib.Path`, and ozaru accepts it directly, just like a `str`.

## Use threads for large folders

ozaru releases Python's GIL while it reads a PDF, so several threads can work at
the same time. `ThreadPoolExecutor` is the simplest way to do that:

```python
--8<-- "docs_src/process_folder_threads.py"
```

Output:

```text
--8<-- "docs_src/process_folder_threads.out"
```

`executor.map` returns the results in the same order as `pdfs`, so `zip` can pair
each file with its text.

That's the end of the tutorial. From here, the
[API reference](../reference/api.md) has every detail in one place.
