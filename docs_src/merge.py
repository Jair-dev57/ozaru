import ozaru

ozaru.merge(["report.pdf", "brochure.pdf"], "joined.pdf")

print(ozaru.page_count("joined.pdf"), "pages")
for image in ozaru.extract_images("joined.pdf"):
    print(f"Image on page {image.page}: {image.width} x {image.height}")
