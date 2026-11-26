"""What `ozaru.draw` draws: text, lines and shapes on a page.

Positions are in points, measured from the top left corner of the visible area
of the page, as the boxes of words are. Colors are `(red, green, blue)` with
each from 0 to 255, or `"#rrggbb"`, `"#rgb"` or a name such as `"red"`.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any, Sequence, Union

Color = Union[tuple[int, int, int], str]

_NAMES = {
    "black": (0, 0, 0),
    "white": (255, 255, 255),
    "red": (255, 0, 0),
    "green": (0, 128, 0),
    "blue": (0, 0, 255),
    "yellow": (255, 255, 0),
    "orange": (255, 165, 0),
    "purple": (128, 0, 128),
    "gray": (128, 128, 128),
    "grey": (128, 128, 128),
    "lightgray": (211, 211, 211),
    "lightgrey": (211, 211, 211),
    "darkgray": (64, 64, 64),
    "darkgrey": (64, 64, 64),
}


def _color(value: Any, what: str = "color") -> tuple[int, int, int] | None:
    if value is None:
        return None
    if isinstance(value, str):
        text = value.strip().lower()
        if text in _NAMES:
            return _NAMES[text]
        if text.startswith("#"):
            digits = text[1:]
            if len(digits) == 3:
                digits = "".join(c * 2 for c in digits)
            if len(digits) == 6:
                try:
                    return (int(digits[0:2], 16), int(digits[2:4], 16), int(digits[4:6], 16))
                except ValueError:
                    pass
        raise ValueError(f"{what} {value!r} is not a color: use (r, g, b), '#rrggbb' or a name")
    try:
        red, green, blue = value
    except (TypeError, ValueError):
        raise ValueError(f"{what} must be (red, green, blue) or a color name, not {value!r}") from None
    channels = (red, green, blue)
    if not all(isinstance(c, (int, float)) and not isinstance(c, bool) and 0 <= c <= 255 for c in channels):
        raise ValueError(f"The parts of {what} must be numbers from 0 to 255, not {value!r}")
    return (round(red), round(green), round(blue))


def _opacity(value: float) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool) or not 0 <= value <= 1:
        raise ValueError(f"opacity must be a number from 0 to 1, not {value!r}")
    return float(value)


def _positive(value: float, what: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool) or not value > 0:
        raise ValueError(f"{what} must be a positive number, not {value!r}")
    return float(value)


def _number(value: float, what: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise TypeError(f"{what} must be a number, not {value!r}")
    return float(value)


def _page(value: int) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise ValueError(f"page must be a whole number from 1, not {value!r}")
    return value


@dataclass(frozen=True)
class Text:
    """Text in a standard font: Helvetica, Times-Roman or Courier, each with
    `-Bold`, `-Italic` or `-BoldItalic` (`Times-Bold`, `Courier-Oblique`...),
    Symbol or ZapfDingbats. The characters are those of Western Europe.

    `font` can also be the path of a TrueType file (`.ttf`), for any alphabet
    the font has. The PDF gets only the glyphs the text uses (a subset of the
    font), so it stays small, and the text can still be selected and extracted.
    The font's license must allow embedding.

    `(x, y)` is where the baseline starts, or where its middle (`align="center"`)
    or end (`align="right"`) is; with `valign="middle"`, the middle of the
    letters is at `y`. `rotation` turns it counterclockwise around `(x, y)`.
    """

    page: int
    x: float
    y: float
    text: str
    font: str | os.PathLike[str] = "Helvetica"
    size: float = 12.0
    color: Color = (0, 0, 0)
    opacity: float = 1.0
    rotation: float = 0.0
    align: str = "left"
    valign: str = "baseline"

    def _op(self) -> dict[str, Any]:
        if self.align not in ("left", "center", "right"):
            raise ValueError(f"align must be 'left', 'center' or 'right', not {self.align!r}")
        if self.valign not in ("baseline", "middle"):
            raise ValueError(f"valign must be 'baseline' or 'middle', not {self.valign!r}")
        if not isinstance(self.text, str):
            raise TypeError(f"text must be a string, not {self.text!r}")
        return {
            "kind": "text",
            "page": _page(self.page),
            "x": _number(self.x, "x"),
            "y": _number(self.y, "y"),
            "text": self.text,
            "font": os.fspath(self.font),
            "size": _positive(self.size, "size"),
            "color": _color(self.color),
            "opacity": _opacity(self.opacity),
            "rotation": _number(self.rotation, "rotation"),
            "align": self.align,
            "valign": self.valign,
        }


def _stroke_fields(
    stroke: Color | None, width: float, dash: Sequence[float], round_ends: bool
) -> dict[str, Any]:
    pattern = tuple(_positive(d, "a dash length") for d in dash)
    return {
        "stroke": _color(stroke, "stroke"),
        "width": _positive(width, "width"),
        "dash": pattern,
        "round": bool(round_ends),
    }


@dataclass(frozen=True)
class Line:
    """A straight line from `(x0, y0)` to `(x1, y1)`. `dash` is the lengths of
    dashes and gaps, such as `(4, 2)`."""

    page: int
    x0: float
    y0: float
    x1: float
    y1: float
    color: Color = (0, 0, 0)
    width: float = 1.0
    dash: Sequence[float] = ()
    opacity: float = 1.0
    round_ends: bool = False

    def _op(self) -> dict[str, Any]:
        return {
            "kind": "polyline",
            "page": _page(self.page),
            "points": [
                (_number(self.x0, "x0"), _number(self.y0, "y0")),
                (_number(self.x1, "x1"), _number(self.y1, "y1")),
            ],
            "fill": None,
            "opacity": _opacity(self.opacity),
            **_stroke_fields(self.color, self.width, self.dash, self.round_ends),
        }


@dataclass(frozen=True)
class Rect:
    """A rectangle with corners `(x0, y0)` and `(x1, y1)`, filled with `fill` and
    outlined with `stroke`; either can be `None`, but not both."""

    page: int
    x0: float
    y0: float
    x1: float
    y1: float
    fill: Color | None = None
    stroke: Color | None = (0, 0, 0)
    width: float = 1.0
    dash: Sequence[float] = ()
    opacity: float = 1.0

    def _op(self) -> dict[str, Any]:
        return _shape("rect", self)


@dataclass(frozen=True)
class Ellipse:
    """An ellipse inside the rectangle with corners `(x0, y0)` and `(x1, y1)`; a
    circle if that is a square."""

    page: int
    x0: float
    y0: float
    x1: float
    y1: float
    fill: Color | None = None
    stroke: Color | None = (0, 0, 0)
    width: float = 1.0
    dash: Sequence[float] = ()
    opacity: float = 1.0

    def _op(self) -> dict[str, Any]:
        return _shape("ellipse", self)


def _shape(kind: str, shape: Rect | Ellipse) -> dict[str, Any]:
    if shape.fill is None and shape.stroke is None:
        raise ValueError("A shape needs a fill, a stroke or both")
    return {
        "kind": kind,
        "page": _page(shape.page),
        "box": tuple(_number(v, "a corner") for v in (shape.x0, shape.y0, shape.x1, shape.y1)),
        "fill": _color(shape.fill, "fill"),
        "opacity": _opacity(shape.opacity),
        **_stroke_fields(shape.stroke, shape.width, shape.dash, False),
    }


@dataclass(frozen=True)
class Polygon:
    """A path through `points`, closed by default: filled with `fill` and outlined
    with `stroke`; either can be `None`, but not both."""

    page: int
    points: Sequence[tuple[float, float]]
    fill: Color | None = None
    stroke: Color | None = (0, 0, 0)
    width: float = 1.0
    dash: Sequence[float] = ()
    opacity: float = 1.0
    closed: bool = True

    def _op(self) -> dict[str, Any]:
        if self.fill is None and self.stroke is None:
            raise ValueError("A shape needs a fill, a stroke or both")
        points = [(_number(x, "a point"), _number(y, "a point")) for x, y in self.points]
        if len(points) < 2:
            raise ValueError("A polygon needs at least two points")
        return {
            "kind": "polygon" if self.closed else "polyline",
            "page": _page(self.page),
            "points": points,
            "fill": _color(self.fill, "fill"),
            "opacity": _opacity(self.opacity),
            **_stroke_fields(self.stroke, self.width, self.dash, False),
        }
