//! Windows device-independent bitmaps (DIB): BITMAPINFO + pixel rows.
//!
//! Decodes 1/4/8/16/24/32-bit pictures, RLE4/RLE8, and passes JPEG through.

use crate::gfx::{Image, ImageData};

fn u16le(b: &[u8], i: usize) -> u32 {
    i.checked_add(2).and_then(|e| b.get(i..e)).map(|x| u16::from_le_bytes([x[0], x[1]]) as u32).unwrap_or(0)
}
fn u32le(b: &[u8], i: usize) -> u32 {
    i.checked_add(4).and_then(|e| b.get(i..e)).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])).unwrap_or(0)
}

/// What a BITMAPINFO says.
#[derive(Debug, Clone)]
pub struct Info {
    pub w: u32,
    pub h: u32,
    pub top_down: bool,
    pub bpp: u32,
    pub compression: u32,
    /// Colour table as 0xRRGGBB.
    pub palette: Vec<u32>,
    pub masks: Option<[u32; 3]>,
}

pub fn info(bmi: &[u8]) -> Option<Info> {
    let size = u32le(bmi, 0) as usize;
    if size == 12 {
        let w = u16le(bmi, 4);
        let h = u16le(bmi, 6);
        let bpp = u16le(bmi, 10);
        let n = if bpp <= 8 { 1usize << bpp } else { 0 };
        let palette = (0..n)
            .map(|k| {
                let p = 12 + 3 * k;
                let c = bmi.get(p..p.checked_add(3)?)?;
                Some((c[2] as u32) << 16 | (c[1] as u32) << 8 | c[0] as u32)
            })
            .map(|c| c.unwrap_or(0))
            .collect();
        return Some(Info { w, h, top_down: false, bpp, compression: 0, palette, masks: None });
    }
    if size < 40 || bmi.len() < 40 {
        return None;
    }
    let w = u32le(bmi, 4) as i32;
    let h = u32le(bmi, 8) as i32;
    let bpp = u16le(bmi, 14);
    let compression = u32le(bmi, 16);
    let used = u32le(bmi, 32) as usize;
    let mut at = size;
    let mut masks = None;
    if compression == 3 || compression == 6 {
        if size >= 52 {
            masks = Some([u32le(bmi, 40), u32le(bmi, 44), u32le(bmi, 48)]);
        } else {
            // offsets past usize (biSize near u32::MAX on wasm32) read as 0,
            // as they would past the end of the buffer
            let m = |k: usize| at.checked_add(k).map_or(0, |p| u32le(bmi, p));
            masks = Some([m(0), m(4), m(8)]);
            at = at.saturating_add(12);
        }
    }
    let n = if bpp <= 8 { if used > 0 { used.min(256) } else { 1usize << bpp } } else { used.min(256) };
    let palette = (0..n)
        .map(|k| {
            // k < 256: 4 * k cannot overflow, at + 4 * k can
            let p = at.checked_add(4 * k)?;
            let c = bmi.get(p..p.checked_add(3)?)?;
            Some((c[2] as u32) << 16 | (c[1] as u32) << 8 | c[0] as u32)
        })
        .map(|c| c.unwrap_or(0))
        .collect();
    if w <= 0 || h == 0 {
        return None;
    }
    Some(Info { w: w as u32, h: h.unsigned_abs(), top_down: h < 0, bpp, compression, palette, masks })
}

/// Decode to RGBA (top row first). `usage` 1 (palette indices) is treated
/// as plain colours.
pub fn decode(bmi: &[u8], bits: &[u8]) -> Option<Image> {
    let inf = info(bmi)?;
    let (w, h) = (inf.w as usize, inf.h as usize);
    if w == 0 || h == 0 || !crate::limits::image_size(w, h) {
        return None;
    }
    match inf.compression {
        4 => return Some(Image { w: inf.w, h: inf.h, data: ImageData::Jpeg(bits.to_vec()) }),
        5 => return None, // PNG inside a DIB: not seen in DocuWorks files
        _ => {}
    }
    if !crate::limits::image(w, h) {
        return None;
    }
    let mut out = vec![255u8; w * h * 4];
    let put = |out: &mut Vec<u8>, x: usize, row: usize, c: u32| {
        let y = if inf.top_down { row } else { h - 1 - row };
        let p = (y * w + x) * 4;
        out[p] = (c >> 16) as u8;
        out[p + 1] = (c >> 8) as u8;
        out[p + 2] = c as u8;
        out[p + 3] = 255;
    };
    // colours missing from a short palette are black
    let pal = |i: usize| inf.palette.get(i).copied().unwrap_or(0);
    if inf.compression == 1 || inf.compression == 2 {
        rle(&inf, bits, |x, row, i| {
            if x < w && row < h {
                put(&mut out, x, row, pal(i));
            }
        });
        return Some(Image { w: inf.w, h: inf.h, data: ImageData::Rgba(out) });
    }
    let stride = ((w * inf.bpp as usize + 31) / 32) * 4;
    for row in 0..h {
        let line = match bits.get(row * stride..row * stride + stride) {
            Some(l) => l,
            None => break,
        };
        for x in 0..w {
            let c = match inf.bpp {
                1 => pal(((line[x / 8] >> (7 - x % 8)) & 1) as usize),
                4 => pal(((line[x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 15) as usize),
                8 => pal(line[x] as usize),
                16 => {
                    let v = u16le(line, x * 2);
                    match inf.masks {
                        Some(m) => from_masks(v, m),
                        None => {
                            let r = (v >> 10) & 31;
                            let g = (v >> 5) & 31;
                            let b = v & 31;
                            (r * 255 / 31) << 16 | (g * 255 / 31) << 8 | b * 255 / 31
                        }
                    }
                }
                24 => (line[x * 3 + 2] as u32) << 16 | (line[x * 3 + 1] as u32) << 8 | line[x * 3] as u32,
                32 => {
                    let v = u32le(line, x * 4);
                    match inf.masks {
                        Some(m) => from_masks(v, m),
                        None => v & 0xffffff,
                    }
                }
                _ => 0,
            };
            put(&mut out, x, row, c);
        }
    }
    Some(Image { w: inf.w, h: inf.h, data: ImageData::Rgba(out) })
}

fn from_masks(v: u32, m: [u32; 3]) -> u32 {
    let ch = |mask: u32| -> u32 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        let bits = (mask >> shift).count_ones();
        let x = (v & mask) >> shift;
        if bits >= 8 {
            x >> (bits - 8)
        } else {
            x * 255 / ((1 << bits) - 1)
        }
    };
    ch(m[0]) << 16 | ch(m[1]) << 8 | ch(m[2])
}

fn rle(inf: &Info, b: &[u8], mut set: impl FnMut(usize, usize, usize)) {
    let four = inf.compression == 2;
    let (mut x, mut row) = (0usize, 0usize);
    let mut i = 0;
    while i + 1 < b.len() {
        let (n, c) = (b[i] as usize, b[i + 1]);
        i += 2;
        if n > 0 {
            for k in 0..n {
                let idx = if four { if k % 2 == 0 { c >> 4 } else { c & 15 } } else { c };
                set(x, row, idx as usize);
                x += 1;
            }
            continue;
        }
        match c {
            0 => {
                x = 0;
                row += 1;
            }
            1 => break,
            2 => {
                if i + 1 >= b.len() {
                    break;
                }
                x += b[i] as usize;
                row += b[i + 1] as usize;
                i += 2;
            }
            m => {
                let m = m as usize;
                let bytes = if four { (m + 1) / 2 } else { m };
                for k in 0..m {
                    let idx = if four {
                        let byte = b.get(i + k / 2).copied().unwrap_or(0);
                        if k % 2 == 0 { byte >> 4 } else { byte & 15 }
                    } else {
                        b.get(i + k).copied().unwrap_or(0)
                    };
                    set(x, row, idx as usize);
                    x += 1;
                }
                i += bytes + (bytes & 1);
            }
        }
    }
}

/// Size of a BITMAPINFO (header, masks and colours): where the bits start.
pub fn info_len(d: &[u8]) -> Option<usize> {
    let inf = info(d)?;
    let size = u32le(d, 0) as usize;
    // biSize comes from the file: a sum past usize is no header
    let masks = if inf.masks.is_some() && size == 40 { 12 } else { 0 };
    size.checked_add(inf.palette.len().checked_mul(4)?)?.checked_add(masks)
}

/// A DIB as DocuWorks stores it in pictures, thumbnails and bitmap pages
/// (entry kind 7): BITMAPINFO, then either the bits, or `n` (u32) strips,
/// each `stored size, size, rows` (u32) followed by that many bytes of bits
/// compressed with LHA -lh5-. Small pictures have one strip; scanned pages
/// are cut into strips of a few hundred rows.
///
/// The strip form keeps rows top-first although the height is positive, in
/// one strip or several (DocuWorks 9 API render of its own pastes,
/// API-added pictures and thumbnails, and of pastes in DocuWorks 10-saved
/// documents; scanned pages checked against the same book's G4 pages).
///
/// Returns the picture and whether its rows were stored bottom-up (what a
/// caller cropping in DIB terms needs).
pub fn decode_stored(d: &[u8]) -> Option<(Image, bool)> {
    let at = info_len(d)?;
    // the header's own size field can point past the data
    let bmi = d.get(..at)?;
    let rest = d.get(at..)?;
    let Some(bits) = stored_strips(rest) else {
        return Some((decode(bmi, rest)?, !info(bmi)?.top_down));
    };
    if u32le(bmi, 0) >= 40 {
        // Strips keep their rows top-first although the height is positive,
        // in one strip or several: read them as a top-down DIB.
        let mut bmi = bmi.to_vec();
        let h = i32::from_le_bytes([bmi[8], bmi[9], bmi[10], bmi[11]]);
        if h > 0 {
            bmi[8..12].copy_from_slice(&(-h).to_le_bytes());
        }
        return Some((decode(&bmi, &bits)?, false));
    }
    Some((decode(bmi, &bits)?, !info(bmi)?.top_down))
}

/// The bits of a stored DIB's strips, expanded and joined; None when `rest`
/// is not in that form (then it is the bits themselves).
/// Each strip's expansion is checked and counted by [`crate::lzh::decompress`].
fn stored_strips(rest: &[u8]) -> Option<Vec<u8>> {
    if rest.len() < 16 {
        return None;
    }
    // a strip count, not the first bytes of plain bits: every strip must parse
    let n = u32le(rest, 0) as usize;
    if n == 0 || n > 4096 {
        return None;
    }
    let mut bits = Vec::new();
    let mut at = 4;
    for _ in 0..n {
        let head = rest.get(at..at + 12)?;
        let (stored, size) = (u32le(head, 0) as usize, u32le(head, 4) as usize);
        let data = rest.get(at + 12..(at + 12).checked_add(stored)?)?;
        if size == 0 {
            return None;
        }
        bits.extend(crate::lzh::decompress(data, size).ok()?);
        at += 12 + stored;
    }
    Some(bits)
}

/// The DocuWorks form of a DIB: `bmi` + compressed bits block, in one strip,
/// rows top-first as DocuWorks writes and reads them. `bits` are in the order
/// the header says (bottom-up for a positive height); the header is written
/// unchanged.
///
/// Callers pass uncompressed bits (BI_RGB or BI_BITFIELDS) of exactly `rows`
/// rows, as [`crate::edit::dib_24`] makes them; only such bits are turned
/// top-first. Anything else is stored as given.
pub fn encode_stored(bmi: &[u8], bits: &[u8], rows: u32) -> Vec<u8> {
    let h = bmi.get(8..12).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0);
    let bottom_up = u32le(bmi, 0) >= 40 && h > 0;
    let uncompressed = matches!(u32le(bmi, 16), 0 | 3);
    let flip = bottom_up && uncompressed && rows > 0 && bits.len() % rows as usize == 0;
    debug_assert!(
        !bottom_up || flip,
        "encode_stored got a bottom-up header with bits it cannot turn top-first \
         (compressed, or not a whole number of rows)"
    );
    let stored: Vec<u8> = if flip {
        let stride = bits.len() / rows as usize;
        if stride == 0 { bits.to_vec() } else { bits.chunks(stride).rev().flatten().copied().collect() }
    } else {
        bits.to_vec()
    };
    let packed = crate::lzh::compress(&stored);
    let mut out = bmi.to_vec();
    for v in [1u32, packed.len() as u32, stored.len() as u32, rows] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&packed);
    out
}

/// Crop `img` to the source rectangle given in DIB terms (y measured from
/// the bottom for bottom-up pictures is already handled by the caller).
pub fn crop(img: &Image, x: u32, y: u32, w: u32, h: u32) -> Option<Image> {
    let ImageData::Rgba(px) = &img.data else { return None };
    if x == 0 && y == 0 && w == img.w && h == img.h {
        return None;
    }
    let w = w.min(img.w.saturating_sub(x));
    let h = h.min(img.h.saturating_sub(y));
    if w == 0 || h == 0 {
        return None;
    }
    if !crate::limits::charge((w as usize) * (h as usize) * 4) {
        return None;
    }
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for r in y..y + h {
        let s = ((r * img.w + x) * 4) as usize;
        out.extend_from_slice(&px[s..s + (w * 4) as usize]);
    }
    Some(Image { w, h, data: ImageData::Rgba(out) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_dib_in_several_strips() {
        // 2x2, 1 bpp, as a scanned page stores it: two strips of one row each
        let mut bmi = vec![0u8; 40 + 8];
        bmi[0] = 40;
        bmi[4] = 2;
        bmi[8] = 2;
        bmi[12] = 1;
        bmi[14] = 1;
        bmi[44] = 255;
        bmi[45] = 255;
        bmi[46] = 255;
        let rows: [[u8; 4]; 2] = [[0b1000_0000, 0, 0, 0], [0b0100_0000, 0, 0, 0]];
        let mut d = bmi.clone();
        d.extend(2u32.to_le_bytes());
        for r in &rows {
            let packed = crate::lzh::compress(r);
            for v in [packed.len() as u32, 4, 1] {
                d.extend(v.to_le_bytes());
            }
            d.extend(packed);
        }
        let joined: Vec<u8> = rows.concat();
        // strips hold the rows top-first, as a top-down DIB would
        let mut top_down = bmi.clone();
        top_down[8..12].copy_from_slice(&(-2i32).to_le_bytes());
        assert_eq!(decode_stored(&d).unwrap(), (decode(&top_down, &joined).unwrap(), false));
        // the one-strip form round-trips: written top-first, read top-first
        let one = encode_stored(&bmi, &joined, 2);
        assert_eq!(decode_stored(&one).unwrap(), (decode(&bmi, &joined).unwrap(), false));
    }

    #[test]
    fn one_strip_rows_are_top_first() {
        // 2x2, 1 bpp, palette black/white, positive height; one strip as
        // DocuWorks writes it, first stored row = top (a set bit is colour 1,
        // white)
        let mut bmi = vec![0u8; 40 + 8];
        bmi[0] = 40;
        bmi[4] = 2;
        bmi[8] = 2;
        bmi[12] = 1;
        bmi[14] = 1;
        bmi[44] = 255;
        bmi[45] = 255;
        bmi[46] = 255;
        let rows = [0b0100_0000u8, 0, 0, 0, 0b1000_0000, 0, 0, 0];
        let packed = crate::lzh::compress(&rows);
        let mut d = bmi.clone();
        for v in [1u32, packed.len() as u32, 8, 2] {
            d.extend(v.to_le_bytes());
        }
        d.extend(packed);
        let (img, bottom_up) = decode_stored(&d).unwrap();
        assert!(!bottom_up);
        let ImageData::Rgba(p) = img.data else { panic!() };
        // top row = first stored row: black, white
        assert_eq!(&p[0..3], &[0, 0, 0]);
        assert_eq!(&p[4..7], &[255, 255, 255]);
        // bottom row = second stored row: white, black
        assert_eq!(&p[8..11], &[255, 255, 255]);
        assert_eq!(&p[12..15], &[0, 0, 0]);
    }

    #[test]
    fn crafted_header_sizes_do_not_panic() {
        // a core header (12) whose bit count is 0, in 16 bytes
        let mut a = vec![0u8; 16];
        a[0] = 12;
        assert!(decode_stored(&a).is_none());
        // 1 × 1 below, so the header itself is read and its size used
        // a size field past the end of the data
        let mut b = vec![0u8; 100];
        b[0..4].copy_from_slice(&1000u32.to_le_bytes());
        b[4] = 1;
        b[8] = 1;
        assert!(decode_stored(&b).is_none());
        // a size field near u32::MAX
        let mut c = vec![0u8; 48];
        c[0..4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        c[4] = 1;
        c[8] = 1;
        assert!(decode_stored(&c).is_none());
    }

    #[test]
    fn one_bit_bottom_up() {
        // 2x2, 1 bpp, palette black/white; bottom row first
        let mut bmi = vec![0u8; 40 + 8];
        bmi[0] = 40;
        bmi[4] = 2;
        bmi[8] = 2;
        bmi[12] = 1;
        bmi[14] = 1;
        bmi[44] = 255;
        bmi[45] = 255;
        bmi[46] = 255;
        let bits = [0b1000_0000, 0, 0, 0, 0b0100_0000, 0, 0, 0];
        let img = decode(&bmi, &bits).unwrap();
        let ImageData::Rgba(p) = img.data else { panic!() };
        // top row = second stored row: black, white
        assert_eq!(&p[0..3], &[0, 0, 0]);
        assert_eq!(&p[4..7], &[255, 255, 255]);
        // bottom row = first stored row: white, black
        assert_eq!(&p[8..11], &[255, 255, 255]);
        assert_eq!(&p[12..15], &[0, 0, 0]);
    }
}
