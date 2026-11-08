from datetime import datetime
from os import PathLike

class PageWarning(UserWarning):
    """Warns that the text of one page could not be extracted."""

    page: int
    reason: str

class Metadata:
    """The document information of a PDF: its title, author and dates.

    Each attribute is `None` when the PDF doesn't have it.
    """

    @property
    def title(self) -> str | None: ...
    @property
    def author(self) -> str | None: ...
    @property
    def subject(self) -> str | None: ...
    @property
    def keywords(self) -> str | None: ...
    @property
    def creator(self) -> str | None:
        """The application that created the original document."""
    @property
    def producer(self) -> str | None:
        """The application that converted it to PDF."""
    @property
    def creation_date(self) -> datetime | None:
        """Aware when the PDF gives its time zone, naive otherwise."""
    @property
    def modification_date(self) -> datetime | None:
        """Aware when the PDF gives its time zone, naive otherwise."""

def extract_text(path: str | PathLike[str]) -> str:
    """Return all the text of the PDF."""
    ...

def metadata(path: str | PathLike[str]) -> Metadata:
    """Return the document information of the PDF: title, author and dates."""
    ...

def page_count(path: str | PathLike[str]) -> int:
    """Return the number of pages of the PDF."""
    ...
