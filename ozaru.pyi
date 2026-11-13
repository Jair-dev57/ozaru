from datetime import datetime
from collections.abc import Sequence
from os import PathLike
from pathlib import Path

class PageWarning(UserWarning):
    """Warns that the text or the images of one page could not be extracted."""

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
    @property
    def xmp(self) -> str | None:
        """The XMP metadata of the PDF, the XML of its catalog's /Metadata."""

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

class Word:
    """A word of a PDF, and where it is on its page.

    The box is in points (1/72 inch) from the top left corner of the visible
    area of the page, before the page is rotated: `x0` and `x1` go right, and
    `y0` and `y1` go down.
    """

    @property
    def page(self) -> int:
        """The number of its page, starting at 1."""
    @property
    def block(self) -> int:
        """The number of its block on the page, starting at 1."""
    @property
    def line(self) -> int:
        """The number of its line on the page, starting at 1."""
    @property
    def text(self) -> str: ...
    @property
    def x0(self) -> float:
        """The left side of its box."""
    @property
    def y0(self) -> float:
        """The top of its box."""
    @property
    def x1(self) -> float:
        """The right side of its box."""
    @property
    def y1(self) -> float:
        """The bottom of its box."""
    @property
    def bbox(self) -> tuple[float, float, float, float]:
        """The box as `(x0, y0, x1, y1)`."""

class Block:
    """A block of text of a PDF, a paragraph, a heading or a cell, and where it
    is on its page.

    The box is in points (1/72 inch) from the top left corner of the visible
    area of the page, before the page is rotated, as the box of a `Word` is.
    """

    @property
    def page(self) -> int:
        """The number of its page, starting at 1."""
    @property
    def number(self) -> int:
        """The number of the block on the page, starting at 1."""
    @property
    def text(self) -> str:
        """Its lines, separated by line feeds."""
    @property
    def x0(self) -> float: ...
    @property
    def y0(self) -> float: ...
    @property
    def x1(self) -> float: ...
    @property
    def y1(self) -> float: ...
    @property
    def bbox(self) -> tuple[float, float, float, float]:
        """The box as `(x0, y0, x1, y1)`."""

class Image:
    """An image of a PDF, and where it is on its page.

    The box is in points (1/72 inch) from the top left corner of the visible
    area of the page, before the page is rotated, as the box of a `Word` is.
    """

    @property
    def page(self) -> int:
        """The number of its page, starting at 1."""
    @property
    def number(self) -> int:
        """The number of the image on the page, starting at 1."""
    @property
    def x0(self) -> float: ...
    @property
    def y0(self) -> float: ...
    @property
    def x1(self) -> float: ...
    @property
    def y1(self) -> float: ...
    @property
    def bbox(self) -> tuple[float, float, float, float]:
        """The box as `(x0, y0, x1, y1)`."""
    @property
    def width(self) -> int | None:
        """Its width in pixels, or `None` if the image is damaged."""
    @property
    def height(self) -> int | None:
        """Its height in pixels, or `None` if the image is damaged."""
    @property
    def format(self) -> str | None:
        """The format of `data`: `"jpeg"`, `"jp2"` or `"png"`. `None` if ozaru
        can't give the image as a file yet."""
    @property
    def data(self) -> bytes | None:
        """The image file, in its `format`."""
    def save(self, path: str | PathLike[str]) -> None:
        """Write the image file to `path`. Raises `ValueError` if `data` is
        `None`."""

def extract_text(path: str | PathLike[str], *, password: str | None = None) -> str:
    """Return all the text of the PDF."""
    ...

def extract_words(path: str | PathLike[str], *, password: str | None = None) -> list[Word]:
    """Return every word of the PDF, page by page and line by line, with its box."""
    ...

def extract_blocks(path: str | PathLike[str], *, password: str | None = None) -> list[Block]:
    """Return every block of text of the PDF, page by page, with its box."""
    ...

def extract_images(path: str | PathLike[str], *, password: str | None = None) -> list[Image]:
    """Return every image of the PDF, page by page, with its box and its file."""
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

def rotate(
    path: str | PathLike[str],
    output: str | PathLike[str],
    degrees: int,
    *,
    pages: Sequence[int] | None = None,
    password: str | None = None,
) -> None:
    """Turn pages of the PDF clockwise and save the result to another file."""
    ...

def extract_pages(
    path: str | PathLike[str],
    output: str | PathLike[str],
    pages: Sequence[int],
    *,
    password: str | None = None,
) -> None:
    """Save some pages of the PDF, in the order given, to another file."""
    ...

def split(
    path: str | PathLike[str],
    folder: str | PathLike[str],
    *,
    pages_per_file: int = 1,
    password: str | None = None,
) -> list[Path]:
    """Save the PDF as several files in a folder, one for each page or group of pages."""
    ...
