# libjpeg-turbo answers

`tests/libjpeg_turbo.rs` decodes every `NAME.jpg` here and checks that the
result is exactly `NAME.png` (difference 0 in every sample).

No outside image is in these files. `make.py` draws the pictures (colour
gradients, hard edges in saturated colours, and a little deterministic noise
hashed from the position), encodes them and writes the answers:

    uv run --no-project --with simplejpeg==1.9.0 --with numpy --with pillow python -I make.py

Made 2026-11-15 on Windows 11 with simplejpeg 1.9.0 (its bundled
libjpeg-turbo reports version 3.1.2), Pillow 13.0.0 (libjpeg-turbo 3.2.0),
numpy and Python 3.13.

## The cases (104)

| name | written by | what |
|---|---|---|
| `s444_*`, `s422_*`, `s420_*`, `s440_*`, `s411_*` | simplejpeg `encode_jpeg(..., colorsubsampling=...)` | baseline, sizes 1×1, 1×9, 2×2, 2×9, 3×5, 5×3, 4×4, 6×7, 9×1, 9×2, 17×13, 33×31 (quality 30, 75, 95 or 100 in turn) and 41×19 at quality 5 and 95 |
| `s420_q50_257x131` | simplejpeg | many MCUs |
| `gray_*` | simplejpeg, `Gray` | one component, the same sizes and 41×19 |
| `prog_*` | Pillow, `progressive=True` | 4:4:4 and 4:2:2 at 17×13, 4:2:0 at 33×31 and 49×37, grey 41×23 |
| `rst_*` | Pillow, `restart_marker_blocks=1` | the same, a restart marker after every MCU |
| `rstrows_*` | Pillow, `restart_marker_rows=1` | the same, a restart marker after every MCU row |
| `progrst_*` | Pillow, `progressive=True, restart_marker_rows=1` | progressive with restart markers |

Names give the quality and width×height (`s420_q75_17x13`).

## The answers

`NAME.png` is libjpeg-turbo's decode with the fast integer IDCT and fancy
upsampling, which is how DocuWorks 9 draws JPEG pictures:

    simplejpeg.decode_jpeg(jpg, colorspace="RGB", fastdct=True, fastupsample=False)

(`colorspace="GRAY"` for one-component files, saved as an 8-bit grey PNG).

4:4:0 files (`s440_*`) are the exception: their answer is

    simplejpeg.decode_jpeg(jpg, colorspace="RGB", fastdct=True, fastupsample=True)

which replicates each chroma row (in a 4:4:0 file the chroma planes are the
only upsampled components, and they are 1h2v). libjpeg-turbo 2.0 and later
upsample 1h2v with `h1v2_fancy_upsample`; earlier versions replicate. DocuWorks
9 draws 4:4:0 like libjpeg-turbo before 2.0: checked on 2026-11-15 against
DocuWorks 9's own renders of JPEG pages (`dwapi fromimg`, the JPEG stored as
is), where the fast IDCT with replication gave difference 0 and with fancy
upsampling up to 67. 4:4:4, 4:2:2, 4:2:0, 4:1:1, grey and progressive 4:2:0
and 4:4:4 matched the fast IDCT with fancy upsampling with difference 0.
