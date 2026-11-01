from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import ozaru

pdfs = sorted(Path(".").glob("*.pdf"))

with ThreadPoolExecutor() as executor:
    texts = executor.map(ozaru.extract_text, pdfs)

    for pdf, text in zip(pdfs, texts):
        print(f"{pdf.name}: {len(text)} characters")
