import ozaru

for annotation in ozaru.annotations("annotated.pdf"):
    if annotation.uri:
        print(f"page {annotation.page}: link to {annotation.uri}")
    elif annotation.target_page:
        print(f"page {annotation.page}: link to page {annotation.target_page}")
    elif annotation.contents:
        print(f"page {annotation.page}: {annotation.subtype} {annotation.contents!r}")
