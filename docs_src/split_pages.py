import ozaru

text = ozaru.extract_text("report.pdf")
pages = text.split("\f")

for number, page in enumerate(pages, start=1):
    title = page.splitlines()[0]
    print(f"Page {number}: {title}")
