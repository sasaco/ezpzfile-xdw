"""Write the JPEG fixtures and their answers for tests/libjpeg_turbo.rs.

Run from anywhere (it writes next to itself):

    uv run --no-project --with simplejpeg==1.9.0 --with numpy --with pillow python -I make.py

Each case is NAME.jpg and NAME.png. The pictures are drawn here (colour gradients, hard colour
edges and a little deterministic noise), so no outside image is used. The answer is simplejpeg's
decode with libjpeg-turbo's fast integer IDCT and fancy upsampling:

    simplejpeg.decode_jpeg(jpg, colorspace="RGB" or "GRAY", fastdct=True, fastupsample=False)

except for 4:4:0 files, whose answer uses fastupsample=True (replication), because DocuWorks 9
draws 4:4:0 like libjpeg-turbo before 2.0 (see README.md).
"""

import io
import pathlib

import numpy as np
import simplejpeg
from PIL import Image

OUT = pathlib.Path(__file__).resolve().parent


def picture(w, h, seed, noise_amp=24):
    """An RGB picture: gradients, hard edges with saturated colours, and hashed noise."""
    y, x = np.mgrid[0:h, 0:w].astype(np.int64)
    r = (x * 255) // max(w - 1, 1)
    g = (y * 255) // max(h - 1, 1)
    b = ((x + y) * 255) // max(w + h - 2, 1)
    img = np.stack([r, g, 255 - b], axis=-1)
    # Hard edges: a saturated block and a diagonal stripe.
    block = (x >= w // 3) & (x < (2 * w) // 3 + 1) & (y >= h // 4) & (y < (3 * h) // 4 + 1)
    img[block] = [(255, 0, 0), (0, 0, 255), (0, 255, 0), (255, 255, 0)][seed % 4]
    stripe = ((x - y + seed) % 11) < 2
    img[stripe] = [(0, 0, 0), (255, 255, 255), (255, 0, 255)][seed % 3]
    # Deterministic noise of +-noise_amp, hashed from the position.
    c = np.arange(3, dtype=np.int64)
    hsh = (x[..., None] * 73856093) ^ (y[..., None] * 19349663) ^ ((c + seed) * 83492791)
    hsh = (hsh * 2654435761) & 0xFFFFFFFF
    noise = (hsh >> 13) % (2 * noise_amp + 1) - noise_amp
    img = np.clip(img + noise, 0, 255).astype(np.uint8)
    return np.ascontiguousarray(img)


def answer(jpg, gray, fastupsample=False):
    cs = "GRAY" if gray else "RGB"
    arr = simplejpeg.decode_jpeg(jpg, colorspace=cs, fastdct=True, fastupsample=fastupsample)
    return arr[..., 0] if gray else arr


def write(name, jpg, gray, fastupsample=False):
    (OUT / f"{name}.jpg").write_bytes(jpg)
    arr = answer(jpg, gray, fastupsample)
    Image.fromarray(arr, "L" if gray else "RGB").save(OUT / f"{name}.png", optimize=True)


def main():
    for f in list(OUT.glob("*.jpg")) + list(OUT.glob("*.png")):
        f.unlink()

    small = [(1, 1), (1, 9), (2, 2), (2, 9), (3, 5), (5, 3), (4, 4), (6, 7),
             (9, 1), (9, 2), (17, 13), (33, 31)]
    qualities = [30, 75, 95, 100]
    seed = 0
    for sub in ["444", "422", "420", "440", "411"]:
        for i, (w, h) in enumerate(small):
            q = qualities[(i + seed) % len(qualities)]
            seed += 1
            jpg = simplejpeg.encode_jpeg(picture(w, h, seed), quality=q, colorspace="RGB",
                                         colorsubsampling=sub)
            write(f"s{sub}_q{q}_{w}x{h}", jpg, False, fastupsample=(sub == "440"))
        # Several MCUs, two qualities (one with large quantisation steps).
        for q in [5, 95]:
            seed += 1
            jpg = simplejpeg.encode_jpeg(picture(41, 19, seed, 8), quality=q, colorspace="RGB",
                                         colorsubsampling=sub)
            write(f"s{sub}_q{q}_41x19", jpg, False, fastupsample=(sub == "440"))

    # One larger picture with many MCUs.
    seed += 1
    jpg = simplejpeg.encode_jpeg(picture(257, 131, seed, 0), quality=50, colorspace="RGB",
                                 colorsubsampling="420")
    write("s420_q50_257x131", jpg, False)

    for i, (w, h) in enumerate(small + [(41, 19)]):
        q = qualities[i % len(qualities)]
        seed += 1
        grey = np.ascontiguousarray(picture(w, h, seed, 8)[..., 1:2])
        jpg = simplejpeg.encode_jpeg(grey, quality=q, colorspace="GRAY", colorsubsampling="Gray")
        write(f"gray_q{q}_{w}x{h}", jpg, True)

    # Progressive and restart markers, written by Pillow (libjpeg-turbo).
    pillow_cases = [
        ("prog", dict(progressive=True)),
        ("rst", dict(restart_marker_blocks=1)),
        ("rstrows", dict(restart_marker_rows=1)),
        ("progrst", dict(progressive=True, restart_marker_rows=1)),
    ]
    for prefix, opts in pillow_cases:
        for sub, sub_name, sizes in [(0, "444", [(17, 13, 90)]), (1, "422", [(17, 13, 90)]),
                                     (2, "420", [(33, 31, 90), (49, 37, 60)])]:
            for (w, h, q) in sizes:
                seed += 1
                buf = io.BytesIO()
                Image.fromarray(picture(w, h, seed, 8), "RGB").save(
                    buf, "JPEG", quality=q, subsampling=sub, **opts)
                write(f"{prefix}_s{sub_name}_q{q}_{w}x{h}", buf.getvalue(), False)
        seed += 1
        buf = io.BytesIO()
        Image.fromarray(picture(41, 23, seed, 8)[..., 0], "L").save(buf, "JPEG", quality=80, **opts)
        write(f"{prefix}_gray_q80_41x23", buf.getvalue(), True)

    jpgs = sorted(OUT.glob("*.jpg"))
    total = sum(f.stat().st_size for f in jpgs) + sum(f.stat().st_size for f in OUT.glob("*.png"))
    print(f"{len(jpgs)} cases, {total} bytes")


if __name__ == "__main__":
    main()
