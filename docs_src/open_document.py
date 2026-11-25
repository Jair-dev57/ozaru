import ozaru

pdf = ozaru.open("report.pdf")

print(len(pdf), "pages")
for page in pdf:
    print(f"Page {page.number}: {page.text.splitlines()[0]}")
