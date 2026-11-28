"""Sanitizing: taking out of a PDF what it carries besides what its pages show."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Sequence


@dataclass(frozen=True)
class SanitizeReport:
    """What `sanitize` took out."""

    info_entries: int
    """Entries of the document information (title, author, dates...)."""
    metadata_streams: int
    """XMP metadata of the document, pages, images and drawings."""
    private_data: int
    """Private data of applications and thumbnails."""
    scripts: int
    """Actions that run things, additional actions, XFA forms and media annotations."""
    attachments: int
    """Attached files and the annotations that held them."""
    bookmarks: int
    comments: int
    form_values: int
    """Fields whose value was emptied."""
    structure: int
    """Tagged structure of the document and of its pages."""

    @property
    def empty(self) -> bool:
        """Whether there was nothing to take out."""
        return not any(
            (
                self.info_entries,
                self.metadata_streams,
                self.private_data,
                self.scripts,
                self.attachments,
                self.bookmarks,
                self.comments,
                self.form_values,
                self.structure,
            )
        )


def sanitize(
    path: Any,
    output: Any,
    *,
    metadata: bool = True,
    keep_info: Sequence[str] = (),
    scripts: bool = True,
    attachments: bool = True,
    bookmarks: bool = False,
    comments: bool = False,
    form_values: bool = False,
    structure: bool = False,
    everything: bool = False,
    password: str | None = None,
) -> SanitizeReport:
    """Take out of the PDF what it carries besides what its pages show, and save the
    result to `output`. The pages look the same.

    - `metadata` (on by default): the document information (title, author, dates,
      producer...) except the names in `keep_info` (such as `["Title"]`), the XMP
      metadata, private data of applications, thumbnails, and the metadata of pages,
      images and drawings.
    - `scripts` (on): JavaScript and the other actions that run things (launching
      files, sending forms, playing media), the additional actions of the document,
      pages, annotations and fields, XFA forms, and media annotations. Links to
      addresses and pages stay.
    - `attachments` (on): attached files, and the annotations that hold a file.
    - `bookmarks`, `comments` (annotations that aren't links or form fields),
      `form_values` (the fields are emptied) and `structure` (the tags of tagged
      files) change the document, so they are asked for. `everything=True` asks for
      all of them.

    What it doesn't reach: the text in the content of the pages (see `redact`), the
    names of fonts and the identifier of the file. It never saves incrementally, since
    that would keep what was taken out.
    """
    from . import ozaru as native

    if everything:
        bookmarks = comments = form_values = structure = True
    if isinstance(keep_info, str):
        keep_info = [keep_info]
    counts = native._sanitize(
        path,
        output,
        bool(metadata),
        list(keep_info),
        bool(scripts),
        bool(attachments),
        bool(bookmarks),
        bool(comments),
        bool(form_values),
        bool(structure),
        password,
    )
    return SanitizeReport(*counts)
