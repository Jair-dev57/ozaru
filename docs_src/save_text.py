from pathlib import Path

import ozaru

text = ozaru.extract_text("report.pdf")
Path("report.txt").write_text(text, encoding="utf-8")

print("Saved report.txt")
