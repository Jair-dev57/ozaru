import warnings

import ozaru

with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always", ozaru.PageWarning)
    text = ozaru.extract_text("damaged-page.pdf")

for warning in caught:
    print(f"Page {warning.message.page} could not be read")

for number, page in enumerate(text.split("\f"), start=1):
    print(f"Page {number}: {len(page)} characters")
