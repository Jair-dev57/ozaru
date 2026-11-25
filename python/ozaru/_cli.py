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
