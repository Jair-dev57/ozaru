"""Makes the PNG files of the engine's PNG reader tests, in `files/png/`.

Each `name.png` comes with `name.raw`: its pixels as ImageMagick decodes them,
RGBA, 8 bits per sample, rows without padding (16-bit samples are rounded by
ImageMagick and cut to their high byte by the engine: the tests allow 1 of
difference for the files named `16`).

Needs Pillow and ImageMagick. Run from this folder:

    python png.py
"""

from __future__ import annotations

import random
import subprocess
from pathlib import Path

from PIL import Image

OUT = Path(__file__).resolve().parent.parent / "files" / "png"
W, H = 13, 11  # odd sizes, so rows end in the middle of a byte


def source(mode: str) -> Image.Image:
    rng = random.Random(7)
    image = Image.new(mode, (W, H))
    pixels = image.load()
    for y in range(H):
        for x in range(W):
            base = (x * 19 + y * 7) % 256
            if mode in ("L", "P"):
                pixels[x, y] = base if mode == "L" else (x + y) % 16
            elif mode == "LA":
                pixels[x, y] = (base, (x * 20) % 256)
            elif mode == "RGB":
                pixels[x, y] = (base, rng.randrange(256), (y * 23) % 256)
            else:
                pixels[x, y] = (base, rng.randrange(256), (y * 23) % 256, (x * 21 + y * 5) % 256)
    return image


def save(name: str, image: Image.Image, **options) -> None:
    path = OUT / f"{name}.png"
    image.save(path, **options)
    finish(name)


def finish(name: str) -> None:
    path = OUT / f"{name}.png"
    raw = subprocess.run(
        ["magick", str(path), "-depth", "8", "rgba:-"], check=True, capture_output=True
    ).stdout
    assert len(raw) == W * H * 4, (name, len(raw))
    (OUT / f"{name}.raw").write_bytes(raw)


def magick(name: str, args: list[str], image: Image.Image | None = None) -> None:
    """A file ImageMagick makes from the source `image` (saved as a PNG first)."""
    path = OUT / f"{name}.png"
    tmp = OUT / "_tmp.png"
    image.save(tmp)
    subprocess.run(["magick", str(tmp), *args, str(path)], check=True)
    tmp.unlink()
    finish(name)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    save("gray8", source("L"))
    save("rgb8", source("RGB"))
    save("rgba8", source("RGBA"))
    save("gray-alpha8", source("LA"))
    magick("palette8", ["-colors", "40", "-define", "png:color-type=3", "-define", "png:bit-depth=8"], source("RGB"))
    magick("palette4", ["-colors", "16", "-define", "png:color-type=3", "-define", "png:bit-depth=4"], source("RGB"))
    magick("palette2", ["-colors", "4", "-define", "png:color-type=3", "-define", "png:bit-depth=2"], source("RGB"))
    magick("palette-transparent", ["-colors", "16", "-define", "png:color-type=3", "-define", "png:bit-depth=4"], source("RGBA"))
    for depth in (1, 2, 4):
        magick(f"gray{depth}", ["-depth", str(depth), "-colorspace", "Gray", "-define", f"png:bit-depth={depth}", "-define", "png:color-type=0"], source("L"))
    magick("gray16", ["-depth", "16", "-evaluate", "Multiply", "0.7731", "-define", "png:color-type=0"], source("L"))
    magick("rgb16", ["-depth", "16", "-evaluate", "Multiply", "0.7731", "-define", "png:color-type=2"], source("RGB"))
    magick("rgba16", ["-depth", "16", "-evaluate", "Multiply", "0.7731", "-define", "png:color-type=6"], source("RGBA"))
    magick("rgb-interlaced", ["-interlace", "PNG", "-define", "png:color-type=2"], source("RGB"))
    magick("rgba-interlaced", ["-interlace", "PNG", "-define", "png:color-type=6"], source("RGBA"))
    magick("gray4-interlaced", ["-interlace", "PNG", "-depth", "4", "-define", "png:color-type=0", "-define", "png:bit-depth=4"], source("L"))
    save("gray-key", source("L"), transparency=100)
    save("rgb-key", source("RGB"), transparency=(0, 0, 0))
    for filt in range(5):
        magick(f"filter{filt}", ["-define", f"png:compression-filter={filt}", "-define", "png:color-type=2"], source("RGB"))


main()
