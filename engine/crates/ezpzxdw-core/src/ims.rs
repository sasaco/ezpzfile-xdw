//! Image pages with high compression: DocuWorks stores an image file made
//! into a page at an MRC ("high compression") setting as drawing kind 11, a
//! JPEG background plus one-colour CCITT Group 4 (T.6) masks.
//!
//! ```text
//! header (0x1c bytes, little-endian u32 after the tag)
//!   ".ims"  0x0001_0000  width  height  dpi_x  dpi_y  1
//!   (the last value is 1 in every sample; anything else is refused)
//! records, until a u32 0xdeadfeed (then one more u32, a count, ignored)
//!   17 u32: kind x y w h stored_w stored_h 0 coding colour
//!           255 255|128 255|128 v0 v1 v2 length
//!   `length` bytes of data, then zero padding to a multiple of 4
//!
//! kind 1, coding 0x20: background JPEG of stored_w × stored_h
//!   (colour 0x27 = YCbCr colour JPEG, 0x12 = grey JPEG), stretched over
//!   (x, y, w, h) with nearest sampling: destination (x+dx, y+dy) takes JPEG
//!   pixel (dx*stored_w/w, dy*stored_h/h), integer division. Typically half
//!   size (a 300 × 200 page, a 150 × 100 JPEG).
//! kind 3, coding 0x11: Group 4 mask of w × h (stored_w × stored_h the same)
//!   at (x, y); its black pixels are painted in one colour:
//!   colour 0x12 = grey (v0, v0, v0), 0x21 = RGB (v0, v1, v2).
//! Records are painted in file order over the background; pixels no record
//! covers are white.
//! ```
//!
//! Checked against DocuWorks 9 (xdwapi 9.0.1.2, 2026-11-15): image files made
//! into documents with XDW_CreateXdwFromImageFile at the MRC compressions 8, 9
//! and 10 — the same bytes as dropping the file on Viewer — compared with the
//! API's own render.

use crate::gfx::{Image, ImageData};
use crate::limits;

const HEADER: usize = 0x1c;
const RECORD: usize = 17 * 4;
const END: u32 = 0xdead_feed;

fn u32_at(d: &[u8], i: usize) -> Option<u32> {
    let b = d.get(i..i.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The output being painted: `w` × `h` opaque RGBA.
struct Canvas {
    w: u64,
    h: u64,
    px: Vec<u8>,
}

impl Canvas {
    fn set(&mut self, x: u64, y: u64, rgb: [u8; 3]) {
        if x >= self.w || y >= self.h {
            return;
        }
        let Some(i) = y.checked_mul(self.w).and_then(|r| r.checked_add(x)).and_then(|p| p.checked_mul(4)) else {
            return;
        };
        let Ok(i) = usize::try_from(i) else { return };
        if let Some(p) = self.px.get_mut(i..i.saturating_add(3)) {
            p.copy_from_slice(&rgb);
        }
    }
}

/// Decodes a kind-11 drawing body into opaque RGBA. `px` is the drawing's
/// own pixel size (fields 0x87 × 0x88) when stated; it must equal the
/// body's. None for anything not understood.
pub fn decode(d: &[u8], px: Option<(u32, u32)>) -> Option<Image> {
    if d.get(0..4)? != b".ims" || u32_at(d, 4)? != 0x0001_0000 || u32_at(d, 0x18)? != 1 {
        return None;
    }
    let (w, h) = (u32_at(d, 8)?, u32_at(d, 12)?);
    if w == 0 || h == 0 || px.is_some_and(|p| p != (w, h)) {
        return None;
    }
    if !limits::image(w as usize, h as usize) {
        return None;
    }
    let n = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    let mut out = Canvas { w: w as u64, h: h as u64, px: vec![255u8; n] };

    // A crafted body can repeat cheap records or claim huge masks: count the
    // records, and the pixels painted (at most 16 page-sized layers).
    let mut budget: u64 = (w as u64 * h as u64).saturating_mul(16);
    let mut i = HEADER;
    loop {
        if !limits::record() {
            return None;
        }
        if u32_at(d, i)? == END {
            break;
        }
        let mut f = [0u32; 17];
        for (k, v) in f.iter_mut().enumerate() {
            *v = u32_at(d, i.checked_add(k * 4)?)?;
        }
        let [kind, x, y, rw, rh, sw, sh, _, coding, colour, _, _, _, v0, v1, v2, len] = f;
        let start = i.checked_add(RECORD)?;
        let len = len as usize;
        let data = d.get(start..start.checked_add(len)?)?;
        i = start.checked_add(len.checked_add(3)? & !3)?;
        if rw == 0 || rh == 0 {
            return None;
        }
        match (kind, coding) {
            (1, 0x20) if colour == 0x27 || colour == 0x12 => background(&mut out, data, [x, y, rw, rh], (sw, sh), &mut budget)?,
            (3, 0x11) => {
                let rgb = match colour {
                    0x12 => [v0, v0, v0],
                    0x21 => [v0, v1, v2],
                    _ => return None,
                };
                if rgb.iter().any(|&v| v > 255) || (sw, sh) != (rw, rh) {
                    return None;
                }
                mask(&mut out, data, [x, y, rw, rh], rgb.map(|v| v as u8), &mut budget)?;
            }
            _ => return None,
        }
    }
    Some(Image { w, h, data: ImageData::Rgba(out.px) })
}

/// Takes `n` pixels from what is left of `budget`; false when it does not cover them.
fn spend(budget: &mut u64, n: u64) -> bool {
    match budget.checked_sub(n) {
        Some(left) => {
            *budget = left;
            true
        }
        None => false,
    }
}

/// A JPEG of `sw` × `sh` stretched over `r` (x, y, w, h) with nearest sampling.
fn background(out: &mut Canvas, jpeg: &[u8], r: [u32; 4], (sw, sh): (u32, u32), budget: &mut u64) -> Option<()> {
    if !limits::image(sw as usize, sh as usize) || crate::pages::jpeg_size(jpeg)? != (sw, sh) {
        return None;
    }
    let mut dec = ezpzxdw_jpeg::Decoder::new(jpeg);
    let pixels = dec.decode().ok()?;
    let info = dec.info()?;
    if (info.width as u32, info.height as u32) != (sw, sh) {
        return None;
    }
    let ch: usize = match info.pixel_format {
        ezpzxdw_jpeg::PixelFormat::L8 => 1,
        ezpzxdw_jpeg::PixelFormat::RGB24 => 3,
        _ => return None,
    };
    if pixels.len() != (sw as usize).checked_mul(sh as usize)?.checked_mul(ch)? {
        return None;
    }
    let [x, y, rw, rh] = r.map(|v| v as u64);
    let (sw, sh) = (sw as u64, sh as u64);
    // only the part of the rectangle that lands on the page
    let cols = rw.min(out.w.saturating_sub(x));
    let rows = rh.min(out.h.saturating_sub(y));
    if !spend(budget, cols * rows) {
        return None;
    }
    for dy in 0..rows {
        let sy = dy * sh / rh;
        for dx in 0..cols {
            let sx = dx * sw / rw;
            let k = usize::try_from((sy * sw + sx) * ch as u64).ok()?;
            let p = pixels.get(k..k.checked_add(ch)?)?;
            let rgb = if ch == 1 { [p[0]; 3] } else { [p[0], p[1], p[2]] };
            out.set(x + dx, y + dy, rgb);
        }
    }
    Some(())
}

/// A Group 4 mask of `r`'s w × h at (x, y); its black pixels painted `rgb`.
fn mask(out: &mut Canvas, data: &[u8], r: [u32; 4], rgb: [u8; 3], budget: &mut u64) -> Option<()> {
    let [x, y, rw, rh] = r;
    if !limits::image_size(rw as usize, rh as usize) {
        return None;
    }
    let (x, y, wu) = (x as u64, y as u64, rw as u64);
    // only the rows that land on the page are decoded and paid for
    let visible = (rh as u64).min(out.h.saturating_sub(y));
    if !spend(budget, wu * visible) {
        return None;
    }
    if visible == 0 {
        return Some(());
    }
    let mut row = 0u64;
    fax::decoder::decode_g4(data.iter().copied(), rw, Some(visible as u32), |transitions: &[u32]| {
        if row >= visible {
            return;
        }
        let py = y + row;
        if py < out.h {
            // transitions alternate white→black, black→white, starting white at 0
            for pair in transitions.chunks(2) {
                let start = (pair[0] as u64).min(wu);
                let end = pair.get(1).map_or(wu, |&e| (e as u64).min(wu));
                // clip to the page
                let end = end.min(out.w.saturating_sub(x));
                for c in start..end {
                    out.set(x + c, py, rgb);
                }
            }
        }
        row += 1;
    });
    (row > 0).then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &[u8] = include_bytes!("../tests/data/tiny.jpg");
    const RED: [u8; 4] = [200, 10, 20, 255];

    fn record(f: [u32; 16], data: &[u8]) -> Vec<u8> {
        let mut v: Vec<u8> = f.iter().flat_map(|n| n.to_le_bytes()).collect();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        while v.len() % 4 != 0 {
            v.push(0);
        }
        v
    }

    fn header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b".ims".to_vec();
        for n in [0x0001_0000, w, h, 300, 300, 1] {
            v.extend(u32::to_le_bytes(n));
        }
        v
    }

    fn g4() -> Vec<u8> {
        // 8 × 2: row 0 all white, row 1 black at 2..6
        let mut enc = fax::encoder::Encoder::new(fax::VecWriter::new());
        let white = [fax::Color::White; 8];
        let mut black = [fax::Color::White; 8];
        black[2..6].fill(fax::Color::Black);
        enc.encode_line(white.iter().copied(), 8).unwrap();
        enc.encode_line(black.iter().copied(), 8).unwrap();
        enc.finish().unwrap().finish()
    }

    fn body() -> Vec<u8> {
        let mut b = header(48, 32);
        b.extend(record([1, 0, 0, 48, 32, 24, 16, 0, 0x20, 0x27, 255, 128, 128, 0, 0, 0], TINY));
        b.extend(record([3, 4, 3, 8, 2, 8, 2, 0, 0x11, 0x21, 255, 255, 255, 200, 10, 20], &g4()));
        b.extend(END.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        b
    }

    fn at(px: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * 48 + x) * 4;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn paints_background_and_mask() {
        let mut dec = ezpzxdw_jpeg::Decoder::new(TINY);
        let jp = dec.decode().unwrap();
        let info = dec.info().unwrap();
        assert_eq!((info.width, info.height, info.pixel_format), (24, 16, ezpzxdw_jpeg::PixelFormat::RGB24));
        let src = |x: usize, y: usize| {
            let i = ((y / 2) * 24 + x / 2) * 3;
            [jp[i], jp[i + 1], jp[i + 2], 255]
        };

        let img = decode(&body(), Some((48, 32))).unwrap();
        assert_eq!((img.w, img.h), (48, 32));
        let ImageData::Rgba(px) = img.data else { panic!() };
        assert_eq!(px.len(), 48 * 32 * 4);
        for x in 4 + 2..=4 + 5 {
            assert_eq!(at(&px, x, 3 + 1), RED);
        }
        for (x, y) in [(4 + 1, 3 + 1), (4 + 2, 3), (0, 0), (47, 31)] {
            assert_ne!(at(&px, x, y), RED);
            assert_eq!(at(&px, x, y), src(x, y));
        }
        assert!(px.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn refuses_what_it_does_not_understand() {
        assert!(decode(&[], None).is_none());
        assert!(decode(b".imsXXXX", None).is_none());
        let b = body();
        assert!(decode(&b, Some((47, 32))).is_none());
        let mut k = b.clone();
        k[HEADER..HEADER + 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode(&k, None).is_none());
        let mut v = b.clone();
        v[0x18..0x1c].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode(&v, None).is_none());

        let marker = b.len() - 8;
        for n in 0..b.len() {
            let r = decode(&b[..n], Some((48, 32)));
            if n < marker + 4 {
                assert!(r.is_none(), "cut at {n}");
            }
        }

        let mut huge = header(100_000, 100_000);
        huge.extend(END.to_le_bytes());
        huge.extend(0u32.to_le_bytes());
        assert!(decode(&huge, None).is_none());
    }

    #[test]
    fn paints_at_most_sixteen_page_sized_layers() {
        // 4 × 4, four white rows
        let mut enc = fax::encoder::Encoder::new(fax::VecWriter::new());
        let white = [fax::Color::White; 4];
        for _ in 0..4 {
            enc.encode_line(white.iter().copied(), 4).unwrap();
        }
        let g4 = enc.finish().unwrap().finish();
        let page = |n: usize| {
            let mut b = header(4, 4);
            for _ in 0..n {
                b.extend(record([3, 0, 0, 4, 4, 4, 4, 0, 0x11, 0x12, 255, 255, 255, 0, 0, 0], &g4));
            }
            b.extend(END.to_le_bytes());
            b.extend(0u32.to_le_bytes());
            b
        };
        assert!(decode(&page(16), Some((4, 4))).is_some());
        assert!(decode(&page(17), Some((4, 4))).is_none());
    }

    #[test]
    fn a_tall_mask_decodes_only_the_rows_on_the_page() {
        // claims 100 000 000 rows; only an end-of-data mark follows
        let eofb = fax::encoder::Encoder::new(fax::VecWriter::new()).finish().unwrap().finish();
        let mut b = header(4, 4);
        b.extend(record([3, 0, 0, 1, 100_000_000, 1, 100_000_000, 0, 0x11, 0x12, 255, 255, 255, 0, 0, 0], &eofb));
        b.extend(END.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        // returns at all (Some or None) rather than decoding 10^8 rows
        let _ = decode(&b, Some((4, 4)));
    }
}
