//! Pictures DocuWorks stores losslessly: a picture annotation's drawing of
//! kind 10, a 24-bit DIB header followed by predicted runs and new colours.
//!
//! ```text
//! d[0..40]  BITMAPINFOHEADER (little-endian): size 40, width > 0,
//!           height > 0, planes 1, 24 bits, compression 0 (else refused)
//! d[40..]   bit stream, most significant bit of each byte first
//!
//! Pixels run top row first, left to right, with no mark between rows.
//! Predictors (dx, dy): L (-1,0)  A (0,-1)  UL (-1,-1)  UR (1,-1)  U2R (2,-1);
//! outside the picture they are white. They sit in a move-to-front list M,
//! first [L, A, UL, UR, U2R]. State: swapped = false, after_run = false.
//!
//! symbol:
//!   11      switch: 0 → sel 1; 1 then 2 bits: 00 → 2, 01 → 3, 10 → 4,
//!           11 then 1 bit: 0 → escape (swapped = !swapped, after_run kept),
//!           1 → refused. Else a run with rank sel + after_run.
//!   0 / 10  not swapped: 0 = run, 10 = new colour; swapped: the reverse.
//!           A run has rank after_run (0 or 1).
//! new colour: three differences B, G, R added (mod 256) to the left pixel
//!   (white at x = 0); one pixel; after_run = false.
//! run: length n; rank ≥ 5 is refused; M[rank] moves to the front; n pixels
//!   copied each from that predictor; after_run = (n != 17057).
//!
//! run length (prefix + extra bits, length = base + extra):
//!   00 +0 → 1   01 +1 → 2   100 +1 → 4   101 +2 → 6   1100 +3 → 10
//!   1101 +3 → 18   11100 +4 → 26   111010 +3 → 42   111011 +4 → 50
//!   111100 +5 → 66   111101 +6 → 98   111110 +9 → 162   111111 +14 → 674
//! difference: 000 → 0, 001 → +1, 010 → -1; otherwise prefix, sign bit
//!   (1 = negative), extra bits, magnitude = base + extra:
//!   011 +1 → 2   100 +2 → 4   101 +3 → 8   110 +5 → 16   1110 +5 → 48
//!   1111 +6 → 80
//! end: all pixels written (the rest of the bits, DocuWorks pads with 1s,
//!   ignored); bits running out leave the remaining pixels white.
//! ```
//!
//! Checked against DocuWorks 9 (xdwapi 9.0.1.2, 2026-11-15): pictures added
//! with XDW_AddAnnotation from BMP files (synthetic, 148 samples, every pixel
//! equal), and customer drawings whose decoded pixels, added again through
//! the API, give back the stored bytes exactly.

use crate::gfx::{Image, ImageData};
use crate::limits;

const HEADER: usize = 40;
/// A run this long does not count as a run for the next symbol's rank.
const LONGEST_RUN: usize = 17057;
/// The predictors' offsets (dx, dy): L, A, UL, UR, U2R.
const PREDICTORS: [(isize, isize); 5] = [(-1, 0), (0, -1), (-1, -1), (1, -1), (2, -1)];
/// Run-length codes: (prefix bits, prefix length, extra bits, base).
const RUNS: [(u32, u32, u32, usize); 13] = [
    (0b00, 2, 0, 1),
    (0b01, 2, 1, 2),
    (0b100, 3, 1, 4),
    (0b101, 3, 2, 6),
    (0b1100, 4, 3, 10),
    (0b1101, 4, 3, 18),
    (0b11100, 5, 4, 26),
    (0b111010, 6, 3, 42),
    (0b111011, 6, 4, 50),
    (0b111100, 6, 5, 66),
    (0b111101, 6, 6, 98),
    (0b111110, 6, 9, 162),
    (0b111111, 6, 14, 674),
];

fn u16_at(d: &[u8], i: usize) -> Option<u16> {
    let b = d.get(i..i.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], i: usize) -> Option<u32> {
    let b = d.get(i..i.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Reads bits, most significant first.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.d.get(self.pos / 8)?;
        let b = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Some(b as u32)
    }

    /// `n` bits (at most 31) as an unsigned number, first bit highest.
    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Some(v)
    }
}

/// How one symbol of the stream ended.
enum Step {
    Done,
    OutOfBits,
    Refused,
}

/// What follows the switch code `11`.
enum Selector {
    Rank(usize),
    Escape,
    Unknown,
}

fn selector(r: &mut Bits) -> Option<Selector> {
    if r.bit()? == 0 {
        return Some(Selector::Rank(1));
    }
    Some(match r.bits(2)? {
        0b00 => Selector::Rank(2),
        0b01 => Selector::Rank(3),
        0b10 => Selector::Rank(4),
        _ if r.bit()? == 0 => Selector::Escape,
        _ => Selector::Unknown,
    })
}

fn run_length(r: &mut Bits) -> Option<usize> {
    // read the prefix bit by bit until it matches one code
    let mut prefix = 0u32;
    let mut len = 0u32;
    while len < 6 {
        prefix = (prefix << 1) | r.bit()?;
        len += 1;
        if let Some(&(_, _, extra, base)) = RUNS.iter().find(|&&(p, l, _, _)| l == len && p == prefix) {
            return Some(base + r.bits(extra)? as usize);
        }
    }
    None
}

fn difference(r: &mut Bits) -> Option<u8> {
    let (base, extra) = match r.bits(3)? {
        0b000 => return Some(0),
        0b001 => return Some(1),
        0b010 => return Some(255),
        0b011 => (2u32, 1),
        0b100 => (4, 2),
        0b101 => (8, 3),
        0b110 => (16, 5),
        _ => {
            if r.bit()? == 0 {
                (48, 5)
            } else {
                (80, 6)
            }
        }
    };
    let negative = r.bit()? == 1;
    let m = (base + r.bits(extra)?) as u8;
    Some(if negative { m.wrapping_neg() } else { m })
}

/// Decodes a kind-10 drawing body into opaque RGBA (top row first). `px` is the drawing's
/// own pixel size (fields 0x87 × 0x88) when stated; it must equal the header's. None for
/// anything not understood.
pub fn decode(d: &[u8], px: Option<(u32, u32)>) -> Option<Image> {
    if u32_at(d, 0)? != 40 || u16_at(d, 12)? != 1 || u16_at(d, 14)? != 24 || u32_at(d, 16)? != 0 {
        return None;
    }
    let (w, h) = (u32_at(d, 4)? as i32, u32_at(d, 8)? as i32);
    if w <= 0 || h <= 0 {
        return None;
    }
    let (w, h) = (w as u32, h as u32);
    if px.is_some_and(|p| p != (w, h)) {
        return None;
    }
    if !limits::image(w as usize, h as usize) {
        return None;
    }
    let (wu, hu) = (w as usize, h as usize);
    let total = wu.checked_mul(hu)?;
    let mut out = vec![255u8; total.checked_mul(4)?];

    // the RGB the predictor `k` gives at pixel (x, y); white outside
    let predict = |out: &[u8], x: usize, y: usize, k: usize| -> [u8; 3] {
        let (dx, dy) = PREDICTORS[k];
        let (nx, ny) = (x as isize + dx, y as isize + dy);
        if nx < 0 || ny < 0 || nx as usize >= wu {
            return [255; 3];
        }
        let i = (ny as usize * wu + nx as usize) * 4;
        match out.get(i..i + 3) {
            Some(p) => [p[0], p[1], p[2]],
            None => [255; 3],
        }
    };

    let mut r = Bits { d: d.get(HEADER..)?, pos: 0 };
    let mut order: [usize; 5] = [0, 1, 2, 3, 4];
    let mut swapped = false;
    let mut after_run = false;
    let mut p = 0usize;
    while p < total {
        let step = (|| -> Step {
            let Some(a) = r.bit() else { return Step::OutOfBits };
            let b = if a == 1 {
                let Some(b) = r.bit() else { return Step::OutOfBits };
                b
            } else {
                0
            };
            let rank = if a == 1 && b == 1 {
                let sel = match selector(&mut r) {
                    None => return Step::OutOfBits,
                    Some(Selector::Rank(s)) => s,
                    Some(Selector::Escape) => {
                        // swap the new-colour and run codes
                        swapped = !swapped;
                        return Step::Done;
                    }
                    Some(Selector::Unknown) => return Step::Refused,
                };
                Some(sel + after_run as usize)
            } else if (a == 0) != swapped {
                Some(after_run as usize)
            } else {
                None
            };
            match rank {
                Some(rank) => {
                    let Some(n) = run_length(&mut r) else { return Step::OutOfBits };
                    if rank >= 5 {
                        return Step::Refused;
                    }
                    let k = order[rank];
                    order.copy_within(0..rank, 1);
                    order[0] = k;
                    for _ in 0..n.min(total - p) {
                        let c = predict(&out, p % wu, p / wu, k);
                        if let Some(o) = out.get_mut(p * 4..p * 4 + 3) {
                            o.copy_from_slice(&c);
                        }
                        p += 1;
                    }
                    after_run = n != LONGEST_RUN;
                }
                None => {
                    let (Some(db), Some(dg), Some(dr)) = (difference(&mut r), difference(&mut r), difference(&mut r)) else {
                        return Step::OutOfBits;
                    };
                    let [br, bg, bb] = predict(&out, p % wu, p / wu, 0);
                    let c = [br.wrapping_add(dr), bg.wrapping_add(dg), bb.wrapping_add(db)];
                    if let Some(o) = out.get_mut(p * 4..p * 4 + 3) {
                        o.copy_from_slice(&c);
                    }
                    p += 1;
                    after_run = false;
                }
            }
            Step::Done
        })();
        match step {
            Step::Done => {}
            Step::Refused => return None,
            // the bits ran out: what is decoded stands, the rest stays white
            Step::OutOfBits if p > 0 => break,
            Step::OutOfBits => return None,
        }
    }
    Some(Image { w, h, data: ImageData::Rgba(out) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: i32, h: i32) -> Vec<u8> {
        let mut v = Vec::with_capacity(40);
        v.extend(40u32.to_le_bytes());
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(24u16.to_le_bytes());
        v.extend(0u32.to_le_bytes());
        v.resize(40, 0);
        v
    }

    fn pixels(img: &Image) -> Vec<[u8; 4]> {
        match &img.data {
            ImageData::Rgba(v) => v.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
            ImageData::Jpeg(_) => panic!("not RGBA"),
        }
    }

    #[test]
    fn new_colour_then_runs_left_and_above() {
        let mut d = header(3, 2);
        d.extend([0x89, 0x24, 0x7F]);
        let img = decode(&d, Some((3, 2))).unwrap();
        assert_eq!((img.w, img.h), (3, 2));
        assert_eq!(pixels(&img), vec![[0, 0, 0, 255]; 6]);
    }

    #[test]
    fn escape_swaps_new_colour_and_run_codes() {
        let mut d = header(2, 1);
        d.extend([0xF8, 0x49, 0x00, 0xCF]);
        let img = decode(&d, None).unwrap();
        assert_eq!(pixels(&img), vec![[0, 0, 0, 255], [2, 0, 0, 255]]);
    }

    #[test]
    fn refuses_unknown_selector() {
        let mut d = header(1, 1);
        d.push(0xFF);
        assert!(decode(&d, None).is_none());
    }

    #[test]
    fn refuses_other_headers() {
        let mut d = header(1, 1);
        d[14] = 8;
        d.push(0x00);
        assert!(decode(&d, None).is_none());
        let mut d = header(3, 2);
        d.extend([0x89, 0x24, 0x7F]);
        assert!(decode(&d, Some((3, 3))).is_none());
        assert!(decode(&[], None).is_none());
    }
}
