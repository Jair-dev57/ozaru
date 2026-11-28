"""What `ozaru.add_fields` adds: the fields of a form.

Boxes are `(x0, y0, x1, y1)` in points from the top left corner of the visible
area of the page, as the boxes of words are. Colors are as for `ozaru.draw`.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal, Sequence

from ._annotate import BoxTuple, _box
from ._draw import Color, _color, _page

Align = Literal["left", "center", "right"]


def _options(options: Sequence[Any]) -> list[tuple[str, str]]:
    # Each option is its text, or (value, text) when the value it gives is another.
    clean = []
    for option in options:
        if isinstance(option, str):
            clean.append((option, option))
        else:
            try:
                value, label = option
                clean.append((str(value), str(label)))
            except (TypeError, ValueError):
                raise ValueError(f"An option is a text or (value, text), not {option!r}") from None
    return clean


@dataclass(frozen=True)
class _Field:
    """What every field has: its look and how it behaves."""

    page: int
    name: str
    font_size: float = field(default=0.0, kw_only=True)
    """Size of the text in points; 0 fits it to the box."""
    color: Color = field(default="black", kw_only=True)
    """The color of the text."""
    border: Color | None = field(default="black", kw_only=True)
    background: Color | None = field(default=None, kw_only=True)
    border_width: float = field(default=1.0, kw_only=True)
    align: Align = field(default="left", kw_only=True)
    read_only: bool = field(default=False, kw_only=True)
    required: bool = field(default=False, kw_only=True)
    tooltip: str | None = field(default=None, kw_only=True)

    def _common(self, kind: str) -> dict[str, Any]:
        if not isinstance(self.name, str) or not self.name:
            raise ValueError(f"The name of a field must be a text that isn't empty, not {self.name!r}")
        if self.align not in ("left", "center", "right"):
            raise ValueError(f"align must be 'left', 'center' or 'right', not {self.align!r}")
        if not isinstance(self.font_size, (int, float)) or isinstance(self.font_size, bool) or self.font_size < 0:
            raise ValueError(f"font_size must be 0 or more, not {self.font_size!r}")
        if not isinstance(self.border_width, (int, float)) or isinstance(self.border_width, bool) or self.border_width < 0:
            raise ValueError(f"border_width must be 0 or more, not {self.border_width!r}")
        return {
            "kind": kind,
            "page": _page(self.page),
            "name": self.name,
            "font_size": float(self.font_size),
            "color": _color(self.color),
            "border": _color(self.border, "border"),
            "background": _color(self.background, "background"),
            "border_width": float(self.border_width),
            "align": ("left", "center", "right").index(self.align),
            "read_only": bool(self.read_only),
            "required": bool(self.required),
            "tooltip": self.tooltip,
        }


@dataclass(frozen=True)
class TextField(_Field):
    """A box to type text in, with `value` in it. `multiline` lets it take several lines,
    `password` shows dots, and `max_length` limits the characters."""

    box: BoxTuple
    value: str = field(default="", kw_only=True)
    multiline: bool = field(default=False, kw_only=True)
    password: bool = field(default=False, kw_only=True)
    max_length: int | None = field(default=None, kw_only=True)

    def _field(self) -> dict[str, Any]:
        d = self._common("text")
        if not isinstance(self.value, str):
            raise TypeError(f"value must be a string, not {self.value!r}")
        if self.max_length is not None and (
            not isinstance(self.max_length, int) or isinstance(self.max_length, bool) or self.max_length < 1
        ):
            raise ValueError(f"max_length must be a whole number from 1, not {self.max_length!r}")
        d.update(
            box=_box(self.box),
            value=self.value,
            multiline=bool(self.multiline),
            password=bool(self.password),
            max_length=self.max_length,
        )
        return d


@dataclass(frozen=True)
class CheckBox(_Field):
    """A box that is on or off."""

    box: BoxTuple
    checked: bool = field(default=False, kw_only=True)

    def _field(self) -> dict[str, Any]:
        d = self._common("checkbox")
        d.update(box=_box(self.box), checked=bool(self.checked))
        return d


@dataclass(frozen=True)
class RadioGroup(_Field):
    """Round buttons of which one is on, as one field. `buttons` is a list of `(box,
    value)`: the field's value is the value of the button that is on, `selected`."""

    buttons: Sequence[tuple[BoxTuple, str]] = ()
    selected: str | None = field(default=None, kw_only=True)

    def _field(self) -> dict[str, Any]:
        d = self._common("radio")
        buttons = []
        for item in self.buttons:
            try:
                box, value = item
            except (TypeError, ValueError):
                raise ValueError(f"A button is (box, value), not {item!r}") from None
            if not isinstance(value, str):
                raise TypeError(f"The value of a button must be a string, not {value!r}")
            buttons.append((_box(box), value))
        d.update(buttons=buttons, selected=self.selected)
        return d


@dataclass(frozen=True)
class Dropdown(_Field):
    """A drop-down list. `options` are texts, or `(value, text)` when the value it gives
    is another; `value` is the one chosen, and `editable` lets the user type another."""

    box: BoxTuple
    options: Sequence[Any] = ()
    value: str | None = field(default=None, kw_only=True)
    editable: bool = field(default=False, kw_only=True)

    def _field(self) -> dict[str, Any]:
        d = self._common("combo")
        d.update(box=_box(self.box), options=_options(self.options), value=self.value, editable=bool(self.editable))
        return d


@dataclass(frozen=True)
class ListBox(_Field):
    """A list of options to choose from; `selected` are the values chosen, and `multiple`
    lets it take several."""

    box: BoxTuple
    options: Sequence[Any] = ()
    selected: Sequence[str] = field(default=(), kw_only=True)
    multiple: bool = field(default=False, kw_only=True)

    def _field(self) -> dict[str, Any]:
        d = self._common("list")
        selected = [self.selected] if isinstance(self.selected, str) else list(self.selected)
        d.update(box=_box(self.box), options=_options(self.options), selected=selected, multiple=bool(self.multiple))
        return d


@dataclass(frozen=True)
class Button(_Field):
    """A push button with a `caption`; it opens `uri` when clicked, if it has one."""

    box: BoxTuple
    caption: str = ""
    uri: str | None = field(default=None, kw_only=True)

    def _field(self) -> dict[str, Any]:
        d = self._common("button")
        if not isinstance(self.caption, str):
            raise TypeError(f"caption must be a string, not {self.caption!r}")
        d.update(box=_box(self.box), caption=self.caption, uri=self.uri)
        return d


@dataclass(frozen=True)
class SignatureField(_Field):
    """A place to sign, left empty."""

    box: BoxTuple

    def _field(self) -> dict[str, Any]:
        d = self._common("signature")
        d.update(box=_box(self.box))
        return d
