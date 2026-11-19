from collections.abc import Collection, Sequence
from datetime import datetime
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

class Annotation:
    """An annotation of a page: a link, a note, a highlight or a form widget.

    `rect` and `quad_points` are in points from the top left corner of the
    visible area, before rotation, as for `Word`.
    """

    @property
    def page(self) -> int:
        """The page number, starting at 1."""
    @property
    def subtype(self) -> str:
        """`Link`, `Text`, `Highlight`, `Widget`... as the PDF names it."""
    @property
    def rect(self) -> tuple[float, float, float, float]:
        """The rectangle as (x0, y0, x1, y1)."""
    @property
    def contents(self) -> str | None:
        """The text of a note, or its description."""
    @property
    def name(self) -> str | None:
        """The name that identifies the annotation in its page."""
    @property
    def author(self) -> str | None: ...
    @property
    def subject(self) -> str | None: ...
    @property
    def modified(self) -> datetime | None: ...
    @property
    def flags(self) -> int:
        """The flags of the PDF standard: 2 is hidden, 4 is print."""
    @property
    def color(self) -> list[float] | None:
        """The color, as 1, 3 or 4 numbers from 0 to 1."""
    @property
    def quad_points(self) -> list[tuple[float, float, float, float, float, float, float, float]]:
        """The quadrilaterals of text markup, four corners each."""
    @property
    def uri(self) -> str | None:
        """Where a link to the web goes."""
    @property
    def target_page(self) -> int | None:
        """The page a link to this document goes to, starting at 1."""
    @property
    def has_appearance(self) -> bool:
        """Whether the annotation has a drawing that `render_page` shows."""

class FormField:
    """A field of the form of a PDF: a text box, a check box, a list..."""

    @property
    def name(self) -> str:
        """The full name, with the names of its groups joined by dots."""
    @property
    def kind(self) -> str:
        """`text`, `checkbox`, `radio`, `button`, `combo`, `list`, `signature` or `unknown`."""
    @property
    def value(self) -> str | bool | list[str] | None:
        """A `str` for text and choices, a `bool` for check boxes, the chosen button for radio groups."""
    @property
    def default_value(self) -> str | bool | list[str] | None: ...
    @property
    def read_only(self) -> bool: ...
    @property
    def required(self) -> bool: ...
    @property
    def multiline(self) -> bool: ...
    @property
    def max_length(self) -> int | None: ...
    @property
    def options(self) -> list[tuple[str, str]]:
        """The choices of a combo or list, as (value, label)."""
    @property
    def page(self) -> int | None:
        """The page of its first widget, starting at 1."""
    @property
    def rect(self) -> tuple[float, float, float, float] | None:
        """The rectangle of its first widget, as for `Annotation.rect`."""

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

def annotations(path: str | PathLike[str], *, password: str | None = None) -> list[Annotation]:
    """Return the annotations of every page of the PDF."""
    ...

def form_fields(path: str | PathLike[str], *, password: str | None = None) -> list[FormField]:
    """Return the fields of the form of the PDF, with their values."""
    ...

def fill_form(
    path: str | PathLike[str],
    output: str | PathLike[str],
    values: dict[str, str | bool | list[str] | None],
    *,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Fill fields of the form and save the result to another file."""
    ...

def encrypt(
    path: str | PathLike[str],
    output: str | PathLike[str],
    *,
    user_password: str = "",
    owner_password: str | None = None,
    permissions: Collection[str] | None = None,
    encrypt_metadata: bool = True,
    password: str | None = None,
) -> None:
    """Protect the PDF with passwords and permissions, using AES-256, and save it."""
    ...

def permissions(path: str | PathLike[str], *, password: str | None = None) -> list[str] | None:
    """Return what the PDF allows, or None if it isn't protected."""
    ...

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

def render_page(
    path: str | PathLike[str],
    page: int,
    *,
    scale: float = 1.0,
    annotations: bool = True,
    password: str | None = None,
) -> bytes:
    """Draw a page of the PDF as a PNG image, with `scale` pixels for each point.

    Annotations and form widgets are drawn unless `annotations` is false."""
    ...

def rotate(
    path: str | PathLike[str],
    output: str | PathLike[str],
    degrees: int,
    *,
    pages: Sequence[int] | None = None,
    password: str | None = None,
    incremental: bool = False,
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

def merge(
    paths: Sequence[str | PathLike[str]],
    output: str | PathLike[str],
    *,
    password: str | None = None,
) -> None:
    """Join several PDFs into one file, in the order given."""
    ...
