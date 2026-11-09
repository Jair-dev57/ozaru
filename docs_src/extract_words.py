import ozaru

for word in ozaru.extract_words("report.pdf"):
    if word.text == "12%":
        print(f"{word.text} is on page {word.page}, line {word.line}")
        print(f"{word.x0:.0f} points from the left, {word.y0:.0f} from the top")
