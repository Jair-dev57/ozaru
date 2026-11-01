from pathlib import Path

import ozaru

Path("broken.pdf").write_text("This is not a PDF.", encoding="utf-8")

for pdf in sorted(Path(".").glob("*.pdf")):
    try:
        pages = ozaru.page_count(pdf)
    except ValueError:
        print(f"{pdf.name}: skipped")
        continue

    print(f"{pdf.name}: {pages} pages")
