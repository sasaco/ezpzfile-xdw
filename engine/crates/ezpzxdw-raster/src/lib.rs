//! Draws a page's display list ([`ezpzxdw_core::gfx::Display`]) to pixels, so
//! a page can be looked at without a browser: scanned pages, drawings, lines
//! and pictures. Text runs are not drawn (no fonts here); read them with the
//! text extraction instead. [`Raster::skipped_text`] says how many there were.

use ezpzxdw_core::gfx::{Clip, Display, Image, ImageData, Item, Path as XPath, Seg};
use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke,
    StrokeDash, Transform,
};

/// 1/100 mm per inch.
const UNITS_PER_INCH: f32 = 2540.0;

/// The largest page we draw, in pixels (about A0 at 300 dpi).
pub const MAX_PIXELS: u64 = 160_000_000;

pub struct Raster {
    pub pixmap: Pixmap,
    /// Text runs left out (not drawn).
    pub skipped_text: usize,
    /// Pictures that could not be decoded.
    pub skipped_images: usize,
}

impl Raster {
    pub fn png(&self) -> Result<Vec<u8>, String> {
        self.pixmap.encode_png().map_err(|e| e.to_string())
    }
}

/// Draws `d` at `dpi` on a white page.
pub fn render(d: &Display, dpi: f32) -> Result<Raster, String> {
    if !(dpi.is_finite() && dpi > 0.0) {
        return Err(format!("bad dpi {dpi}"));
    }
    let s = dpi / UNITS_PER_INCH;
    let (w, h) = ((d.w * s).ceil().max(1.0), (d.h * s).ceil().max(1.0));
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(format!("page of {w} × {h} pixels is too large; use a lower dpi"));
    }
    let mut pixmap = Pixmap::new(w as u32, h as u32).ok_or("cannot allocate the page")?;
    pixmap.fill(Color::WHITE);
    let page = Transform::from_scale(s, s);

    let masks: Vec<Option<Mask>> = d.clips.iter().map(|c| clip_mask(c, pixmap.width(), pixmap.height(), page)).collect();
    let mask_of = |clip: u32| masks.get(clip as usize).and_then(|m| m.as_ref());
    let mut decoded: Vec<Option<Option<Pixmap>>> = vec![None; d.images.len()];
    let (mut skipped_text, mut skipped_images) = (0, 0);

    for item in &d.items {
        match item {
            Item::Fill { path, color, evenodd, clip, mul } => {
                let Some(p) = to_path(path) else { continue };
                let mut paint = solid(*color);
                if *mul {
                    paint.blend_mode = BlendMode::Multiply;
                }
                let rule = if *evenodd { FillRule::EvenOdd } else { FillRule::Winding };
                pixmap.fill_path(&p, &paint, rule, page, mask_of(*clip));
            }
            Item::Stroke { path, color, width, dash, cap, join, clip } => {
                let Some(p) = to_path(path) else { continue };
                let mut stroke = Stroke {
                    // at least a hairline (one pixel)
                    width: width.max(1.0 / s),
                    line_cap: match cap {
                        0 => LineCap::Round,
                        1 => LineCap::Square,
                        _ => LineCap::Butt,
                    },
                    line_join: match join {
                        0 => LineJoin::Round,
                        1 => LineJoin::Bevel,
                        _ => LineJoin::Miter,
                    },
                    ..Stroke::default()
                };
                if dash.len() >= 2 && dash.len() % 2 == 0 && dash.iter().all(|v| *v > 0.0) {
                    stroke.dash = StrokeDash::new(dash.clone(), 0.0);
                }
                pixmap.stroke_path(&p, &solid(*color), &stroke, page, mask_of(*clip));
            }
            Item::Text { .. } => skipped_text += 1,
            Item::Image { image, m, clip, alpha, mul } => {
                let k = *image as usize;
                let Some(slot) = decoded.get_mut(k) else { continue };
                let pic = slot.get_or_insert_with(|| d.images.get(k).and_then(decode));
                let Some(pic) = pic.as_ref() else {
                    skipped_images += 1;
                    continue;
                };
                // pixels → unit square → page (through m) → device
                let unit = Transform::from_scale(1.0 / pic.width() as f32, 1.0 / pic.height() as f32);
                let place = Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
                let t = page.pre_concat(place).pre_concat(unit);
                let paint = PixmapPaint {
                    opacity: alpha.clamp(0.0, 1.0),
                    blend_mode: if *mul { BlendMode::Multiply } else { BlendMode::SourceOver },
                    quality: tiny_skia::FilterQuality::Bilinear,
                };
                pixmap.draw_pixmap(0, 0, pic.as_ref(), &paint, t, mask_of(*clip));
            }
        }
    }
    Ok(Raster { pixmap, skipped_text, skipped_images })
}

fn solid(rgb: u32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255);
    paint.anti_alias = true;
    paint
}

fn to_path(p: &XPath) -> Option<Path> {
    let mut b = PathBuilder::new();
    for seg in &p.0 {
        match *seg {
            Seg::M(x, y) => b.move_to(x, y),
            Seg::L(x, y) => b.line_to(x, y),
            Seg::C(x1, y1, x2, y2, x, y) => b.cubic_to(x1, y1, x2, y2, x, y),
            Seg::Z => b.close(),
        }
    }
    b.finish()
}

/// The clip as a mask: the intersection of its paths. None for "no clip".
fn clip_mask(c: &Clip, w: u32, h: u32, page: Transform) -> Option<Mask> {
    let mut paths = c.0.iter().filter_map(to_path);
    let first = paths.next()?;
    let mut mask = Mask::new(w, h)?;
    mask.fill_path(&first, FillRule::Winding, true, page);
    for p in paths {
        mask.intersect_path(&p, FillRule::Winding, true, page);
    }
    Some(mask)
}

/// A picture as a premultiplied pixmap.
fn decode(im: &Image) -> Option<Pixmap> {
    let (w, h) = (im.w, im.h);
    let rgba: Vec<u8> = match &im.data {
        ImageData::Rgba(px) => px.clone(),
        ImageData::Jpeg(bytes) => {
            let mut dec = jpeg_decoder::Decoder::new(bytes.as_slice());
            let px = dec.decode().ok()?;
            let info = dec.info()?;
            if (info.width as u32, info.height as u32) != (w, h) {
                return None;
            }
            match info.pixel_format {
                jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
                jpeg_decoder::PixelFormat::L16 => px.chunks_exact(2).flat_map(|g| [g[0], g[0], g[0], 255]).collect(),
                jpeg_decoder::PixelFormat::RGB24 => px.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
                jpeg_decoder::PixelFormat::CMYK32 => px
                    .chunks_exact(4)
                    .flat_map(|p| {
                        // Adobe JPEGs store CMYK inverted
                        let k = p[3] as u16;
                        let ch = |c: u8| ((c as u16 * k) / 255) as u8;
                        [ch(p[0]), ch(p[1]), ch(p[2]), 255]
                    })
                    .collect(),
            }
        }
    };
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        return None;
    }
    // tiny-skia wants premultiplied alpha
    let mut pm = rgba;
    for p in pm.chunks_exact_mut(4) {
        let a = p[3] as u16;
        if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u16 * a + 127) / 255) as u8;
            }
        }
    }
    Pixmap::from_vec(pm, tiny_skia::IntSize::from_wh(w, h)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Display {
        // 1 inch square
        Display::new(2540.0, 2540.0)
    }

    fn px(r: &Raster, x: u32, y: u32) -> [u8; 4] {
        let c = r.pixmap.pixel(x, y).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn draws_a_filled_rect_at_dpi() {
        let mut d = page();
        d.items.push(Item::Fill { path: XPath::rect(0.0, 0.0, 1270.0, 1270.0), color: 0xff0000, evenodd: false, clip: 0, mul: false });
        let r = render(&d, 100.0).unwrap();
        assert_eq!((r.pixmap.width(), r.pixmap.height()), (100, 100));
        assert_eq!(px(&r, 10, 10), [255, 0, 0, 255]);
        assert_eq!(px(&r, 90, 90), [255, 255, 255, 255]);
    }

    #[test]
    fn places_an_rgba_picture_through_its_matrix_and_counts_text() {
        let mut d = page();
        // 2 × 1 picture: black, white; stretched over the right half of the page
        d.images.push(Image { w: 2, h: 1, data: ImageData::Rgba(vec![0, 0, 0, 255, 255, 255, 255, 255]) });
        d.items.push(Item::Image { image: 0, m: [1270.0, 0.0, 0.0, 2540.0, 1270.0, 0.0], clip: 0, alpha: 1.0, mul: false });
        d.items.push(Item::Text {
            x: 0.0, y: 0.0, angle: 0.0, size: 100.0, sx: 1.0, face: String::new(), weight: 400, italic: false,
            underline: false, strike: false, vertical: false, color: 0, text: "a".into(), xs: vec![], clip: 0,
        });
        let r = render(&d, 100.0).unwrap();
        assert_eq!(px(&r, 25, 50), [255, 255, 255, 255]); // left half untouched
        assert_eq!(px(&r, 55, 50)[0], 0); // first picture pixel: black
        assert_eq!(px(&r, 95, 50)[0], 255); // second: white
        assert_eq!(r.skipped_text, 1);
    }

    #[test]
    fn clips_to_the_intersection() {
        let mut d = page();
        d.clips.push(Clip(vec![XPath::rect(0.0, 0.0, 1270.0, 2540.0), XPath::rect(0.0, 0.0, 2540.0, 1270.0)]));
        d.items.push(Item::Fill { path: XPath::rect(0.0, 0.0, 2540.0, 2540.0), color: 0x000000, evenodd: false, clip: 1, mul: false });
        let r = render(&d, 100.0).unwrap();
        assert_eq!(px(&r, 25, 25)[0], 0);
        assert_eq!(px(&r, 75, 25)[0], 255);
        assert_eq!(px(&r, 25, 75)[0], 255);
    }

    #[test]
    fn refuses_huge_pages() {
        assert!(render(&Display::new(1_000_000.0, 1_000_000.0), 300.0).is_err());
    }
}
