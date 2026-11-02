import warnings

import ozaru

warnings.simplefilter("error", ozaru.PageWarning)

try:
    text = ozaru.extract_text("damaged-page.pdf")
except ozaru.PageWarning as error:
    print(f"Stopped: page {error.page} could not be read")
