#!/usr/bin/env python3
"""Draws the DupeLens logo into the Android launcher icon files.

Writes desktop/icons/android/ (the layout `cargo tauri icon` would produce), which the Android
CI job copies into the generated Android project. Re-run it only if the logo changes:

    python scripts/make_android_icons.py

The shapes mirror docs/design/logo/dupelens-app-icon.svg (64x64 grid). Needs Pillow.
"""
from pathlib import Path
from PIL import Image, ImageChops, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "desktop" / "icons" / "android"
INK, AMBER, PAPER = "#151515", "#F2A33A", "#EDEBE6"
SS = 8  # draw 8x larger, then shrink, for smooth edges


def mark(size: int, width_fraction: float) -> Image.Image:
    """The two-ring mark on a transparent square of `size` px; the mark spans
    `width_fraction` of the width (its true extent is 58 of the 64 grid units)."""
    big = size * SS
    unit = big * width_fraction / 58.0  # pixels per grid unit
    cx = cy = big / 2
    # mark centre in grid units: x 3..61, y 4..60 -> centre (32, 32)
    def box(x, y, w, h):
        return [cx + (x - 32) * unit, cy + (y - 32) * unit,
                cx + (x + w - 32) * unit, cy + (y + h - 32) * unit]
    r, stroke = 16 * unit, max(1, round(6 * unit))
    back, front = box(6, 7, 40, 32), box(18, 25, 40, 32)

    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    # amber overlap: the lower ring's fill, clipped to the upper ring's shape
    amber = Image.new("L", (big, big), 0)
    ImageDraw.Draw(amber).rounded_rectangle(front, r, fill=255)
    clip = Image.new("L", (big, big), 0)
    ImageDraw.Draw(clip).rounded_rectangle(back, r, fill=255)
    img.paste(Image.new("RGBA", (big, big), AMBER), mask=ImageChops.multiply(amber, clip))
    d = ImageDraw.Draw(img)
    # SVG strokes are centred on the edge; Pillow draws them inside, so grow the box by half.
    half = stroke / 2
    for b in (back, front):
        d.rounded_rectangle([b[0] - half, b[1] - half, b[2] + half, b[3] + half],
                            r + half, outline=PAPER, width=stroke)
    return img.resize((size, size), Image.LANCZOS)


def legacy_square(size: int) -> Image.Image:
    """The desktop look: rounded dark tile with the mark (mark .78 of the tile, as in the SVG)."""
    big = size * SS
    tile = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(tile).rounded_rectangle([0, 0, big - 1, big - 1], big * 14 // 64, fill=INK)
    tile = tile.resize((size, size), Image.LANCZOS)
    tile.alpha_composite(mark(size, 0.78 * 58 / 64))
    return tile


def legacy_round(size: int) -> Image.Image:
    big = size * SS
    disc = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(disc).ellipse([0, 0, big - 1, big - 1], fill=INK)
    disc = disc.resize((size, size), Image.LANCZOS)
    disc.alpha_composite(mark(size, 0.56))
    return disc


def foreground(size: int) -> Image.Image:
    """Adaptive-icon foreground: 108dp canvas, the system masks to ~66dp in the middle, so the
    mark spans about 42% of the canvas (diagonal stays well inside the safe circle)."""
    return mark(size, 0.42)


def main() -> None:
    legacy = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
    for density, px in legacy.items():
        d = OUT / f"mipmap-{density}"
        d.mkdir(parents=True, exist_ok=True)
        legacy_square(px).save(d / "ic_launcher.png")
        legacy_round(px).save(d / "ic_launcher_round.png")
        foreground(px * 108 // 48).save(d / "ic_launcher_foreground.png")

    v26 = OUT / "mipmap-anydpi-v26"
    v26.mkdir(parents=True, exist_ok=True)
    adaptive = ('<?xml version="1.0" encoding="utf-8"?>\n'
                '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
                '  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>\n'
                '  <background android:drawable="@color/ic_launcher_background"/>\n'
                '</adaptive-icon>\n')
    (v26 / "ic_launcher.xml").write_text(adaptive, encoding="utf-8")
    (v26 / "ic_launcher_round.xml").write_text(adaptive, encoding="utf-8")

    values = OUT / "values"
    values.mkdir(parents=True, exist_ok=True)
    (values / "ic_launcher_background.xml").write_text(
        '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
        f'  <color name="ic_launcher_background">{INK}</color>\n</resources>\n', encoding="utf-8")
    print("wrote", OUT)


if __name__ == "__main__":
    main()
