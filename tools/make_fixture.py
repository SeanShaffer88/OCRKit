#!/usr/bin/env python3
"""Regenerate tests/fixtures/hello-ocr.png — the e2e OCR fixture.

Renders one line of plain text, black on white, with PIL and the
DejaVu Sans TTF, at a size that behaves like a ~200 DPI scan line.
The PNG is committed (it is our own generated test data); this script
exists so the fixture is reproducible and its provenance is explicit.

    python3 tools/make_fixture.py

Requires: Pillow (pip install pillow) and a DejaVu Sans TTF. On
Debian/Ubuntu the font ships in fonts-dejavu-core and lives at
/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf. On other systems,
edit FONT_CANDIDATES or set OCRKIT_FONT to a TTF path.
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont

TEXT = "The quick brown fox jumps over the lazy dog 12345"
FONT_SIZE = 48
PADDING = 30

FONT_CANDIDATES = [
    os.environ.get("OCRKIT_FONT", ""),
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
    "/Library/Fonts/Arial.ttf",
]


def main() -> int:
    font_path = next((p for p in FONT_CANDIDATES if p and os.path.isfile(p)), None)
    if font_path is None:
        print("no TTF font found; set OCRKIT_FONT to one", file=sys.stderr)
        return 1
    font = ImageFont.truetype(font_path, FONT_SIZE)
    probe = ImageDraw.Draw(Image.new("RGB", (1, 1)))
    x0, y0, x1, y1 = probe.textbbox((0, 0), TEXT, font=font)
    w = (x1 - x0) + 2 * PADDING
    h = (y1 - y0) + 2 * PADDING
    img = Image.new("RGB", (w, h), "white")
    draw = ImageDraw.Draw(img)
    draw.text((PADDING - x0, PADDING - y0), TEXT, fill="black", font=font)
    out = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "tests",
        "fixtures",
        "hello-ocr.png",
    )
    img.save(out)
    print(f"wrote {out} ({w}x{h}) from {font_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
