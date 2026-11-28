"""The `ozaru` command: the functions of the library from the terminal.

    uvx ozaru text report.pdf
    uvx ozaru info https://example.com/report.pdf
    cat report.pdf | uvx ozaru words -
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from pathlib import Path
from typing import Any, Sequence

from . import ozaru as _native
from ._redact import redact_text
from ._annotate import Link, Note, delete_annotations, highlight


def _source(text: str) -> Any:
    """The PDF a command line argument names: a path, a URL, or `-` for stdin."""
    if text == "-":
        return sys.stdin.buffer.read()
    return text


def _pages(spec: str) -> list[int]:
    """Page numbers from `1,3-5`, counted from 1."""
    pages: list[int] = []
    for part in spec.split(","):
        first, dash, last = part.strip().partition("-")
        try:
            low = int(first)
            high = int(last) if dash else low
        except ValueError:
            raise argparse.ArgumentTypeError(f"not a list of pages: {spec!r}") from None
        if low < 1 or high < low:
            raise argparse.ArgumentTypeError(f"not a list of pages: {spec!r}")
        pages.extend(range(low, high + 1))
    return pages


def _json_default(value: object) -> object:
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, bytes):
        return None
    fields = getattr(value, "fields", None)
    if callable(fields):
        return {name: field for name, field in fields()}
    raise TypeError(f"{type(value).__name__} is not JSON serializable")


def _print_json(value: object) -> None:
    print(json.dumps(value, indent=2, ensure_ascii=False, default=_json_default))


def _record(item: object, names: Sequence[str]) -> dict[str, object]:
    return {name: getattr(item, name) for name in names}


WORD = ("page", "block", "line", "text", "x0", "y0", "x1", "y1")
BLOCK = ("page", "number", "text", "x0", "y0", "x1", "y1")
IMAGE = ("page", "number", "x0", "y0", "x1", "y1", "width", "height", "format")
SPAN = ("page", "block", "line", "font", "size", "bold", "italic", "text", "x0", "y0", "x1", "y1")
ANNOTATION = ("page", "subtype", "rect", "contents", "name", "author", "uri", "target_page")
FIELD = ("name", "kind", "value", "default_value", "read_only", "required", "page", "rect")


def _text(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    if args.pages:
        pages = [pdf[number - 1].text for number in args.pages]
        sys.stdout.write("\x0c".join(pages))
    else:
        sys.stdout.write(pdf.text)
    sys.stdout.write("\n")


def _count(args: argparse.Namespace) -> None:
    print(_native.page_count(_source(args.file), password=args.password))


def _info(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    metadata = pdf.metadata
    _print_json(
        {
            "pages": len(pdf),
            "permissions": pdf.permissions,
            "metadata": {
                name: getattr(metadata, name)
                for name in (
                    "title",
                    "author",
                    "subject",
                    "keywords",
                    "creator",
                    "producer",
                    "creation_date",
                    "modification_date",
                )
            },
            "page_sizes": [
                {"number": p.number, "width": p.width, "height": p.height, "rotation": p.rotation}
                for p in pdf.page_info
            ],
        }
    )


def _listing(names: Sequence[str], getter: str):  # noqa: ANN202
    def run(args: argparse.Namespace) -> None:
        pdf = _native.open(_source(args.file), password=args.password)
        items = getattr(pdf, getter)
        if args.json:
            _print_json([_record(item, names) for item in items])
            return
        for item in items:
            row = [str(getattr(item, name)).replace("\t", " ").replace("\n", " ") for name in names]
            print("\t".join(row))

    return run


def _bookmarks(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    items = pdf.bookmarks
    if args.json:
        _print_json([_record(item, ("title", "level", "page", "uri", "open")) for item in items])
        return
    for item in items:
        where = f"page {item.page}" if item.page else item.uri or ""
        print(f"{'  ' * (item.level - 1)}{item.title}\t{where}".rstrip())


def _labels(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    for page, label in enumerate(pdf.page_labels, start=1):
        print(f"{page}\t{label}")


def _attachments(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    folder = Path(args.output) if args.output else None
    if folder:
        folder.mkdir(parents=True, exist_ok=True)
    for attachment in pdf.attachments:
        if folder is None:
            print(f"{attachment.name}\t{attachment.size if attachment.size is not None else ''}")
            continue
        if attachment.data is None:
            print(f"ozaru: {attachment.name}: the contents can't be read", file=sys.stderr)
            continue
        # The name comes from the PDF: only its last part is used.
        name = Path((attachment.filename or attachment.name).replace("\\", "/")).name or "attachment"
        path = folder / name
        attachment.save(path)
        print(path)


def _set_metadata(args: argparse.Namespace) -> None:
    values: dict[str, Any] = {}
    for name in ("title", "author", "subject", "keywords", "creator", "producer"):
        value = getattr(args, name)
        if value is not None:
            values[name] = value
    for name in args.remove:
        values[name] = None
    if not values:
        raise SystemExit("ozaru: nothing to change: give --title, --author... or --remove")
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.set_metadata(args.output, values)
    print(args.output)


def _delete_pages(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.delete_pages(args.output, args.pages)
    print(args.output)


def _crop(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.crop(args.output, tuple(args.box), pages=args.pages)
    print(args.output)


def _blank_page(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.add_blank_page(args.output, position=args.position, width=args.width, height=args.height)
    print(args.output)


def _attach(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.add_attachment(
        args.output,
        args.name or Path(args.attachment).name,
        Path(args.attachment),
        description=args.description,
    )
    print(args.output)


def _search(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    found = pdf.search(args.text, case_sensitive=args.case_sensitive, pages=args.pages)
    if args.json:
        _print_json([{"page": m.page, "text": m.text, "boxes": m.boxes} for m in found])
    else:
        for match in found:
            x0, y0, x1, y1 = match.bbox
            print(f"{match.page}\t{x0:.1f}\t{y0:.1f}\t{x1:.1f}\t{y1:.1f}\t{match.text}")
    if not found:
        raise SystemExit(1)


def _tables(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    found = pdf.find_tables(strategy=args.strategy)
    if args.json:
        _print_json([{"page": t.page, "bbox": t.bbox, "ruled": t.ruled, "rows": t.rows} for t in found])
        return
    for number, table in enumerate(found, start=1):
        print(f"<!-- table {number}, page {table.page} -->")
        print(table.markdown)
        print()


def _markdown(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    text = pdf.to_markdown(page_markers=args.page_markers, strategy=args.strategy)
    sys.stdout.write(text)
    sys.stdout.write("\n")


def _watermark(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.add_watermark(
        args.output,
        args.text,
        pages=args.pages,
        font=args.font,
        size=args.size,
        color=args.color,
        opacity=args.opacity,
        rotation=args.rotation,
    )
    print(args.output)


def _highlight(args: argparse.Namespace) -> None:
    count = highlight(
        _source(args.file),
        args.output,
        args.text,
        kind=args.kind,
        case_sensitive=args.case_sensitive,
        pages=args.pages,
        color=args.color,
        opacity=args.opacity,
        note=args.note,
        author=args.author,
        password=args.password,
    )
    print(f"{args.output}: {count} marked")


def _redact(args: argparse.Namespace) -> None:
    count = redact_text(
        _source(args.file),
        args.output,
        args.text,
        case_sensitive=args.case_sensitive,
        pages=args.pages,
        fill=None if args.fill == "none" else args.fill,
        images=args.images,
        graphics=args.graphics,
        password=args.password,
    )
    print(f"{args.output}: {count} redacted")


def _flatten(args: argparse.Namespace) -> None:
    from . import ozaru as native

    pdf = native.open(_source(args.file), password=args.password)
    if args.annotations:
        count = pdf.flatten_annotations(args.output, pages=args.pages, subtypes=args.subtype)
    else:
        count = pdf.flatten_form(args.output, fields=args.field, pages=args.pages)
    print(f"{args.output}: {count} flattened")


def _add_note(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.annotate(args.output, [Note(args.page, args.x, args.y, args.text, author=args.author, open=args.open)])
    print(args.output)


def _add_link(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    link = Link(
        args.page, (args.x0, args.y0, args.x1, args.y1), uri=args.uri, to_page=args.to_page, border=args.border
    )
    pdf.annotate(args.output, [link])
    print(args.output)


def _delete_annotations(args: argparse.Namespace) -> None:
    count = delete_annotations(
        _source(args.file),
        args.output,
        pages=args.pages,
        subtypes=args.subtype,
        author=args.author,
        contains=args.contains,
        password=args.password,
    )
    print(f"{args.output}: {count} removed")


def _add_image(args: argparse.Namespace) -> None:
    from ._draw import Picture

    pdf = _native.open(_source(args.file), password=args.password)
    item = Picture(
        args.page,
        args.x,
        args.y,
        args.image,
        width=args.width,
        height=args.height,
        opacity=args.opacity,
        rotation=args.rotation,
    )
    pdf.draw(args.output, [item])
    print(args.output)


def _add_text(args: argparse.Namespace) -> None:
    from ._draw import Text

    pdf = _native.open(_source(args.file), password=args.password)
    item = Text(
        args.page,
        args.x,
        args.y,
        args.text,
        font=args.font,
        size=args.size,
        color=args.color,
        opacity=args.opacity,
        rotation=args.rotation,
    )
    pdf.draw(args.output, [item])
    print(args.output)


def _images(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    folder = Path(args.output)
    folder.mkdir(parents=True, exist_ok=True)
    saved = 0
    for image in pdf.images:
        if image.data is None:
            print(f"ozaru: page {image.page}, image {image.number}: {image.missing}", file=sys.stderr)
            continue
        path = folder / f"page{image.page}-image{image.number}.{image.format}"
        image.save(path)
        print(path)
        saved += 1
    if not saved:
        print("ozaru: no images saved", file=sys.stderr)


def _render(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    png = pdf.render(args.page, scale=args.scale, annotations=not args.no_annotations)
    if args.output == "-":
        sys.stdout.buffer.write(png)
    else:
        Path(args.output).write_bytes(png)
        print(args.output)


def _rotate(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.rotate(args.output, args.degrees, pages=args.pages)
    print(args.output)


def _extract_pages(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.extract_pages(args.output, args.pages)
    print(args.output)


def _split(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    for path in pdf.split(args.output, pages_per_file=args.pages_per_file):
        print(path)


def _merge(args: argparse.Namespace) -> None:
    _native.merge([_source(file) for file in args.files], args.output, password=args.password)
    print(args.output)


def _encrypt(args: argparse.Namespace) -> None:
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.encrypt(
        args.output,
        user_password=args.user_password,
        owner_password=args.owner_password,
    )
    print(args.output)


def _fill(args: argparse.Namespace) -> None:
    values: dict[str, Any] = {}
    for item in args.values:
        name, equals, value = item.partition("=")
        if not equals:
            raise SystemExit(f"ozaru: {item!r} is not name=value")
        values[name] = {"true": True, "false": False}.get(value.lower(), value)
    pdf = _native.open(_source(args.file), password=args.password)
    pdf.fill_form(args.output, values)
    print(args.output)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ozaru",
        description="Read, edit and protect PDF files. FILE is a path, a URL, or - for stdin.",
    )
    parser.add_argument("--version", action="version", version="ozaru " + _version())
    commands = parser.add_subparsers(dest="command", required=True, metavar="command")

    def command(name: str, help: str, run, *, many: bool = False) -> argparse.ArgumentParser:  # noqa: ANN001
        sub = commands.add_parser(name, help=help, description=help)
        if many:
            sub.add_argument("files", nargs="+", metavar="FILE")
        else:
            sub.add_argument("file", metavar="FILE")
        sub.add_argument("-p", "--password", help="password of the PDF")
        sub.set_defaults(run=run)
        return sub

    sub = command("text", "print the text", _text)
    sub.add_argument("--pages", type=_pages, help="only these pages, such as 1,3-5")
    command("count", "print the number of pages", _count)
    command("info", "print the metadata and the size of each page, as JSON", _info)
    for name, help, names, getter in (
        ("words", "print every word with its box", WORD, "words"),
        ("blocks", "print every block of text with its box", BLOCK, "blocks"),
        ("annotations", "print the annotations", ANNOTATION, "annotations"),
        ("fields", "print the fields of the form", FIELD, "form_fields"),
    ):
        sub = command(name, help, _listing(names, getter))
        sub.add_argument("--json", action="store_true", help="JSON instead of tab-separated")
    sub = command("search", "find text and print where it is (exit 1 if it isn't there)", _search)
    sub.add_argument("text", metavar="TEXT")
    sub.add_argument("--case-sensitive", action="store_true")
    sub.add_argument("--pages", type=_pages, help="only these pages, such as 1,3-5")
    sub.add_argument("--json", action="store_true", help="JSON instead of tab-separated")
    sub = command("spans", "print the pieces of text with one font, size and color", _listing(SPAN, "spans"))
    sub.add_argument("--json", action="store_true", help="JSON instead of tab-separated")
    sub = command("tables", "print the tables as Markdown", _tables)
    sub.add_argument("--strategy", choices=["lines", "text", "both"], default="both")
    sub.add_argument("--json", action="store_true", help="JSON instead of Markdown")
    sub = command("markdown", "print the document as Markdown", _markdown)
    sub.add_argument("--page-markers", action="store_true", help="a comment before each page")
    sub.add_argument("--strategy", choices=["lines", "text", "both"], default="both", help="how to find tables")
    sub = command("bookmarks", "print the bookmarks, indented by level", _bookmarks)
    sub.add_argument("--json", action="store_true", help="JSON instead of an outline")
    command("labels", "print the label of each page (i, ii, 1, A-1...)", _labels)
    sub = command("attachments", "list the attached files, or save them with -o", _attachments)
    sub.add_argument("-o", "--output", help="folder to save them into")
    sub = command("images", "save the images to a folder", _images)
    sub.add_argument("-o", "--output", default="images", help="folder (default: images)")
    sub = command("render", "draw a page as a PNG file", _render)
    sub.add_argument("page", type=int, help="page number, from 1")
    sub.add_argument("-o", "--output", required=True, help="PNG file, or - for stdout")
    sub.add_argument("--scale", type=float, default=1.0, help="pixels per point (default: 1)")
    sub.add_argument("--no-annotations", action="store_true", help="don't draw annotations")
    sub = command("rotate", "turn pages clockwise", _rotate)
    sub.add_argument("degrees", type=int, help="a multiple of 90")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub = command("extract-pages", "save some pages to a new file", _extract_pages)
    sub.add_argument("pages", type=_pages, help="pages in the order wanted, such as 3,1,5-7")
    sub.add_argument("-o", "--output", required=True)
    sub = command("split", "save each page, or group of pages, as a file in a folder", _split)
    sub.add_argument("-o", "--output", required=True, help="folder")
    sub.add_argument("--pages-per-file", type=int, default=1)
    sub = command("merge", "join PDFs, in order, into one", _merge, many=True)
    sub.add_argument("-o", "--output", required=True)
    sub = command("set-metadata", "change the title, author and other information", _set_metadata)
    for name in ("title", "author", "subject", "keywords", "creator", "producer"):
        sub.add_argument(f"--{name}")
    sub.add_argument("--remove", action="append", default=[], metavar="NAME", help="remove a value, such as author")
    sub.add_argument("-o", "--output", required=True)
    sub = command("delete-pages", "remove pages", _delete_pages)
    sub.add_argument("pages", type=_pages, help="pages to remove, such as 2,5-7")
    sub.add_argument("-o", "--output", required=True)
    sub = command("crop", "crop pages to a box measured from the top left", _crop)
    sub.add_argument("box", type=float, nargs=4, metavar=("X0", "Y0", "X1", "Y1"))
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub.add_argument("-o", "--output", required=True)
    sub = command("blank-page", "add an empty page", _blank_page)
    sub.add_argument("--position", type=int, help="page number it goes before (default: the end)")
    sub.add_argument("--width", type=float)
    sub.add_argument("--height", type=float)
    sub.add_argument("-o", "--output", required=True)
    sub = command("attach", "attach a file", _attach)
    sub.add_argument("attachment", metavar="ATTACHMENT", help="the file to attach")
    sub.add_argument("--name", help="its name in the PDF (default: its file name)")
    sub.add_argument("--description")
    sub.add_argument("-o", "--output", required=True)
    sub = command("watermark", "write a text across the middle of the pages", _watermark)
    sub.add_argument("text", metavar="TEXT")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub.add_argument("--font", default="Helvetica-Bold", help="a standard font or the path of a .ttf file")
    sub.add_argument("--size", type=float, help="in points (default: 70%% of the diagonal)")
    sub.add_argument("--color", default="gray", help="a name, #rrggbb (default: gray)")
    sub.add_argument("--opacity", type=float, default=0.3)
    sub.add_argument("--rotation", type=float, default=45.0, help="degrees counterclockwise")
    sub = command("highlight", "mark every place a text is found (highlight, underline, strike out)", _highlight)
    sub.add_argument("text", metavar="TEXT")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--kind", choices=["highlight", "underline", "strikeout", "squiggly"], default="highlight")
    sub.add_argument("--case-sensitive", action="store_true")
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub.add_argument("--color", help="a name or #rrggbb (default: yellow for highlights, red for the rest)")
    sub.add_argument("--opacity", type=float, default=1.0)
    sub.add_argument("--note", help="a comment that goes with each mark")
    sub.add_argument("--author")
    sub = command("redact", "remove a text from the PDF for good, and draw a box where it was", _redact)
    sub.add_argument("text", metavar="TEXT")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--case-sensitive", action="store_true")
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub.add_argument("--fill", default="black", help="color of the box, or none (default: black)")
    sub.add_argument("--images", choices=["pixels", "remove", "keep"], default="pixels")
    sub.add_argument("--graphics", choices=["covered", "touched", "keep"], default="covered")
    sub = command("flatten", "make the form fields (or the annotations) part of the page", _flatten)
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub.add_argument("--field", action="append", help="only this field (repeat for several; default: all)")
    sub.add_argument("--annotations", action="store_true", help="flatten annotations, not form fields")
    sub.add_argument("--subtype", action="append", help="with --annotations: only these, such as Highlight")
    sub = command("add-note", "put a sticky note on a page, its top left corner at a position", _add_note)
    sub.add_argument("page", type=int, help="page number, from 1")
    sub.add_argument("x", type=float)
    sub.add_argument("y", type=float)
    sub.add_argument("text", metavar="TEXT")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--author")
    sub.add_argument("--open", action="store_true", help="show the note open")
    sub = command("add-link", "make an area of a page a link to a web address or a page", _add_link)
    sub.add_argument("page", type=int, help="page number, from 1")
    for corner in ("x0", "y0", "x1", "y1"):
        sub.add_argument(corner, type=float)
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--uri", help="a web address")
    sub.add_argument("--to-page", type=int, help="a page of this document, from 1")
    sub.add_argument("--border", type=float, default=0.0, help="width of a blue frame (default: none)")
    sub = command("delete-annotations", "remove annotations (all, or those that match)", _delete_annotations)
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--pages", type=_pages, help="only these pages (default: all)")
    sub.add_argument("--subtype", action="append", help="such as Highlight or Link (repeat for several)")
    sub.add_argument("--author")
    sub.add_argument("--contains", help="text the comment has")
    sub = command("add-image", "put a JPEG or PNG picture on a page, its top left corner at a position", _add_image)
    sub.add_argument("page", type=int, help="page number, from 1")
    sub.add_argument("x", type=float)
    sub.add_argument("y", type=float)
    sub.add_argument("image", metavar="IMAGE", help="a .jpg or .png file")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--width", type=float, help="in points (the height keeps the shape)")
    sub.add_argument("--height", type=float, help="in points")
    sub.add_argument("--opacity", type=float, default=1.0)
    sub.add_argument("--rotation", type=float, default=0.0, help="degrees counterclockwise")
    sub = command("add-text", "write a text at a position, from the top left of the page", _add_text)
    sub.add_argument("page", type=int, help="page number, from 1")
    sub.add_argument("x", type=float)
    sub.add_argument("y", type=float)
    sub.add_argument("text", metavar="TEXT")
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--font", default="Helvetica", help="a standard font or the path of a .ttf file")
    sub.add_argument("--size", type=float, default=12.0)
    sub.add_argument("--color", default="black")
    sub.add_argument("--opacity", type=float, default=1.0)
    sub.add_argument("--rotation", type=float, default=0.0)
    sub = command("encrypt", "protect a PDF with a password (AES-256)", _encrypt)
    sub.add_argument("-o", "--output", required=True)
    sub.add_argument("--user-password", default="", help="password to open it")
    sub.add_argument("--owner-password", help="password to change it (default: the user's)")
    sub = command("fill", "fill form fields: name=value, and true or false for check boxes", _fill)
    sub.add_argument("values", nargs="+", metavar="name=value")
    sub.add_argument("-o", "--output", required=True)
    return parser


def _version() -> str:
    from importlib.metadata import PackageNotFoundError, version

    try:
        return version("ozaru")
    except PackageNotFoundError:
        return "unknown"


def _utf8_output() -> None:
    """Text comes out as UTF-8 with `\\n` line ends on every system, not in the
    code page of the Windows console, which can't write most scripts."""
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", newline="\n")  # type: ignore[union-attr]
        except (AttributeError, ValueError):
            pass


def main(argv: Sequence[str] | None = None) -> int:
    _utf8_output()
    args = _parser().parse_args(argv)
    # `fill` takes its PDF before the values: the first positional is the file.
    if args.command == "fill" and not hasattr(args, "file"):
        raise SystemExit("ozaru: fill needs a file")
    try:
        args.run(args)
    except BrokenPipeError:
        # `ozaru text file.pdf | head`: the reader left, which is fine.
        sys.stderr.close()
        return 0
    except (OSError, ValueError, TypeError) as error:
        print(f"ozaru: {error}", file=sys.stderr)
        return 1
    return 0
