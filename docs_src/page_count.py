import ozaru

pages = ozaru.page_count("report.pdf")

if pages > 2:
    print(f"Long document: {pages} pages")
else:
    print(f"Short document: {pages} pages")
