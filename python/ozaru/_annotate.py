"""What `ozaru.annotate` adds: marks on text.

Boxes are `(x0, y0, x1, y1)` in points from the top left corner of the visible
area of the page, as the boxes of words, lines and search results are, so those
can be given as they are. Colors are as for `ozaru.draw`.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Literal, Sequence

from ._draw import Color, _color, _opacity, _page

Box = tuple[float, float, float, float]


def _boxes(value: Any) -> list[tuple[float, float, float, float]]:
    # A single box, or a list of them.
    if isinstance(value, (tuple, list)) and len(value) == 4 and all(
        isinstance(v, (int, float)) and not isinstance(v, bool) for v in value
    ):
        value = [value]
    boxes = []
    for box in value:
        try:
            x0, y0, x1, y1 = (float(v) for v in box)
        except (TypeError, ValueError):
            raise ValueError(f"A box must be (x0, y0, x1, y1), not {box!r}") from None
        boxes.append((x0, y0, x1, y1))
    if not boxes:
        raise ValueError("An annotation needs at least one box")
    return boxes


@dataclass(frozen=True)
class _Markup:
    page: int
    boxes: Box | Sequence[Box]
    color: Color | None = None
    opacity: float = 1.0
    note: str | None = None
    author: str | None = None
    subject: str | None = None
    date: datetime | None = field(default=None, compare=False)

    _kind = ""

    def _annotation(self) -> dict[str, Any]:
        for text, what in ((self.note, "note"), (self.author, "author"), (self.subject, "subject")):
            if text is not None and not isinstance(text, str):
                raise TypeError(f"{what} must be a string, not {text!r}")
        return {
            "kind": self._kind,
            "page": _page(self.page),
            "boxes": _boxes(self.boxes),
            "color": _color(self.color),
            "opacity": _opacity(self.opacity),
            "note": self.note,
            "author": self.author,
            "subject": self.subject,
            "date": self.date or datetime.now().astimezone(),
        }


@dataclass(frozen=True)
class Highlight(_Markup):
    """A translucent band over the text in `boxes` (one box, or a list: one for each
    line), yellow unless `color` says otherwise. The text under it stays readable.
    `note` is the comment that goes with it, `author` and `subject` say who and what
    about; `date` is when (now by default)."""

    _kind = "highlight"


@dataclass(frozen=True)
class Underline(_Markup):
    """A line under the text in `boxes`, red unless `color` says otherwise."""

    _kind = "underline"


@dataclass(frozen=True)
class StrikeOut(_Markup):
    """A line through the text in `boxes`, red unless `color` says otherwise."""

    _kind = "strikeout"


@dataclass(frozen=True)
class Squiggly(_Markup):
    """A wavy line under the text in `boxes`, red unless `color` says otherwise."""

    _kind = "squiggly"


_KINDS = {"highlight": Highlight, "underline": Underline, "strikeout": StrikeOut, "squiggly": Squiggly}


def highlight(
    path: Any,
    output: Any,
    text: str,
    *,
    kind: Literal["highlight", "underline", "strikeout", "squiggly"] = "highlight",
    case_sensitive: bool = False,
    pages: Sequence[int] | None = None,
    color: Color | None = None,
    opacity: float = 1.0,
    note: str | None = None,
    author: str | None = None,
    password: str | None = None,
    incremental: bool = False,
) -> int:
    """Mark every place `text` is found in the PDF, and save the result to `output`.

    `kind` says how: `"highlight"` (the default), `"underline"`, `"strikeout"` or
    `"squiggly"`. `pages` (from 1) limits where it looks. Returns how many places
    were marked; if there are none, the output is still written, unchanged.
    """
    from . import ozaru as native

    try:
        mark = _KINDS[kind]
    except KeyError:
        raise ValueError(
            f"kind must be 'highlight', 'underline', 'strikeout' or 'squiggly', not {kind!r}"
        ) from None
    document = native.open(path, password=password)
    found = document.search(text, case_sensitive=case_sensitive, pages=pages)
    items = [
        mark(m.page, m.boxes, color=color, opacity=opacity, note=note, author=author) for m in found
    ]
    document.annotate(output, items, incremental=incremental)
    return len(items)
