"""Draws the bars of the README (`assets/speed-light.svg` and `speed-dark.svg`) from
the numbers of `compare_libraries.py`: change `ROWS` and run it from the root of the
repository."""

from pathlib import Path

# (what is timed, [(library, milliseconds or None if it can't do it)]), median of one call.
ROWS = [
    ("Text of 200 pages full of text", [("ozaru", 238), ("PyMuPDF", 324), ("pypdf", 2049)]),
    ("Words with their boxes, 200 pages", [("ozaru", 320), ("PyMuPDF", 432), ("pypdf", None)]),
    ("Draw a page of text (PNG)", [("ozaru", 14), ("PyMuPDF", 22), ("pypdf", None)]),
    ("Draw a page that is a photograph (PNG)", [("ozaru", 48), ("PyMuPDF", 104), ("pypdf", None)]),
]


def label(value: float) -> str:
    return f"{value:g} ms" if value < 10 else f"{value:.0f} ms"


def svg(dark: bool) -> str:
    bg, fg, muted = ("#1F272D", "#E8ECEF", "#8A97A3") if dark else ("#FFFFFF", "#1B2A49", "#6B7785")
    other, track = ("#5A6875", "#2A343C") if dark else ("#B7C0C9", "#F1F4F7")
    width, left, bar = 900, 60, 520
    height = 70 + len(ROWS) * 150 + 20
    y = 70
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" font-family="Helvetica, Arial, sans-serif">',
        f'<rect width="{width}" height="{height}" rx="16" fill="{bg}"/>',
        f'<text x="{left}" y="46" font-size="26" font-weight="700" fill="{fg}">Time to do the same work</text>',
        f'<text x="{width - 60}" y="46" font-size="15" text-anchor="end" fill="{muted}">milliseconds, shorter is better</text>',
    ]
    for title, items in ROWS:
        y += 20
        out.append(f'<text x="{left}" y="{y}" font-size="18" font-weight="600" fill="{fg}">{title}</text>')
        top = max(v for _, v in items if v)
        y += 14
        for name, value in items:
            if value is None:
                out.append(f'<text x="{left}" y="{y + 17}" font-size="15" fill="{muted}">{name}</text>')
                out.append(f'<text x="{left + 110}" y="{y + 17}" font-size="14" fill="{muted}">no equivalent</text>')
                y += 34
                continue
            w = max(4, bar * value / top)
            ours = name == "ozaru"
            color = "#F47B20" if ours else other
            weight = "700" if ours else "400"
            out.append(f'<text x="{left}" y="{y + 17}" font-size="15" font-weight="{weight}" fill="{fg}">{name}</text>')
            out.append(f'<rect x="{left + 110}" y="{y + 2}" width="{bar}" height="20" rx="4" fill="{track}"/>')
            out.append(f'<rect x="{left + 110}" y="{y + 2}" width="{w:.1f}" height="20" rx="4" fill="{color}"/>')
            out.append(f'<text x="{left + 110 + w + 10:.1f}" y="{y + 17}" font-size="14" font-weight="{weight}" fill="{fg}">{label(value)}</text>')
            y += 34
        y += 6
    out.append("</svg>")
    return "\n".join(out)


if __name__ == "__main__":
    for name, dark in (("light", False), ("dark", True)):
        Path(f"assets/speed-{name}.svg").write_text(svg(dark), encoding="utf-8")
