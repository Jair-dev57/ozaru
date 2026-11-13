import ozaru

# The last page first, then the first one.
ozaru.extract_pages("report.pdf", "summary.pdf", [3, 1])

print(ozaru.page_count("summary.pdf"), "pages")
