//! JPEG pictures decoded as DocuWorks draws them.
//!
//! DocuWorks decodes JPEGs (picture pages, BI_JPEG bitmaps, JPEGs in EMF)
//! with libjpeg-turbo's fast integer IDCT, which differs from a browser's or
//! another library's decoder by a few levels per channel. The `ezpzxdw-jpeg`
//! crate decodes the same way; this module turns its output into the RGBA
//! the painters use, so the headless raster and the screen match DocuWorks.

use crate::limits;
use ezpzxdw_jpeg::{Decoder, PixelFormat};

/// A JPEG as `(width, height, opaque RGBA)`, top row first; `None` when it
/// cannot be read or is larger than the limits allow (`maxImagePixels`, and
/// inside a scope its 4 bytes per pixel count against `maxBytes`).
pub fn rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h) = crate::pages::jpeg_size(bytes)?;
    if !limits::image(w as usize, h as usize) {
        return None;
    }
    let mut dec = Decoder::new(bytes);
    let px = dec.decode().ok()?;
    let info = dec.info()?;
    if (info.width as u32, info.height as u32) != (w, h) {
        return None;
    }
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    match info.pixel_format {
        PixelFormat::L8 => px.iter().for_each(|&g| out.extend_from_slice(&[g, g, g, 255])),
        // lossless JPEGs over 8 bits come as native-endian u16 (ezpzxdw-jpeg's
        // decoder/lossless.rs `convert_to_u8`): keep the high byte
        PixelFormat::L16 => px.chunks_exact(2).for_each(|g| {
            let v = (u16::from_ne_bytes([g[0], g[1]]) >> 8) as u8;
            out.extend_from_slice(&[v, v, v, 255]);
        }),
        PixelFormat::RGB24 => px.chunks_exact(3).for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255])),
        PixelFormat::CMYK32 => px.chunks_exact(4).for_each(|p| {
            // Adobe JPEGs store CMYK inverted
            let k = p[3] as u16;
            let ch = |c: u8| ((c as u16 * k) / 255) as u8;
            out.extend_from_slice(&[ch(p[0]), ch(p[1]), ch(p[2]), 255]);
        }),
    }
    (out.len() == w as usize * h as usize * 4).then_some((w, h, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &[u8] = include_bytes!("../tests/data/tiny.jpg");

    #[test]
    fn decodes_opaque_rgba() {
        let (w, h, px) = rgba(TINY).unwrap();
        assert_eq!((w, h), (24, 16));
        assert_eq!(px.len(), 24 * 16 * 4);
        assert!(px.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn refuses_garbage_and_over_limit() {
        assert!(rgba(b"not a jpeg").is_none());
        let l = limits::Limits { max_image_pixels: 100, ..Default::default() };
        let r = limits::scope(&l, || Ok(rgba(TINY)));
        assert!(matches!(r, Err(crate::error::Error::TooLarge(_))));
    }
}
