"""Redaction: taking text, images and shapes out of a PDF for good.

Boxes are `(x0, y0, x1, y1)` in points from the top left corner of the visible
area of the page, as the boxes of words and of search results are.
"""

from __future__ import annotations

import warnings
from dataclasses import dataclass
from typing import Any, Literal, Sequence

from ._draw import Color, _color, _page
from ._annotate import _box


@dataclass(frozen=True)
class Redaction:
    """An area of a page to redact. Everything under `box` is taken out of the PDF,
    and a box of `fill` (black by default; `None` for none) is drawn there."""

    page: int
    box: tuple[float, float, float, float]
    fill: Color | None = "black"

    def _item(self) -> tuple[int, tuple[float, float, float, float], tuple[int, int, int] | None]:
        return (_page(self.page), _box(self.box), _color(self.fill, "fill"))


@dataclass(frozen=True)
class RedactionReport:
    """What `redact` took out."""

    glyphs: int
    """Letters taken out of the text."""
    images_removed: int
    """Images removed whole."""
    images_blanked: int
    """Images that had the pixels under an area blanked."""
    paths: int
    """Lines and shapes removed."""
    forms: int
    """Reusable drawings (form XObjects) copied to take something out of them."""
    annotations: int
    """Annotations removed."""
    widgets_left: int
    """Form fields under an area, which are left as they are."""

    @property
    def empty(self) -> bool:
        """Whether nothing was under the areas."""
        return not (
            self.glyphs
            + self.images_removed
            + self.images_blanked
            + self.paths
            + self.forms
            + self.annotations
        )


def redact(
    path: Any,
    output: Any,
    items: Sequence[Redaction],
    *,
    images: Literal["pixels", "remove", "keep"] = "pixels",
    graphics: Literal["covered", "touched", "keep"] = "covered",
    annotations: bool = True,
    password: str | None = None,
) -> RedactionReport:
    """Take what is under the areas out of the PDF, and save the result to `output`.

    Unlike drawing a black box, the content is **removed**: the letters leave the text
    (the rest of the line keeps its place), so it can't be selected, searched or
    extracted, and the old content isn't in the saved file.

    - `images`: `"pixels"` (the default) blanks the pixels under the area and keeps
      the rest of the picture; `"remove"` takes out the whole image; `"keep"` leaves
      it (only the box hides it).
    - `graphics`: lines and shapes that are all inside an area (`"covered"`, the
      default) or that touch it (`"touched"`) are removed, or none (`"keep"`).
    - `annotations`: remove those under an area (form fields are never removed: a
      warning says how many are left).

    It does not reach what is outside the pages' content: bookmarks, form fields, the
    structure of tagged files, the metadata and attachments. The result is never saved
    incrementally, since that would keep the old content.
    """
    from . import ozaru as native

    if not isinstance(items, (list, tuple)):
        items = list(items)
    for item in items:
        if not isinstance(item, Redaction):
            raise TypeError(f"Redact with Redaction objects, not {item!r}")
    counts = native._redact(
        path,
        output,
        [item._item() for item in items],
        images,
        graphics,
        bool(annotations),
        password,
    )
    report = RedactionReport(*counts)
    if report.widgets_left:
        warnings.warn(
            f"{report.widgets_left} form field(s) are under a redaction and were left as they "
            "are: their values aren't removed",
            stacklevel=2,
        )
    return report


def redact_text(
    path: Any,
    output: Any,
    text: str,
    *,
    case_sensitive: bool = False,
    pages: Sequence[int] | None = None,
    fill: Color | None = "black",
    images: Literal["pixels", "remove", "keep"] = "pixels",
    graphics: Literal["covered", "touched", "keep"] = "covered",
    annotations: bool = True,
    password: str | None = None,
) -> int:
    """Redact every place `text` is found, and save the result to `output`.

    Returns how many places were redacted. If there are none, the output is written
    unchanged. The arguments are those of `search` and `redact`.
    """
    from . import ozaru as native

    document = native.open(path, password=password)
    found = document.search(text, case_sensitive=case_sensitive, pages=pages)
    # One box for each line of each match, a point larger so the letters' edges go.
    items = [Redaction(m.page, (b[0] - 0.5, b[1], b[2] + 0.5, b[3]), fill) for m in found for b in m.boxes]
    redact(path, output, items, images=images, graphics=graphics, annotations=annotations, password=password)
    return len(found)
