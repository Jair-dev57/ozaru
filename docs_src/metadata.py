import ozaru

info = ozaru.metadata("report.pdf")

print(f"Title: {info.title}")
print(f"Author: {info.author}")
print(f"Created: {info.creation_date:%Y-%m-%d}")
