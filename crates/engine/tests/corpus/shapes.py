"""Makes the shapes of the rasterizer's tests: `shapes.tsv`, one shape in each
line, and in `shapes/` the coverage of each pixel of its 200 by 200 pixels as
cairo gives it, one byte each, rows from the top.

A line has the shape's name, what is done with it (`nonzero` and `evenodd`
fill it with each rule, `stroke` strokes it), the line style for strokes, and
the path as `M x y`, `L x y`, `C x1 y1 x2 y2 x3 y3` and `Z`.

Needs pycairo. Run from this folder:

    python shapes.py
"""

from __future__ import annotations

import math
from pathlib import Path

import cairo

HERE = Path(__file__).resolve().parent
SIZE = 200

CAPS = {"butt": cairo.LINE_CAP_BUTT, "round": cairo.LINE_CAP_ROUND, "square": cairo.LINE_CAP_SQUARE}
JOINS = {"miter": cairo.LINE_JOIN_MITER, "round": cairo.LINE_JOIN_ROUND, "bevel": cairo.LINE_JOIN_BEVEL}


def circle(cx: float, cy: float, r: float) -> str:
    k = 0.5522847498 * r
    return (
        f"M {cx + r} {cy} C {cx + r} {cy + k} {cx + k} {cy + r} {cx} {cy + r} "
        f"C {cx - k} {cy + r} {cx - r} {cy + k} {cx - r} {cy} "
        f"C {cx - r} {cy - k} {cx - k} {cy - r} {cx} {cy - r} "
        f"C {cx + k} {cy - r} {cx + r} {cy - k} {cx + r} {cy} Z"
    )


def star(cx: float, cy: float, r: float) -> str:
    points = [
        (cx + r * math.sin(4 * math.pi * i / 5), cy - r * math.cos(4 * math.pi * i / 5))
        for i in range(5)
    ]
    return "M " + " L ".join(f"{x:.4f} {y:.4f}" for x, y in points) + " Z"


SHAPES = [
    ("circle", "nonzero", "-", circle(100.3, 99.6, 70)),
    ("star-nonzero", "nonzero", "-", star(100, 104, 85)),
    ("star-evenodd", "evenodd", "-", star(100, 104, 85)),
    ("ring-evenodd", "evenodd", "-", circle(100, 100, 80) + " " + circle(100, 100, 45)),
    ("blob", "nonzero", "-", "M 30 100 C 30 20 170 20 170 100 C 170 190 40 150 30 100 Z"),
    ("sliver", "nonzero", "-", "M 20.5 20.5 L 180.2 22.1 L 20.5 21.3 Z"),
    ("line-thin", "stroke", "w=1 cap=butt join=miter miter=10 dash=- phase=0", "M 20.5 30.5 L 180.5 70.2"),
    ("line-half", "stroke", "w=0.5 cap=butt join=miter miter=10 dash=- phase=0", "M 20 30 L 180 60"),
    ("caps-butt", "stroke", "w=14 cap=butt join=miter miter=10 dash=- phase=0", "M 30 100 L 170 100"),
    ("caps-round", "stroke", "w=14 cap=round join=miter miter=10 dash=- phase=0", "M 30 100 L 170 70"),
    ("caps-square", "stroke", "w=14 cap=square join=miter miter=10 dash=- phase=0", "M 30 100 L 170 130"),
    ("join-miter", "stroke", "w=16 cap=butt join=miter miter=10 dash=- phase=0", "M 30 160 L 100 40 L 170 160"),
    ("join-round", "stroke", "w=16 cap=butt join=round miter=10 dash=- phase=0", "M 30 160 L 100 40 L 170 160"),
    ("join-bevel", "stroke", "w=16 cap=butt join=bevel miter=10 dash=- phase=0", "M 30 160 L 100 40 L 170 160"),
    ("miter-limit", "stroke", "w=12 cap=butt join=miter miter=2 dash=- phase=0", "M 30 100 L 170 90 L 30 80"),
    ("closed-square", "stroke", "w=12 cap=butt join=miter miter=10 dash=- phase=0", "M 50 50 L 150 50 L 150 150 L 50 150 Z"),
    ("closed-round", "stroke", "w=10 cap=butt join=round miter=10 dash=- phase=0", "M 50 50 L 150 70 L 130 150 L 60 140 Z"),
    ("curve-stroke", "stroke", "w=12 cap=round join=round miter=10 dash=- phase=0", "M 20 150 C 40 20 160 20 180 150"),
    ("dashed-line", "stroke", "w=6 cap=butt join=miter miter=10 dash=12,6 phase=0", "M 15 40 L 185 40"),
    ("dashed-phase", "stroke", "w=6 cap=butt join=miter miter=10 dash=12,6 phase=5", "M 15 60 L 185 60"),
    ("dashed-round", "stroke", "w=8 cap=round join=round miter=10 dash=0,14 phase=0", "M 15 80 L 185 80"),
    ("dashed-circle", "stroke", "w=5 cap=butt join=miter miter=10 dash=10,5,3,5 phase=0", circle(100, 120, 60)),
]


def build(context: cairo.Context, path: str) -> None:
    tokens = path.split()
    i = 0
    while i < len(tokens):
        op = tokens[i]
        if op == "M":
            context.move_to(float(tokens[i + 1]), float(tokens[i + 2]))
            i += 3
        elif op == "L":
            context.line_to(float(tokens[i + 1]), float(tokens[i + 2]))
            i += 3
        elif op == "C":
            context.curve_to(*(float(t) for t in tokens[i + 1 : i + 7]))
            i += 7
        elif op == "Z":
            context.close_path()
            i += 1
        else:
            raise ValueError(op)


def main() -> None:
    folder = HERE / "shapes"
    folder.mkdir(exist_ok=True)
    rows = ["# name\toperation\tstyle\tpath"]
    for name, operation, style, path in SHAPES:
        surface = cairo.ImageSurface(cairo.FORMAT_A8, SIZE, SIZE)
        context = cairo.Context(surface)
        context.set_antialias(cairo.ANTIALIAS_GRAY)
        context.set_tolerance(0.02)
        context.set_source_rgba(0, 0, 0, 1)
        build(context, path)
        if operation == "stroke":
            settings = dict(item.split("=") for item in style.split())
            context.set_line_width(float(settings["w"]))
            context.set_line_cap(CAPS[settings["cap"]])
            context.set_line_join(JOINS[settings["join"]])
            context.set_miter_limit(float(settings["miter"]))
            if settings["dash"] != "-":
                context.set_dash([float(d) for d in settings["dash"].split(",")], float(settings["phase"]))
            context.stroke()
        else:
            context.set_fill_rule(
                cairo.FILL_RULE_EVEN_ODD if operation == "evenodd" else cairo.FILL_RULE_WINDING
            )
            context.fill()
        surface.flush()
        stride = surface.get_stride()
        data = bytes(surface.get_data())
        rows_data = b"".join(data[y * stride : y * stride + SIZE] for y in range(SIZE))
        (folder / f"{name}.raw").write_bytes(rows_data)
        rows.append(f"{name}\t{operation}\t{style}\t{path}")
    (HERE / "shapes.tsv").write_text("\n".join(rows) + "\n")


if __name__ == "__main__":
    main()
