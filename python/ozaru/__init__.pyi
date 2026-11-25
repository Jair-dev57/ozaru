from collections.abc import Collection, Iterator, Sequence
from datetime import datetime
from os import PathLike
from pathlib import Path
from typing import BinaryIO, Literal, TypedDict, overload

PdfSource = str | PathLike[str] | bytes | bytearray | memoryview | BinaryIO
"""A PDF: the path of a file, its bytes, a file opened in binary mode, or an
`http://` or `https://` URL."""

PageLabelStyle = Literal[
    "decimal", "roman", "roman_lower", "letters", "letters_lower", "D", "R", "r", "A", "a"
]

class PageLabelRange(TypedDict, total=False):
    """A range of pages labeled alike, for `set_page_labels`."""

    page: int  # required: the first page of the range, from 1
    style: PageLabelStyle | None
    prefix: str
    start: int

class MetadataValues(TypedDict, total=False):
    """What `set_metadata` changes: a value sets it, `None` removes it."""

    title: str | None
    author: str | None
    subject: str | None
    keywords: str | None
    creator: str | None
    producer: str | None
    creation_date: datetime | None
    modification_date: datetime | None
    xmp: str | None

BookmarkLike = Bookmark | tuple[int, str, int | str | None]
"""A `Bookmark`, or `(level, title, page)` where the last is a page number, an
address or `None`."""

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

class Span:
    """A piece of text on a line that has one font, size and color. The box is
    measured as the box of a `Word` is."""

    @property
    def page(self) -> int: ...
    @property
    def block(self) -> int: ...
    @property
    def line(self) -> int: ...
    @property
    def text(self) -> str: ...
    @property
    def font(self) -> str | None:
        """The font as the PDF names it, such as `ABCDEF+Arial-BoldMT`."""
    @property
    def size(self) -> float:
        """The size of the text on the page, in points."""
    @property
    def color(self) -> tuple[int, int, int]:
        """`(red, green, blue)`, each from 0 to 255."""
    @property
    def bold(self) -> bool: ...
    @property
    def italic(self) -> bool: ...
    @property
    def x0(self) -> float: ...
    @property
    def y0(self) -> float: ...
    @property
    def x1(self) -> float: ...
    @property
    def y1(self) -> float: ...
    @property
    def bbox(self) -> tuple[float, float, float, float]: ...

class Match:
    """Where a search found its text."""

    @property
    def page(self) -> int: ...
    @property
    def text(self) -> str:
        """The text as the page has it."""
    @property
    def boxes(self) -> list[tuple[float, float, float, float]]:
        """One box for each line the match is on."""
    @property
    def bbox(self) -> tuple[float, float, float, float]:
        """The box around the whole match."""

class Table:
    """A table of a page."""

    @property
    def page(self) -> int: ...
    @property
    def rows(self) -> list[list[str | None]]:
        """The cells row by row from the top; `None` where the row has no cell."""
    @property
    def ruled(self) -> bool:
        """Whether it is made of lines the page draws, not found from its text."""
    @property
    def markdown(self) -> str: ...
    @property
    def x0(self) -> float: ...
    @property
    def y0(self) -> float: ...
    @property
    def x1(self) -> float: ...
    @property
    def y1(self) -> float: ...
    @property
    def bbox(self) -> tuple[float, float, float, float]: ...

TableStrategy = Literal["lines", "text", "both", "auto"]

class Bookmark:
    """An entry of the outline a viewer shows beside the pages."""

    @property
    def title(self) -> str: ...
    @property
    def level(self) -> int:
        """How deep it is, from 1 for the top ones."""
    @property
    def page(self) -> int | None:
        """The page it goes to, from 1, if it goes to one of the PDF."""
    @property
    def uri(self) -> str | None:
        """The address it opens, if it is a link."""
    @property
    def open(self) -> bool:
        """Whether a viewer shows its children."""

class NamedDestination:
    """A name that links can use to go to a page."""

    @property
    def name(self) -> str: ...
    @property
    def page(self) -> int | None:
        """The page it goes to, from 1, if it is one of the PDF."""

class Attachment:
    """A file attached to the PDF."""

    @property
    def name(self) -> str:
        """The name it has in the list of attachments."""
    @property
    def filename(self) -> str | None: ...
    @property
    def description(self) -> str | None: ...
    @property
    def mime_type(self) -> str | None: ...
    @property
    def size(self) -> int | None:
        """The size in bytes, as the PDF says it."""
    @property
    def creation_date(self) -> datetime | None: ...
    @property
    def modification_date(self) -> datetime | None: ...
    @property
    def data(self) -> bytes | None:
        """The contents, or `None` if they can't be read."""
    def save(self, path: str | PathLike[str]) -> None:
        """Write the file to `path`. Raises `ValueError` if `data` is `None`."""

class Page:
    """One page of a `Document`. What it reads is kept by the document."""

    @property
    def number(self) -> int:
        """The page number, starting at 1."""
    @property
    def text(self) -> str: ...
    @property
    def words(self) -> list[Word]: ...
    @property
    def blocks(self) -> list[Block]: ...
    @property
    def images(self) -> list[Image]: ...
    @property
    def annotations(self) -> list[Annotation]: ...
    @property
    def form_fields(self) -> list[FormField]:
        """The fields whose first widget is on this page."""
    @property
    def spans(self) -> list[Span]: ...
    @property
    def tables(self) -> list[Table]: ...
    @property
    def markdown(self) -> str: ...
    def search(self, text: str, *, case_sensitive: bool = False) -> list[Match]: ...
    @property
    def label(self) -> str:
        """The label a viewer shows for the page: `iv`, `12`, `A-3`..."""
    @property
    def info(self) -> PageInfo: ...
    @property
    def width(self) -> float: ...
    @property
    def height(self) -> float: ...
    @property
    def rotation(self) -> int: ...
    def render(self, *, scale: float = 1.0, annotations: bool = True) -> bytes: ...

class Document:
    """A PDF opened once: the bytes are read when it is opened, and the text,
    words, blocks, images and annotations of each page are read the first time
    they are asked for and kept."""

    def __init__(self, path: PdfSource, *, password: str | None = None) -> None: ...
    @property
    def page_count(self) -> int: ...
    def __len__(self) -> int: ...
    def __enter__(self) -> Document: ...
    def __exit__(self, *exception: object) -> bool: ...
    @overload
    def __getitem__(self, index: int) -> Page: ...
    @overload
    def __getitem__(self, index: slice) -> list[Page]: ...
    def __iter__(self) -> Iterator[Page]: ...
    @property
    def text(self) -> str: ...
    @property
    def words(self) -> list[Word]: ...
    @property
    def blocks(self) -> list[Block]: ...
    @property
    def images(self) -> list[Image]: ...
    @property
    def annotations(self) -> list[Annotation]: ...
    @property
    def form_fields(self) -> list[FormField]: ...
    @property
    def spans(self) -> list[Span]: ...
    @property
    def tables(self) -> list[Table]: ...
    @property
    def markdown(self) -> str: ...
    def to_markdown(
        self, *, page_markers: bool = False, strategy: TableStrategy = "both"
    ) -> str: ...
    def search(
        self, text: str, *, case_sensitive: bool = False, pages: Sequence[int] | None = None
    ) -> list[Match]: ...
    def find_tables(self, *, strategy: TableStrategy = "both") -> list[Table]: ...
    @property
    def page_info(self) -> list[PageInfo]: ...
    @property
    def bookmarks(self) -> list[Bookmark]: ...
    @property
    def page_labels(self) -> list[str]: ...
    @property
    def named_destinations(self) -> list[NamedDestination]: ...
    @property
    def attachments(self) -> list[Attachment]: ...
    @property
    def metadata(self) -> Metadata: ...
    @property
    def permissions(self) -> list[str] | None: ...
    def render(self, page: int, *, scale: float = 1.0, annotations: bool = True) -> bytes: ...
    def save(self, output: str | PathLike[str]) -> None: ...
    def rotate(
        self,
        output: str | PathLike[str],
        degrees: int,
        *,
        pages: Sequence[int] | None = None,
        incremental: bool = False,
    ) -> None: ...
    def extract_pages(self, output: str | PathLike[str], pages: Sequence[int]) -> None: ...
    def set_metadata(
        self, output: str | PathLike[str], values: MetadataValues, *, incremental: bool = False
    ) -> None: ...
    def set_bookmarks(
        self,
        output: str | PathLike[str],
        bookmarks: Sequence[BookmarkLike],
        *,
        incremental: bool = False,
    ) -> None: ...
    def set_page_labels(
        self,
        output: str | PathLike[str],
        labels: Sequence[PageLabelRange],
        *,
        incremental: bool = False,
    ) -> None: ...
    def add_attachment(
        self,
        output: str | PathLike[str],
        name: str,
        data: PdfSource,
        *,
        filename: str | None = None,
        description: str | None = None,
        mime_type: str | None = None,
        incremental: bool = False,
    ) -> None: ...
    def remove_attachment(
        self, output: str | PathLike[str], name: str, *, incremental: bool = False
    ) -> None: ...
    def crop(
        self,
        output: str | PathLike[str],
        box: tuple[float, float, float, float],
        *,
        pages: Sequence[int] | None = None,
        incremental: bool = False,
    ) -> None: ...
    def delete_pages(self, output: str | PathLike[str], pages: Sequence[int]) -> None: ...
    def add_blank_page(
        self,
        output: str | PathLike[str],
        *,
        position: int | None = None,
        width: float | None = None,
        height: float | None = None,
        incremental: bool = False,
    ) -> None: ...
    def split(self, folder: str | PathLike[str], *, pages_per_file: int = 1) -> list[Path]: ...
    def fill_form(
        self,
        output: str | PathLike[str],
        values: dict[str, str | bool | list[str] | None],
        *,
        incremental: bool = False,
    ) -> None: ...
    def encrypt(
        self,
        output: str | PathLike[str],
        *,
        user_password: str = "",
        owner_password: str | None = None,
        permissions: Collection[str] | None = None,
        encrypt_metadata: bool = True,
    ) -> None: ...

def open(path: PdfSource, *, password: str | None = None) -> Document:
    """Open a PDF once to read several things from it. Same as `Document(path)`."""

def bookmarks(path: PdfSource, *, password: str | None = None) -> list[Bookmark]: ...
def page_labels(path: PdfSource, *, password: str | None = None) -> list[str]: ...
def named_destinations(
    path: PdfSource, *, password: str | None = None
) -> list[NamedDestination]: ...
def attachments(path: PdfSource, *, password: str | None = None) -> list[Attachment]: ...
def set_metadata(
    path: PdfSource,
    output: str | PathLike[str],
    values: MetadataValues,
    *,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Change the document information; `None` removes a value."""

def set_bookmarks(
    path: PdfSource,
    output: str | PathLike[str],
    bookmarks: Sequence[BookmarkLike],
    *,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Replace the bookmarks; an empty list removes them."""

def set_page_labels(
    path: PdfSource,
    output: str | PathLike[str],
    labels: Sequence[PageLabelRange],
    *,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Replace the page labels; an empty list removes them."""

def add_attachment(
    path: PdfSource,
    output: str | PathLike[str],
    name: str,
    data: PdfSource,
    *,
    filename: str | None = None,
    description: str | None = None,
    mime_type: str | None = None,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Attach a file, given as bytes or a path; one with the same name is replaced."""

def remove_attachment(
    path: PdfSource,
    output: str | PathLike[str],
    name: str,
    *,
    password: str | None = None,
    incremental: bool = False,
) -> None: ...
def crop(
    path: PdfSource,
    output: str | PathLike[str],
    box: tuple[float, float, float, float],
    *,
    pages: Sequence[int] | None = None,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Crop pages to `box`, measured from the top left of the visible area."""

def delete_pages(
    path: PdfSource,
    output: str | PathLike[str],
    pages: Sequence[int],
    *,
    password: str | None = None,
) -> None: ...
def add_blank_page(
    path: PdfSource,
    output: str | PathLike[str],
    *,
    position: int | None = None,
    width: float | None = None,
    height: float | None = None,
    password: str | None = None,
    incremental: bool = False,
) -> None: ...

def extract_spans(path: PdfSource, *, password: str | None = None) -> list[Span]: ...
def search(
    path: PdfSource,
    text: str,
    *,
    case_sensitive: bool = False,
    pages: Sequence[int] | None = None,
    password: str | None = None,
) -> list[Match]:
    """Find `text` and return where it is, page by page."""

def extract_tables(
    path: PdfSource, *, strategy: TableStrategy = "both", password: str | None = None
) -> list[Table]: ...
def to_markdown(
    path: PdfSource,
    *,
    page_markers: bool = False,
    strategy: TableStrategy = "both",
    password: str | None = None,
) -> str:
    """The PDF as Markdown: headings, bold and italic, lists and tables."""

def annotations(path: PdfSource, *, password: str | None = None) -> list[Annotation]:
    """Return the annotations of every page of the PDF."""
    ...

def form_fields(path: PdfSource, *, password: str | None = None) -> list[FormField]:
    """Return the fields of the form of the PDF, with their values."""
    ...

def fill_form(
    path: PdfSource,
    output: str | PathLike[str],
    values: dict[str, str | bool | list[str] | None],
    *,
    password: str | None = None,
    incremental: bool = False,
) -> None:
    """Fill fields of the form and save the result to another file."""
    ...

def encrypt(
    path: PdfSource,
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

def permissions(path: PdfSource, *, password: str | None = None) -> list[str] | None:
    """Return what the PDF allows, or None if it isn't protected."""
    ...

def extract_text(path: PdfSource, *, password: str | None = None) -> str:
    """Return all the text of the PDF."""
    ...

def extract_words(path: PdfSource, *, password: str | None = None) -> list[Word]:
    """Return every word of the PDF, page by page and line by line, with its box."""
    ...

def extract_blocks(path: PdfSource, *, password: str | None = None) -> list[Block]:
    """Return every block of text of the PDF, page by page, with its box."""
    ...

def extract_images(path: PdfSource, *, password: str | None = None) -> list[Image]:
    """Return every image of the PDF, page by page, with its box and its file."""
    ...

def metadata(path: PdfSource, *, password: str | None = None) -> Metadata:
    """Return the document information of the PDF: title, author and dates."""
    ...

def page_info(path: PdfSource, *, password: str | None = None) -> list[PageInfo]:
    """Return the size and rotation of every page of the PDF."""
    ...

def page_count(path: PdfSource, *, password: str | None = None) -> int:
    """Return the number of pages of the PDF."""
    ...

def render_page(
    path: PdfSource,
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
    path: PdfSource,
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
    path: PdfSource,
    output: str | PathLike[str],
    pages: Sequence[int],
    *,
    password: str | None = None,
) -> None:
    """Save some pages of the PDF, in the order given, to another file."""
    ...

def split(
    path: PdfSource,
    folder: str | PathLike[str],
    *,
    pages_per_file: int = 1,
    password: str | None = None,
) -> list[Path]:
    """Save the PDF as several files in a folder, one for each page or group of pages."""
    ...

def merge(
    paths: Sequence[PdfSource],
    output: str | PathLike[str],
    *,
    password: str | None = None,
) -> None:
    """Join several PDFs into one file, in the order given."""
    ...
