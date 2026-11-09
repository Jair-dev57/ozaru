import ozaru

for block in ozaru.extract_blocks("report.pdf"):
    if block.page == 2:
        print(f"Block {block.number}, {block.y0:.0f} points from the top:")
        print(block.text)
