import ozaru

text = ozaru.extract_text("report.pdf")
pages = ozaru.page_count("report.pdf")

print(f"Pages: {pages}")
print(f"Title: {text.splitlines()[0]}")
