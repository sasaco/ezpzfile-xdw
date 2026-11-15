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

## Changes from jpeg-decoder 0.3.2

Files under `src/` not listed here are byte-identical to upstream, so they can be diffed against it.

- `src/arch/` (SIMD) and `src/worker/rayon.rs` are left out.
- The crate is always built with the upstream `platform_independent` feature (no SIMD). There is
  no `rayon` or `nightly_aarch64_neon` feature; without `platform_independent` it does not build.
- Doc tests are off (the upstream examples read files that are not vendored).
- IDCT, upsampling and colour conversion: see below (next change).
