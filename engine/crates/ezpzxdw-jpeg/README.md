# ezpzxdw-jpeg

A copy of [jpeg-decoder](https://github.com/image-rs/jpeg-decoder) 0.3.2 by The image-rs Developers
("The jpeg-decoder Developers"), taken from crates.io, licensed MIT OR Apache-2.0
(`LICENSE-MIT`, `LICENSE-APACHE`, unchanged from upstream).

## Why

DocuWorks draws JPEG pictures with libjpeg-turbo: the fast integer IDCT (`JDCT_IFAST`),
fancy upsampling of the chroma planes, and table-driven YCbCr to RGB conversion. jpeg-decoder
uses a different IDCT, upsampler and conversion, and has no way to swap its IDCT, so pictures
it decodes differ slightly from what DocuWorks shows. This crate keeps jpeg-decoder's parser and
Huffman decoding and is where those three parts are replaced.

It is used by ezpzxdw-core (`ezpzxdw_core::jpeg::rgba`, and the kind 11 backgrounds in `ims.rs`),
and through it by the raster (headless `xdw_png`) and the wasm screen.

## Changes from jpeg-decoder 0.3.2

Files under `src/` not listed here are byte-identical to upstream, so they can be diffed against it.

- `src/arch/` (SIMD) and `src/worker/rayon.rs` are left out.
- The crate is always built with the upstream `platform_independent` feature (no SIMD). There is
  no `rayon` or `nightly_aarch64_neon` feature; without `platform_independent` it does not build.
- Doc tests are off (the upstream examples read files that are not vendored).
- `src/idct.rs`, the 8×8 IDCT only (`dequantize_and_idct_block_8x8` and its inner function):
  stb_image's IDCT is replaced by libjpeg-turbo 3.0.4's fast integer IDCT for 8-bit samples,
  `jpeg_idct_ifast` (jidctfst.c: CONST_BITS 8, PASS1_BITS 2, no accurate rounding, the zero-AC
  shortcuts for columns and rows), with the dequantisation multipliers of jddctmgr.c
  (`JDCT_IFAST`: `DESCALE(quantval * aanscales, 14 - IFAST_SCALE_BITS)`, kept as a 16-bit
  `short` like libjpeg-turbo's `MULTIPLIER`). The output is clamped to 0..255 where libjpeg
  indexes its range-limit table (equal for every value a valid stream gives; libjpeg-turbo's
  SIMD version saturates too). Arithmetic wraps, so malicious coefficients cannot panic. The
  private stb helpers used only by the 8×8 path are removed. The 4×4, 2×2 and 1×1 scaled IDCTs
  and `choose_idct_size` are unchanged. Of the unit tests, the stb-specific expected values of
  the saturated-input test are replaced by a no-panic check.
- `src/upsampler.rs`: the upsampler is chosen from the sampling factors only, as jdsample.c's
  `jinit_upsampler` (upstream's shortcuts for 1-pixel-wide or 1-pixel-high images are gone).
  2h1v is `h2v1_fancy_upsample` (biases +1 and +2, first and last samples copied) when the
  component is more than 2 samples wide, otherwise `h2v1_upsample` (replication). 2h2v is
  `h2v2_fancy_upsample` (biases +8 and +7, first `(colsum*4+8)>>4`, last `(colsum*4+7)>>4`)
  when the component is more than 2 samples wide, otherwise `h2v2_upsample` (replication in
  both directions). The rows above the first and below the last row are the edge row, as
  libjpeg's context rows. 1h2v (4:4:0) replicates each row, as libjpeg-turbo before 2.0
  (`int_upsample`), not `h1v2_fancy_upsample`: DocuWorks 9 draws 4:4:0 that way (checked
  against DocuWorks 9's own renders on 2026-11-15; fancy upsampling differed by up to 67).
  Other integral ratios replicate as upstream.
- `src/decoder.rs`, only `ycbcr_to_rgb` and the constants and helpers under it: YCbCr to RGB
  is libjpeg-turbo 3.0.4's `build_ycc_rgb_table` (jdcolor.c) and `ycc_rgb_convert`
  (jdcolext.c), computed inline: SCALEBITS 16, `ONE_HALF`, `FIX(x)`, R = y + Cr_r, B = y + Cb_b,
  G = y + ((Cb_g + Cr_g) >> 16), then clamped to 0..255.
- `tests/libjpeg_turbo.rs` and `tests/fixtures/` (added): 104 JPEG files and libjpeg-turbo's
  decode of each (simplejpeg 1.9.0 with `fastdct=True, fastupsample=False`; 4:4:0 with
  `fastupsample=True`), which this crate must reproduce with difference 0. See
  `tests/fixtures/README.md`.

## Independent JPEG Group

This software is based in part on the work of the Independent JPEG Group.

The IDCT, upsampling and colour conversion in `src/idct.rs`, `src/upsampler.rs` and
`src/decoder.rs` listed above are translated to Rust from libjpeg-turbo 3.0.4 (`jidctfst.c`,
`jddctmgr.c`, `jdsample.c`, `jdcolor.c`, `jdcolext.c`) and changed as listed above. The IJG's
README is included unchanged as `README.ijg`. Its copyright and no-warranty notice:

> The authors make NO WARRANTY or representation, either express or implied,
> with respect to this software, its quality, accuracy, merchantability, or
> fitness for a particular purpose.  This software is provided "AS IS", and you,
> its user, assume the entire risk as to its quality and accuracy.
>
> This software is copyright (C) 1991-2020, Thomas G. Lane, Guido Vollbeding.
> All Rights Reserved except as specified below.
>
> Permission is hereby granted to use, copy, modify, and distribute this
> software (or portions thereof) for any purpose, without fee, subject to these
> conditions:
> (1) If any part of the source code for this software is distributed, then this
> README file must be included, with this copyright and no-warranty notice
> unaltered; and any additions, deletions, or changes to the original files
> must be clearly indicated in accompanying documentation.
> (2) If only executable code is distributed, then the accompanying
> documentation must state that "this software is based in part on the work of
> the Independent JPEG Group".
> (3) Permission for use of this software is granted only if the user accepts
> full responsibility for any undesirable consequences; the authors accept
> NO LIABILITY for damages of any kind.
>
> These conditions apply to any software derived from or based on the IJG code,
> not just to the unmodified library.  If you use our work, you ought to
> acknowledge us.
>
> Permission is NOT granted for the use of any IJG author's name or company name
> in advertising or publicity relating to this software or products derived from
> it.  This software may be referred to only as "the Independent JPEG Group's
> software".
>
> We specifically permit and encourage the use of this software as the basis of
> commercial products, provided that all warranty or liability claims are
> assumed by the product vendor.
