"""Makes the JPEG files of the engine's decoder tests, in `files/jpeg/`.

Each `name.jpg` comes with `name.raw`: the pixels as Pillow, with libjpeg,
decodes them, 8 bits per sample, rows without padding. CMYK files have the
samples as the file has them (Adobe's are inverted), as the engine gives them.

Needs Pillow, and ImageMagick for the samplings Pillow can't write. Run from this folder:

    python jpeg.py
"""

from __future__ import annotations

import io
import random
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "files" / "jpeg"


def picture(width: int, height: int, mode: str) -> Image.Image:
    """Gradients, shapes and noise, so every frequency has something."""
    rng = random.Random(width * 1000 + height)
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            pixels[x, y] = (
                x * 255 // max(width - 1, 1),
                y * 255 // max(height - 1, 1),
                (x + y) * 255 // max(width + height - 2, 1),
            )
    draw = ImageDraw.Draw(image)
    draw.ellipse((width // 5, height // 5, width * 4 // 5, height * 3 // 5), fill=(220, 30, 40))
    if width > 8 and height > 8:
        draw.rectangle((width // 2, height // 2, width - 4, height - 4), outline=(0, 0, 0), width=2)
    draw.line((0, height - 1, width - 1, 0), fill=(255, 255, 0), width=1)
    for _ in range(width * height // 25):
        pixels[rng.randrange(width), rng.randrange(height)] = (
            rng.randrange(256),
            rng.randrange(256),
            rng.randrange(256),
        )
    if mode == "L":
        return image.convert("L")
    if mode == "CMYK":
        return image.convert("CMYK")
    return image


def write(name: str, width: int, height: int, mode: str, **options) -> None:
    buffer = io.BytesIO()
    picture(width, height, mode).save(buffer, "JPEG", **options)
    data = buffer.getvalue()
    decoded = Image.open(io.BytesIO(data))
    decoded.load()
    raw = decoded.tobytes()
    if mode == "CMYK":
        raw = bytes(255 - b for b in raw)
    (OUT / f"{name}.jpg").write_bytes(data)
    (OUT / f"{name}.raw").write_bytes(raw)


def write_magick(name: str, width: int, height: int, sampling: str, quality: int) -> None:
    """A file with a sampling Pillow doesn't write, made by ImageMagick."""
    with tempfile.TemporaryDirectory() as folder:
        source = Path(folder) / "in.png"
        picture(width, height, "RGB").save(source)
        target = OUT / f"{name}.jpg"
        subprocess.run(
            ["magick", str(source), "-sampling-factor", sampling, "-quality", str(quality), str(target)],
            check=True,
        )
    decoded = Image.open(target)
    decoded.load()
    (OUT / f"{name}.raw").write_bytes(decoded.tobytes())


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    write("gray", 67, 45, "L", quality=90)
    write("gray-progressive", 67, 45, "L", quality=85, progressive=True)
    write("rgb-444", 67, 45, "RGB", quality=90, subsampling=0)
    write("rgb-422", 67, 45, "RGB", quality=85, subsampling=1)
    write("rgb-420", 67, 45, "RGB", quality=85, subsampling=2)
    write_magick("rgb-440", 67, 45, "1x2", 85)
    write_magick("rgb-411", 67, 45, "4x1", 85)
    write("rgb-420-big", 203, 121, "RGB", quality=75, subsampling=2)
    write("rgb-444-progressive", 67, 45, "RGB", quality=90, subsampling=0, progressive=True)
    write("rgb-420-progressive", 67, 45, "RGB", quality=80, subsampling=2, progressive=True)
    write("rgb-422-progressive", 99, 70, "RGB", quality=60, subsampling=1, progressive=True)
    write("rgb-restart", 67, 45, "RGB", quality=85, subsampling=2, restart_marker_blocks=3)
    write("rgb-optimized", 67, 45, "RGB", quality=85, optimize=True)
    write("rgb-quality-10", 67, 45, "RGB", quality=10)
    write("rgb-quality-100", 67, 45, "RGB", quality=100, subsampling=0)
    write("rgb-tiny", 1, 1, "RGB", quality=90)
    write("rgb-narrow", 3, 40, "RGB", quality=90)
    write("cmyk", 67, 45, "CMYK", quality=90)
    write("cmyk-progressive", 67, 45, "CMYK", quality=90, progressive=True)


if __name__ == "__main__":
    main()
