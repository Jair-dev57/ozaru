"""What `ozaru.annotate` adds: marks on text.

Boxes are `(x0, y0, x1, y1)` in points from the top left corner of the visible
area of the page, as the boxes of words, lines and search results are, so those
can be given as they are. Colors are as for `ozaru.draw`.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Literal, Sequence

from ._draw import Color, _color, _number, _opacity, _page, _positive

BoxTuple = tuple[float, float, float, float]


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
    boxes: BoxTuple | Sequence[BoxTuple]
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


def _box(value: Any) -> tuple[float, float, float, float]:
    try:
        x0, y0, x1, y1 = (float(v) for v in value)
    except (TypeError, ValueError):
        raise ValueError(f"A box must be (x0, y0, x1, y1), not {value!r}") from None
    return (x0, y0, x1, y1)


def _width(value: float, what: str = "width") -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool) or value < 0:
        raise ValueError(f"{what} must be a number that isn't negative, not {value!r}")
    return float(value)


@dataclass(frozen=True)
class _Shape:
    """What the annotations that aren't marks on text have in common."""

    page: int
    color: Color | None = field(default=None, kw_only=True)
    opacity: float = field(default=1.0, kw_only=True)
    note: str | None = field(default=None, kw_only=True)
    author: str | None = field(default=None, kw_only=True)
    subject: str | None = field(default=None, kw_only=True)
    date: datetime | None = field(default=None, compare=False, kw_only=True)

    def _common(self, kind: str) -> dict[str, Any]:
        for text, what in ((self.note, "note"), (self.author, "author"), (self.subject, "subject")):
            if text is not None and not isinstance(text, str):
                raise TypeError(f"{what} must be a string, not {text!r}")
        return {
            "kind": kind,
            "page": _page(self.page),
            "color": _color(self.color),
            "opacity": _opacity(self.opacity),
            "note": self.note,
            "author": self.author,
            "subject": self.subject,
            "date": self.date or datetime.now().astimezone(),
        }


@dataclass(frozen=True)
class Note(_Shape):
    """A sticky note: a small icon with its top left corner at `(x, y)` that opens
    `text` when it is clicked. `color` is the icon's (yellow by default); `open`
    shows the text from the start."""

    x: float
    y: float
    text: str
    open: bool = field(default=False, kw_only=True)

    def _annotation(self) -> dict[str, Any]:
        if not isinstance(self.text, str):
            raise TypeError(f"text must be a string, not {self.text!r}")
        d = self._common("note")
        d.update(x=_number(self.x, "x"), y=_number(self.y, "y"), open=bool(self.open))
        d["note"] = self.text if self.note is None else self.note
        return d


@dataclass(frozen=True)
class Link(_Shape):
    """An area that goes somewhere when clicked: to the web address `uri`, or to the
    page `to_page` (from 1) of this document. With `border` above 0, a frame of
    that width in `color` (blue by default) is drawn around it."""

    box: BoxTuple
    uri: str | None = field(default=None, kw_only=True)
    to_page: int | None = field(default=None, kw_only=True)
    border: float = field(default=0.0, kw_only=True)

    def _annotation(self) -> dict[str, Any]:
        d = self._common("link")
        d.update(box=_box(self.box), uri=self.uri, to_page=self.to_page, border=_width(self.border, "border"))
        if self.uri is not None and not isinstance(self.uri, str):
            raise TypeError(f"uri must be a string, not {self.uri!r}")
        if self.to_page is not None and (
            not isinstance(self.to_page, int) or isinstance(self.to_page, bool) or self.to_page < 1
        ):
            raise ValueError(f"to_page must be a whole number from 1, not {self.to_page!r}")
        return d


@dataclass(frozen=True)
class Box(_Shape):
    """A rectangle with corners `(x0, y0, x1, y1)`: outlined in `color` (red by
    default), filled with `fill` if given."""

    box: BoxTuple
    fill: Color | None = field(default=None, kw_only=True)
    width: float = field(default=1.5, kw_only=True)

    _kind = "box"

    def _annotation(self) -> dict[str, Any]:
        d = self._common(self._kind)
        d.update(box=_box(self.box), fill=_color(self.fill, "fill"), width=_width(self.width))
        return d


@dataclass(frozen=True)
class Oval(Box):
    """An ellipse inside the rectangle `box`; a circle if it is a square."""

    _kind = "oval"


@dataclass(frozen=True)
class Arrow(_Shape):
    """A line from `start` to `end` (each `(x, y)`), with an arrowhead at the end
    unless `arrow` is false."""

    start: tuple[float, float]
    end: tuple[float, float]
    width: float = field(default=1.5, kw_only=True)
    arrow: bool = field(default=True, kw_only=True)

    def _annotation(self) -> dict[str, Any]:
        d = self._common("arrow")
        try:
            points = (tuple(float(v) for v in self.start), tuple(float(v) for v in self.end))
            if any(len(p) != 2 for p in points):
                raise ValueError
        except (TypeError, ValueError):
            raise ValueError("start and end must be (x, y) points") from None
        d.update(points=points, width=_width(self.width), arrow=bool(self.arrow))
        return d


@dataclass(frozen=True)
class Ink(_Shape):
    """A freehand drawing: `strokes` is a list of strokes, each a list of `(x, y)`
    points joined by lines (a single stroke can be given as a list of points)."""

    strokes: Sequence[Sequence[tuple[float, float]]]
    width: float = field(default=1.5, kw_only=True)

    def _annotation(self) -> dict[str, Any]:
        strokes = list(self.strokes)
        # A single stroke given as a list of points.
        if strokes and all(
            isinstance(p, (tuple, list)) and len(p) == 2 and all(isinstance(v, (int, float)) for v in p)
            for p in strokes
        ):
            strokes = [strokes]
        try:
            clean = [[(float(x), float(y)) for x, y in stroke] for stroke in strokes]
        except (TypeError, ValueError):
            raise ValueError("strokes must be lists of (x, y) points") from None
        if not clean or any(not stroke for stroke in clean):
            raise ValueError("An ink drawing needs strokes with points")
        d = self._common("ink")
        d.update(strokes=clean, width=_width(self.width))
        return d


@dataclass(frozen=True)
class TextBox(_Shape):
    """Text in a box `(x0, y0, x1, y1)`, in Helvetica (the letters of Western Europe),
    wrapped to the box. `color` is the color of the text (black by default); `fill`
    is the background, and `border` the width of a frame in the text's color."""

    box: BoxTuple
    text: str
    size: float = field(default=12.0, kw_only=True)
    fill: Color | None = field(default=None, kw_only=True)
    border: float = field(default=0.0, kw_only=True)

    def _annotation(self) -> dict[str, Any]:
        if not isinstance(self.text, str):
            raise TypeError(f"text must be a string, not {self.text!r}")
        d = self._common("textbox")
        d.update(
            box=_box(self.box),
            text=self.text,
            size=_positive(self.size, "size"),
            fill=_color(self.fill, "fill"),
            border=_width(self.border, "border"),
        )
        if d["note"] is None:
            d["note"] = self.text
        return d


def delete_annotations(
    path: Any,
    output: Any,
    *,
    pages: Sequence[int] | None = None,
    subtypes: Sequence[str] | None = None,
    name: str | None = None,
    author: str | None = None,
    contains: str | None = None,
    password: str | None = None,
    incremental: bool = False,
) -> int:
    """Remove annotations and save the result to `output`; return how many went.

    Those removed are the ones that match every argument given, on `pages` (from 1;
    all by default): `subtypes` such as `["Highlight", "Link"]`, the `name` or
    `author` of the annotation, or text its comment `contains`. With none given,
    all of them go. Form fields are never removed.
    """
    from . import ozaru as native

    return native.open(path, password=password).delete_annotations(
        output,
        pages=pages,
        subtypes=None if subtypes is None else list(subtypes),
        name=name,
        author=author,
        contains=contains,
        incremental=incremental,
    )


def update_annotations(
    path: Any,
    output: Any,
    changes: dict[str, Any],
    *,
    pages: Sequence[int] | None = None,
    subtypes: Sequence[str] | None = None,
    name: str | None = None,
    author: str | None = None,
    contains: str | None = None,
    password: str | None = None,
    incremental: bool = False,
) -> int:
    """Change annotations and save the result to `output`; return how many changed.

    `changes` has any of `contents`, `author` and `subject` (text, or `None` to
    remove it) and `hidden` (`True` or `False`). The annotations changed are those
    that match every filter argument, as for `delete_annotations`. The drawing of
    an annotation isn't redone, so changing a color isn't possible: delete the
    annotation and add another.
    """
    from . import ozaru as native

    return native.open(path, password=password).update_annotations(
        output,
        dict(changes),
        pages=pages,
        subtypes=None if subtypes is None else list(subtypes),
        name=name,
        author=author,
        contains=contains,
        incremental=incremental,
    )


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
