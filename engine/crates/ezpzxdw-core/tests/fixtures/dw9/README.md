# DocuWorks 9 samples

These are small documents made with DocuWorks 9 from pictures whose pixels
are known. Next to each document is DocuWorks' own drawing of it (the
answer). `tests/dw9.rs` checks that the engine decodes each picture the way
DocuWorks draws it.

No customer material is in these files. The pictures are made by a script.

Made 2026-11-15 on Windows 11 with DocuWorks 9.0.11 (xdwapi.dll 9.0.1.2).
The pictures come from `pictures.py`: `python pictures.py OUTDIR` writes
them as BMP files. `pictures.py` needs only Python. `jpeg440.py` turns
photo-300x200.bmp into photo-300x200-440.jpg, a JPEG with 4:4:0
subsampling; it needs simplejpeg 1.9.0, numpy and Pillow
(`uv run --no-project --with simplejpeg==1.9.0 --with numpy --with pillow python -I jpeg440.py OUTDIR/photo-300x200.bmp`).

| file | drawing | how it was made | picture (from pictures.py) |
|---|---|---|---|
| paste-grad-64.xdw | picture annotation, kind 7 (stored DIB, one strip) | grad-64.bmp opened in Paint, Ctrl+A, Ctrl+C, then Ctrl+V on a blank page in DocuWorks Viewer, saved | grad-64 |
| paste-odd-67x66.xdw | same | the same with odd-67x66.bmp | odd-67x66 (rows need padding) |
| api-odd-67x66.xdw | kind 7 | odd-67x66.bmp added with the API (XDW_AddAnnotation, a bitmap annotation from the file) at 10 mm, 10 mm on a blank page | odd-67x66 |
| file-line-300x200.xdw | image page, kind 11 (`.ims`: JPEG background at half size + one grey Group 4 mask) | line-300x200.bmp dropped on a blank page in Viewer (the same bytes as XDW_CreateXdwFromImageFile with compression 8) | line-300x200 |
| mrc-two-300x200.xdw | kind 11 with two masks (grey and RGB) | XDW_CreateXdwFromImageFile, compression 8 (MRC, normal) | two-300x200 |
| mrc-grey-401x303.xdw | kind 11 with a grey JPEG background | the same, from the 8-bit grey BMP | grey-401x303 |
| api-drawing-800x600.xdw | picture annotation, kind 10 (predicted runs and new colours, lossless; DocuWorks uses it for large pictures) | drawing-800x600.bmp (300 dpi) added with the API (XDW_AddAnnotation, a bitmap annotation from the file) at 2.54 mm, 2.54 mm on a blank A4 page, as for api-odd-67x66 | drawing-800x600 |
| jpeg-photo-300x200.xdw | JPEG page, kind 5 (JPEG 4:2:0 made by DocuWorks) | photo-300x200.bmp made into a document with the API (XDW_CreateXdwFromImageFile, `dwapi fromimg`, compression 2) | photo-300x200 |
| jpeg440-photo-300x200.xdw | JPEG page, kind 5 (JPEG 4:4:0, kept as is: the page's data is the .jpg byte for byte) | photo-300x200-440.jpg (from jpeg440.py) made into a document the same way, compression 0 | photo-300x200 |

## The answers

`NAME.png` is DocuWorks' render of page 1, cropped to the picture. The
render is made with XDW_ConvertPageToImageFile at 96 dpi, the pictures' own
resolution. The crop's position and size are
round(1/100 mm × 96 / 2540). They were made with `tools/dwapi`
(`dwapi render`, `addimg`, `fromimg`).

api-drawing-800x600.png is rendered at 300 dpi, the picture's own
resolution, and cropped at round(1/100 mm × 300 / 2540). Kind 10 is
lossless: this render is the BMP's pixels exactly, the first row and column
included (the picture starts 30 px into the page).

jpeg-photo-300x200.png is rendered at 96 dpi and jpeg440-photo-300x200.png
at 200 dpi: DocuWorks makes a page from a JPEG file at 200 dpi, so 200 dpi is
that picture's own resolution. Each is cropped to the page's body at
round(1/100 mm × dpi / 2540), which is 300 x 200 px, the JPEG's size.

## Notes

- DocuWorks' render leaves the page's first row and column white. So the
  test skips each picture's first row and column.
- A picture pasted through Paint is not always the BMP's pixels exactly.
  paste-odd-67x66 is blended. This is why the answer is DocuWorks' render
  and not the BMP.
- Allowed differences: none, for every kind. The engine decodes JPEGs as
  libjpeg-turbo's fast integer IDCT with fancy upsampling (4:4:0 is
  replicated, not smoothed), which is how DocuWorks 9 draws them (see
  `engine/crates/ezpzxdw-jpeg/README.md`). This holds for the JPEG
  backgrounds of kind 11 and for JPEG pages (kind 5).
- Rows of kind 7 are stored top-first, although the BITMAPINFO height is
  positive. A picture stored bottom-up shows upside down in DocuWorks 9.
