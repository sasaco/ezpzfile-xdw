"""The 4:4:0 JPEG behind jpeg440-photo-300x200.xdw (see README.md).

python jpeg440.py DIR/photo-300x200.bmp  ->  DIR/photo-300x200-440.jpg

photo-300x200.bmp comes from pictures.py. The JPEG is made by libjpeg-turbo
(through simplejpeg) at quality 90 with 4:4:0 subsampling (colour halved
vertically only), which DocuWorks' own JPEG pages do not use.

Needs simplejpeg 1.9.0, numpy and Pillow, for example:
uv run --no-project --with simplejpeg==1.9.0 --with numpy --with pillow python -I jpeg440.py DIR/photo-300x200.bmp
"""
import os
import sys

import numpy
import simplejpeg
from PIL import Image

if __name__ == "__main__":
    src = sys.argv[1]
    rgb = numpy.ascontiguousarray(numpy.asarray(Image.open(src).convert("RGB")))
    out = os.path.join(os.path.dirname(src), "photo-300x200-440.jpg")
    with open(out, "wb") as f:
        f.write(simplejpeg.encode_jpeg(rgb, 90, "RGB", "440"))
