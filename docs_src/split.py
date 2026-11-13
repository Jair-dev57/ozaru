import ozaru

for path in ozaru.split("report.pdf", "pages"):
    print(path.name, ozaru.page_count(path), "page")
