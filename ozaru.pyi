from datetime import datetime
from os import PathLike

class PageWarning(UserWarning):
    """Warns that the text of one page could not be extracted."""

    page: int
    reason: str

class PasswordError(ValueError):
    """Raised when a PDF needs a password, or the password given is incorrect."""

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

class PageInfo:
    """The size and rotation of one page of a PDF, in points (1/72 inch)."""

    @property
    def number(self) -> int:
        """The page number, starting at 1."""
    @property
    def width(self) -> float:
        """The width of the visible area, as a viewer shows it, rotated."""
    @property
    def height(self) -> float:
        """The height of the visible area, as a viewer shows it, rotated."""
    @property
    def rotation(self) -> int:
        """How many degrees the page is turned clockwise: 0, 90, 180 or 270."""
    @property
    def media_box(self) -> tuple[float, float, float, float]:
        """The page's media box as (left, bottom, right, top), not rotated."""
    @property
    def crop_box(self) -> tuple[float, float, float, float]:
        """The visible area as (left, bottom, right, top), not rotated."""

def extract_text(path: str | PathLike[str], *, password: str | None = None) -> str:
    """Return all the text of the PDF."""
    ...

def metadata(path: str | PathLike[str], *, password: str | None = None) -> Metadata:
    """Return the document information of the PDF: title, author and dates."""
    ...

def page_info(path: str | PathLike[str], *, password: str | None = None) -> list[PageInfo]:
    """Return the size and rotation of every page of the PDF."""
    ...

def page_count(path: str | PathLike[str], *, password: str | None = None) -> int:
    """Return the number of pages of the PDF."""
    ...
