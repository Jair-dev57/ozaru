from os import PathLike

def extract_text(path: str | PathLike[str]) -> str:
    """Return all the text of the PDF."""
    ...

def page_count(path: str | PathLike[str]) -> int:
    """Return the number of pages of the PDF."""
    ...
