"""Comparing two PDFs: what changed between a version and another."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal

Box = tuple[float, float, float, float]
Status = Literal["same", "changed", "added", "removed"]


@dataclass(frozen=True)
class TextChange:
    """A place where the words of a page changed. Boxes are in points from the top left
    of the page, as the boxes of words are: `removed_boxes` on the page of the first PDF
    and `added_boxes` on the page of the second."""

    removed: str
    added: str
    removed_boxes: list[Box]
    added_boxes: list[Box]

    def __str__(self) -> str:
        if self.removed and self.added:
            return f"{self.removed!r} -> {self.added!r}"
        if self.removed:
            return f"- {self.removed!r}"
        return f"+ {self.added!r}"


@dataclass(frozen=True)
class PageComparison:
    """A page of the first PDF and its pair in the second."""

    page_a: int | None
    """The page of the first PDF, from 1, or `None` if the page was added."""
    page_b: int | None
    """The page of the second PDF, from 1, or `None` if the page was removed."""
    status: Status
    changes: list[TextChange]
    size_changed: bool
    difference: float | None
    """With `visual=True`: the share of the page that looks different, from 0 to 1."""
    regions: list[Box]
    """With `visual=True`: the places that look different, in points from the top left."""


@dataclass(frozen=True)
class InfoChange:
    """An entry of the information of the documents (`Title`, `Author`...) that differs."""

    key: str
    old: str | None
    new: str | None


@dataclass(frozen=True)
class FieldChange:
    """A form field whose value differs, or that is in only one PDF (`None`)."""

    name: str
    old: str | None
    new: str | None


@dataclass(frozen=True)
class Comparison:
    """What differs between two PDFs."""

    pages: list[PageComparison]
    info: list[InfoChange]
    bookmarks_removed: list[str]
    bookmarks_added: list[str]
    attachments_removed: list[str]
    attachments_added: list[str]
    fields: list[FieldChange]
    _sources: tuple[Any, Any, str | None, str | None] = field(default=(None, None, None, None), repr=False, compare=False)

    @property
    def identical(self) -> bool:
        """Whether nothing that was compared differs."""
        return (
            all(p.status == "same" for p in self.pages)
            and not self.info
            and not self.bookmarks_removed
            and not self.bookmarks_added
            and not self.attachments_removed
            and not self.attachments_added
            and not self.fields
        )

    @property
    def changed_pages(self) -> list[PageComparison]:
        """The pages that were added, removed or changed."""
        return [p for p in self.pages if p.status != "same"]

    def summary(self) -> str:
        """The differences as text, for people."""
        if self.identical:
            return "The PDFs are identical."
        lines: list[str] = []
        counts = {s: sum(1 for p in self.pages if p.status == s) for s in ("same", "changed", "added", "removed")}
        lines.append(
            f"Pages: {counts['same']} same, {counts['changed']} changed, {counts['added']} added, {counts['removed']} removed"
        )
        for page in self.changed_pages:
            if page.status == "added":
                lines.append(f"  page {page.page_b} of the second PDF was added")
            elif page.status == "removed":
                lines.append(f"  page {page.page_a} of the first PDF was removed")
            else:
                where = f"page {page.page_a}" if page.page_a == page.page_b else f"page {page.page_a} -> {page.page_b}"
                note = " (a different size)" if page.size_changed else ""
                if page.difference is not None and not page.changes and not page.size_changed:
                    note = f" (looks different: {page.difference:.1%} of the page)"
                lines.append(f"  {where}{note}")
                for change in page.changes:
                    lines.append(f"    {change}")
        for entry in self.info:
            lines.append(f"Information {entry.key}: {entry.old!r} -> {entry.new!r}")
        for title in self.bookmarks_removed:
            lines.append(f"Bookmark removed: {title!r}")
        for title in self.bookmarks_added:
            lines.append(f"Bookmark added: {title!r}")
        for name in self.attachments_removed:
            lines.append(f"Attachment removed: {name!r}")
        for name in self.attachments_added:
            lines.append(f"Attachment added: {name!r}")
        for entry in self.fields:
            lines.append(f"Field {entry.name}: {entry.old!r} -> {entry.new!r}")
        return "\n".join(lines)

    __str__ = summary

    def highlight(self, output_a: Any, output_b: Any) -> None:
        """Save copies of both PDFs with the differences marked: removed words
        highlighted in red on the first, added words in green on the second, and
        a frame where a page only looks different (with `visual=True`)."""
        from . import annotate
        from ._annotate import Box as BoxMark, Highlight

        source_a, source_b, password_a, password_b = self._sources
        if source_a is None:
            raise ValueError("This comparison doesn't know the PDFs it compares")
        marks_a: list[Any] = []
        marks_b: list[Any] = []
        for page in self.pages:
            if page.status != "changed":
                continue
            for change in page.changes:
                if change.removed_boxes and page.page_a is not None:
                    marks_a.append(
                        Highlight(page.page_a, change.removed_boxes, color="#ff9c9c", note=_note("Removed", change.removed))
                    )
                if change.added_boxes and page.page_b is not None:
                    marks_b.append(
                        Highlight(page.page_b, change.added_boxes, color="#9cef9c", note=_note("Added", change.added))
                    )
            if not page.changes:
                for region in page.regions:
                    if page.page_a is not None:
                        marks_a.append(BoxMark(page.page_a, region, color="orange", width=1.5))
                    if page.page_b is not None:
                        marks_b.append(BoxMark(page.page_b, region, color="orange", width=1.5))
        annotate(source_a, output_a, marks_a, password=password_a)
        annotate(source_b, output_b, marks_b, password=password_b)


def _note(kind: str, text: str) -> str:
    return f"{kind}: {text}" if len(text) <= 300 else f"{kind}: {text[:300]}..."


def compare(
    path_a: Any,
    path_b: Any,
    *,
    visual: bool = False,
    scale: float = 1.0,
    threshold: int = 32,
    password_a: str | None = None,
    password_b: str | None = None,
) -> Comparison:
    """Compare two PDFs: which pages were added, removed or changed, which words changed
    on each (and where), and what differs in their information, bookmarks, attachments
    and form fields.

    Pages are paired by what they say, so a page added or removed doesn't make the pages
    after it look changed, and words are compared one by one, so text that wraps
    differently is not a change. With `visual=True` the paired pages are also drawn at
    `scale` and compared as pictures, which finds changes of images and graphics: a pixel
    differs if a color changes by more than `threshold` (0 to 255).

    The result has `identical`, `pages`, `info`, `fields`... and `summary()`; its
    `highlight(output_a, output_b)` saves both PDFs with the differences marked.
    """
    from . import ozaru as native

    if not isinstance(threshold, int) or isinstance(threshold, bool) or not 0 <= threshold <= 255:
        raise ValueError(f"threshold must be a whole number from 0 to 255, not {threshold!r}")
    raw = native._compare(path_a, path_b, bool(visual), float(scale), threshold, password_a, password_b)
    pages = [
        PageComparison(
            page_a=a,
            page_b=b,
            status=status,
            changes=[TextChange(r, ad, list(rb), list(ab)) for r, ad, rb, ab in changes],
            size_changed=size_changed,
            difference=difference,
            regions=list(regions),
        )
        for a, b, status, changes, size_changed, difference, regions in raw["pages"]
    ]
    return Comparison(
        pages=pages,
        info=[InfoChange(*entry) for entry in raw["info"]],
        bookmarks_removed=list(raw["bookmarks"][0]),
        bookmarks_added=list(raw["bookmarks"][1]),
        attachments_removed=list(raw["attachments"][0]),
        attachments_added=list(raw["attachments"][1]),
        fields=[FieldChange(*entry) for entry in raw["fields"]],
        _sources=(path_a, path_b, password_a, password_b),
    )
