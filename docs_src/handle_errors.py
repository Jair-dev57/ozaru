from pathlib import Path

import ozaru


def read_pdf(path):
    try:
        return ozaru.extract_text(path)
    except FileNotFoundError:
        print(f"{path}: file not found")
    except ValueError:
        print(f"{path}: not a readable PDF")
    return None


Path("notes.txt").write_text("This is not a PDF.", encoding="utf-8")

read_pdf("missing.pdf")
read_pdf("notes.txt")

text = read_pdf("report.pdf")
print(f"report.pdf: {len(text)} characters")
