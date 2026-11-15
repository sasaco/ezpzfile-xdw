# dwapi: DocuWorks itself as a referee

`dwapi.c` is a small Windows program that calls the DocuWorks API
(`xdwapi.dll`, installed with DocuWorks Desk) under Wine. It lets the real
DocuWorks engine open our saved files, list pages and annotations with their
settings, and draw pages.

On Windows with DocuWorks installed (DocuWorks 9.0.11, `xdwapi.dll` 9.0.1.2
in both System32 and SysWOW64, checked 2026-11-15), build it with Visual
Studio's compiler, 64-bit or 32-bit (both load the matching `xdwapi.dll`),
and run it directly:

    "%ProgramFiles%\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
    cl /nologo /O1 /D_CRT_SECURE_NO_WARNINGS dwapi.c
    dwapi.exe render C:\path\file.xdw 1 C:\path\p1.bmp 96 > out.txt

Redirect the output to a file there too (see the note below).

Under Wine:

    i686-w64-mingw32-gcc -O1 -o dwapi.exe dwapi.c
    wine dwapi.exe info   'Z:\path\file.xdw'          # pages, annotations, settings (Shift_JIS)
    wine dwapi.exe render 'Z:\path\file.xdw' 1 'Z:\path\p1.bmp' 100
    wine dwapi.exe make   'Z:\path\copy.xdw'          # DocuWorks adds a date stamp, sticky note, text
    wine dwapi.exe addimg 'Z:\path\copy.xdw' 'Z:\path\pic.bmp' X Y   # DocuWorks adds a picture annotation (BMP only)
    wine dwapi.exe fromimg 'Z:\path\pic.bmp' 'Z:\path\new.xdw' 8   # DocuWorks makes a document from an image file
    wine dwapi.exe move   'Z:\path\copy.xdw' PAGE N X Y [W H]          # DocuWorks moves (and sizes) annotation N, then saves
    python3 check.py out_*.xdw                        # ours vs DocuWorks: pages, annotations, drawing

`fromimg` takes the API's compression: with DocuWorks 9, 0, 2 and 3 give a
JPEG page (drawing kind 5), 1 a lossless one (kind 7, a stored DIB), and 8, 9
and 10 the high-compression ones (kind 11, MRC: normal, high quality, high
compression); 8 gives the same bytes as dropping the file on Viewer. Other
values are refused (E_INVALIDARG).

`make` is how the attribute names DocuWorks 10 writes for date stamps and
sticky notes were found (see `docs/spec/XDW-FORMAT.md`).

Setup used: the latest DocuWorks (10.1.1) in a 64-bit
Wine 9 prefix with Japanese locale; Visual C++ runtimes from the installer, and
`msvcp140`, `vcruntime140` set to native. DocuWorks is not part of this
project; you download it from FUJIFILM and accept its licence yourself.
Note: `xdwapil.exe` stays running after a call, so capture output through a
file, not a pipe (as `check.py` does).
