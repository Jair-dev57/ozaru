import ozaru

for page in ozaru.page_info("report.pdf"):
    shape = "landscape" if page.width > page.height else "portrait"
    print(f"Page {page.number}: {page.width:.0f} x {page.height:.0f} points, {shape}")
