"""The pictures behind the DocuWorks 9 samples in this folder (see README.md).

python pictures.py OUTDIR  ->  24-bit (and one 8-bit grey) BMPs whose pixels are known exactly.
"""
import os
import struct
import sys


def bmp(path, w, h, px, grey=False):
    """px(x, y) -> (r, g, b), y = 0 at the top; rows stored bottom-up."""
    bpp = 8 if grey else 24
    stride = (w * bpp // 8 + 3) & ~3
    rows = []
    for y in range(h - 1, -1, -1):
        row = bytearray()
        for x in range(w):
            r, g, b = px(x, y)
            row += bytes(((r * 19595 + g * 38470 + b * 7471 + 0x8000) >> 16,)) if grey else bytes((b, g, r))
        rows.append(bytes(row) + b"\0" * (stride - len(row)))
    bits = b"".join(rows)
    pal = b"".join(bytes((i, i, i, 0)) for i in range(256)) if grey else b""
    info = struct.pack("<IiiHHIIiiII", 40, w, h, 1, bpp, 0, len(bits), 3780, 3780, 256 if grey else 0, 0)
    off = 14 + len(info) + len(pal)
    with open(path, "wb") as f:
        f.write(struct.pack("<2sIHHI", b"BM", off + len(bits), 0, 0, off) + info + pal + bits)


def odd(x, y):
    # three bands of 22 rows (white, black, light blue) in the even columns, yellow odd columns
    return [(255, 255, 255), (0, 0, 0), (0, 128, 255)][y // 22] if x % 2 == 0 else (255, 255, 0)


def line(x, y):
    # white paper, a black frame and grid, a red diagonal, a blue box, a green dashed line
    if x in (0, 299) or y in (0, 199) or x % 50 == 0 or y % 40 == 0:
        return (0, 0, 0)
    if abs(x - y * 3 // 2) <= 1:
        return (255, 0, 0)
    if 200 <= x <= 260 and 120 <= y <= 170:
        return (0, 0, 255) if (x in (200, 260) or y in (120, 170)) else (200, 220, 255)
    if y == 100 and (x // 6) % 2 == 0:
        return (0, 160, 0)
    return (255, 255, 255)


def glyph(x, y):
    # letter-like strokes in 10 x 16 px cells
    cy, cx = y % 16, x % 10
    if cy >= 12 or cx >= 8:
        return False
    k = ((x // 10) * 7 + (y // 16) * 3) % 5
    return (cx in (0, 1) and k < 3) or (cy in (0, 1) and k != 2) or (cy in (5, 6) and k in (1, 3)) or (cx in (6, 7) and k > 1) or (cy in (10, 11) and k != 4)


def two(x, y):
    # the strokes black on the left half, red on the right
    if not glyph(x, y):
        return (255, 255, 255)
    return (0, 0, 0) if x < 150 else (220, 0, 0)


def mixed(x, y):
    # a smooth box, a black grid, a red line, a green box
    if 40 <= x < 200 and 30 <= y < 150:
        return (x, y, (x + y) % 256)
    if x % 40 == 0 or y % 30 == 0:
        return (0, 0, 0)
    if abs(y - 2 * x + 100) <= 1:
        return (255, 0, 0)
    if 250 <= x <= 330 and 160 <= y <= 220:
        return (0, 128, 0)
    return (255, 255, 255)


PICTURES = {
    "grad-64.bmp": (64, 64, lambda x, y: (x * 4, y * 4, 128), False),
    "odd-67x66.bmp": (67, 66, odd, False),
    "line-300x200.bmp": (300, 200, line, False),
    "two-300x200.bmp": (300, 200, two, False),
    "grey-401x303.bmp": (401, 303, mixed, True),
}

if __name__ == "__main__":
    os.makedirs(sys.argv[1], exist_ok=True)
    for name, (w, h, px, grey) in PICTURES.items():
        bmp(os.path.join(sys.argv[1], name), w, h, px, grey)
