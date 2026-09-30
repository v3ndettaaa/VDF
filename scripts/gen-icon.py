#!/usr/bin/env python3
"""Generates src-tauri/icons/source.png (1024x1024) — the VDF app icon source.

Pure-stdlib PNG writer (no Pillow dependency). Simple geometric mark: a dark
rounded square, an "open document" triangle fold, and a bold V.

Run: python3 scripts/gen-icon.py
"""

import struct
import zlib
from pathlib import Path

SIZE = 1024


def png_chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def write_png(path: Path, width: int, height: int, rows: list[bytes]) -> None:
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)  # 8-bit RGBA
    raw = b"".join(b"\x00" + row for row in rows)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(raw, 9))
        + png_chunk(b"IEND", b"")
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png)


def rounded_rect_mask(x, y, size, radius):
    if x < 0 or y < 0 or x >= size or y >= size:
        return False
    # distance to the rounded-rect interior
    cx = min(max(x, radius), size - 1 - radius)
    cy = min(max(y, radius), size - 1 - radius)
    dx, dy = x - cx, y - cy
    if dx == 0 and dy == 0:
        return True
    return dx * dx + dy * dy <= radius * radius


def inside_triangle_fold(x, y):
    # top-right page fold: triangle with legs of 280 px
    leg = 280
    x0, y0 = SIZE - leg, 0
    return x >= x0 and y <= leg and (x - x0) + (leg - y) <= leg


def main() -> None:
    bg = (16, 18, 24, 255)
    fold = (58, 64, 78, 255)
    accent = (77, 141, 255, 255)
    radius = 180

    rows: list[bytes] = []
    for y in range(SIZE):
        row = bytearray()
        for x in range(SIZE):
            if not rounded_rect_mask(x, y, SIZE, radius):
                row += bytes((0, 0, 0, 0))
                continue
            color = bg
            if inside_triangle_fold(x, y):
                color = fold
            # bold "V": two thick diagonal strokes meeting at bottom center
            # left stroke: from (300,260) to (512,760); right: mirror
            t = (y - 260) / 500 if y >= 260 and y <= 760 else -1
            if 0 <= t <= 1:
                cx = 512
                lx = 300 + t * 212
                rx = 724 - t * 212
                w = 46 + 38 * (1 - abs(t - 0.55) * 1.2)  # slight taper
                if abs(x - lx) <= w or abs(x - rx) <= w:
                    color = accent
            row += bytes(color)
        rows.append(bytes(row))

    out = Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "source.png"
    write_png(out, SIZE, SIZE, rows)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
