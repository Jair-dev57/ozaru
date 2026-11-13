import ozaru

# Turn the second page a quarter turn clockwise, into a new file.
ozaru.rotate("report.pdf", "rotated.pdf", 90, pages=[2])

for page in ozaru.page_info("rotated.pdf"):
    print(f"Page {page.number}: turned {page.rotation} degrees, {page.width:.0f} x {page.height:.0f}")
