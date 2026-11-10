//! Scanned bi-level pages: DocuWorks stores them (drawing kind 9) as bare
//! CCITT Group 4 (T.6) data, with the pixel size in the drawing's fields.

use crate::gfx::{Image, ImageData};

/// Decodes `data` as a `w` × `h` Group 4 image into opaque black-on-white
/// RGBA. Rows the data does not reach stay white. None when not one row
/// decodes (not Group 4, or the wrong width).
pub fn decode(data: &[u8], w: u32, h: u32) -> Option<Image> {
    let (wu, hu) = (w as usize, h as usize);
    let mut px = vec![255u8; wu.checked_mul(hu)?.checked_mul(4)?];
    let mut row = 0usize;
    fax::decoder::decode_g4(data.iter().copied(), w, Some(h), |transitions| {
        if row >= hu {
            return;
        }
        // transitions alternate white→black, black→white, starting white at 0
        let line = &mut px[row * wu * 4..(row + 1) * wu * 4];
        for pair in transitions.chunks(2) {
            let start = (pair[0] as usize).min(wu);
            let end = pair.get(1).map_or(wu, |&e| (e as usize).min(wu));
            for p in line[start * 4..end * 4].chunks_exact_mut(4) {
                p[0] = 0;
                p[1] = 0;
                p[2] = 0;
            }
        }
        row += 1;
    });
    if row == 0 {
        return None;
    }
    Some(Image { w, h, data: ImageData::Rgba(px) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_group4_rows() {
        // 8 × 2: row 0 all white, row 1 black at 2..6, encoded with the fax crate's encoder
        let mut enc = fax::encoder::Encoder::new(fax::VecWriter::new());
        let white = [fax::Color::White; 8];
        let mut black = [fax::Color::White; 8];
        black[2..6].fill(fax::Color::Black);
        enc.encode_line(white.iter().copied(), 8).unwrap();
        enc.encode_line(black.iter().copied(), 8).unwrap();
        let data = enc.finish().unwrap().finish();

        let img = decode(&data, 8, 2).unwrap();
        let ImageData::Rgba(px) = img.data else { panic!() };
        let dark: Vec<bool> = px.chunks_exact(4).map(|p| p[0] == 0).collect();
        assert_eq!(&dark[..8], &[false; 8]);
        assert_eq!(&dark[8..], &[false, false, true, true, true, true, false, false]);
        assert!(px.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn refuses_data_that_is_not_group4() {
        assert!(decode(&[], 8, 2).is_none());
    }
}
