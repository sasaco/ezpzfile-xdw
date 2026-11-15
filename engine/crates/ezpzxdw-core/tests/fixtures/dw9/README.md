# DocuWorks 9 samples

These are small documents made with DocuWorks 9 from pictures whose pixels
are known. Next to each document is DocuWorks' own drawing of it (the
answer). `tests/dw9.rs` checks that the engine decodes each picture the way
DocuWorks draws it.

No customer material is in these files. The pictures are made by a script.

Made 2026-11-15 on Windows 11 with DocuWorks 9.0.11 (xdwapi.dll 9.0.1.2).
The pictures come from `pictures.py`: `python pictures.py OUTDIR` writes
them as BMP files.

| file | drawing | how it was made | picture (from pictures.py) |
|---|---|---|---|
| paste-grad-64.xdw | picture annotation, kind 7 (stored DIB, one strip) | grad-64.bmp opened in Paint, Ctrl+A, Ctrl+C, then Ctrl+V on a blank page in DocuWorks Viewer, saved | grad-64 |
| paste-odd-67x66.xdw | same | the same with odd-67x66.bmp | odd-67x66 (rows need padding) |
| api-odd-67x66.xdw | kind 7 | odd-67x66.bmp added with the API (XDW_AddAnnotation, a bitmap annotation from the file) at 10 mm, 10 mm on a blank page | odd-67x66 |
| file-line-300x200.xdw | image page, kind 11 (`.ims`: JPEG background at half size + one grey Group 4 mask) | line-300x200.bmp dropped on a blank page in Viewer (the same bytes as XDW_CreateXdwFromImageFile with compression 8) | line-300x200 |
| mrc-two-300x200.xdw | kind 11 with two masks (grey and RGB) | XDW_CreateXdwFromImageFile, compression 8 (MRC, normal) | two-300x200 |
| mrc-grey-401x303.xdw | kind 11 with a grey JPEG background | the same, from the 8-bit grey BMP | grey-401x303 |
| api-drawing-800x600.xdw | picture annotation, kind 10 (predicted runs and new colours, lossless; DocuWorks uses it for large pictures) | drawing-800x600.bmp (300 dpi) added with the API (XDW_AddAnnotation, a bitmap annotation from the file) at 2.54 mm, 2.54 mm on a blank A4 page, as for api-odd-67x66 | drawing-800x600 |

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

## Notes

- DocuWorks' render leaves the page's first row and column white. So the
  test skips each picture's first row and column.
- A picture pasted through Paint is not always the BMP's pixels exactly.
  paste-odd-67x66 is blended. This is why the answer is DocuWorks' render
  and not the BMP.
- Allowed differences: none for kind 7 and kind 10. For kind 11, up to 1 where the
  background JPEG is grey, and up to 5 (mean under 0.5) where it is a
  colour JPEG. JPEG decoders turn YCbCr into RGB a little differently.
- Rows of kind 7 are stored top-first, although the BITMAPINFO height is
  positive. A picture stored bottom-up shows upside down in DocuWorks 9.
