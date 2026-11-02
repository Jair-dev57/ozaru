from pathlib import Path

import ozaru

Path("reports").mkdir()

for path in ["missing.pdf", "reports"]:
    try:
        ozaru.page_count(path)
    except OSError as error:
        print(f"{type(error).__name__}: {error.filename}")
