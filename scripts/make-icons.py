#!/usr/bin/env python3
"""Builds every icon from the master vector docs/brand/mark.svg.

  python3 scripts/make-icons.py

Needs: pip install cairosvg pillow numpy opencv-python-headless
Writes src-tauri/icons/{32x32.png,128x128.png,128x128@2x.png,icon.png,icon.ico}.

Two looks, chosen by size:
  * 16-32 px: the bare mark on a transparent background, with the gaps between the wedges widened so
    they stay visible (taskbar, title bar, tray);
  * 48 px and up: the mark on a dark rounded tile (Explorer, installer, shortcuts).
"""
import io, pathlib, struct
import cairosvg, cv2, numpy as np
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
SVG = ROOT / "docs" / "brand" / "mark.svg"
OUT = ROOT / "src-tauri" / "icons"
ORANGE = (242, 92, 5)
INK = (22, 17, 13)
BIG = 1024

png = cairosvg.svg2png(url=str(SVG), output_width=BIG, output_height=BIG)
MASK = np.array(Image.open(io.BytesIO(png)).convert("RGBA"))[:, :, 3]


def mark(size, erode=0):
    """The mark as an RGBA image of `size` px. `erode` widens the gaps (in 1024-px units)."""
    m = MASK
    if erode:
        k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * erode + 1, 2 * erode + 1))
        m = cv2.erode(m, k)
    a = Image.fromarray(m).resize((size, size), Image.LANCZOS)
    img = Image.new("RGBA", (size, size), ORANGE + (0,))
    img.putalpha(a)
    return img


def small(size):
    return mark(size, {16: 24, 24: 18, 32: 13}.get(size, 10))


def tile(size, fill=0.76):
    ss = 4
    big = Image.new("RGBA", (size * ss, size * ss), (0, 0, 0, 0))
    ImageDraw.Draw(big).rounded_rectangle((0, 0, size * ss - 1, size * ss - 1), radius=int(size * ss * 0.225), fill=INK + (255,))
    base = big.resize((size, size), Image.LANCZOS)
    m = mark(int(round(size * fill)))
    base.alpha_composite(m, ((size - m.width) // 2, (size - m.height) // 2))
    return base


def ico(images):
    blobs = []
    for im in images:
        b = io.BytesIO()
        im.save(b, "PNG")
        blobs.append(b.getvalue())
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    for im, blob in zip(images, blobs):
        w = im.width if im.width < 256 else 0
        entries += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    return head + entries + b"".join(blobs)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    small(32).save(OUT / "32x32.png")
    tile(128).save(OUT / "128x128.png")
    tile(256).save(OUT / "128x128@2x.png")
    tile(512).save(OUT / "icon.png")
    layers = [small(16), small(24), small(32), tile(48), tile(64), tile(128), tile(256)]
    (OUT / "icon.ico").write_bytes(ico(layers))
    print("icons written to", OUT)
