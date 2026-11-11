import ozaru

for image in ozaru.extract_images("brochure.pdf"):
    name = f"page{image.page}-{image.number}.{image.format}"
    image.save(name)
    print(f"{name}: {image.width} x {image.height} pixels, {image.x0:.0f} points from the left")
