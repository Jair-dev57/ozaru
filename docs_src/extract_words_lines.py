import ozaru

lines = {}
for word in ozaru.extract_words("report.pdf"):
    if word.page == 1:
        lines.setdefault(word.line, []).append(word.text)

for number, words in lines.items():
    print(number, words)
