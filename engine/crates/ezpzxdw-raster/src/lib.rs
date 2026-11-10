//! Draws a page's display list ([`ezpzxdw_core::gfx::Display`]) to pixels, so
//! a page can be looked at without a browser: scanned pages, drawings, lines,
//! pictures and text. Text is drawn with the [`Fonts`] given (none are
//! bundled); without any the runs are left out and counted in
//! [`Raster::skipped_text`] (read them with the text extraction).

mod fonts;
pub use fonts::Fonts;

use ezpzxdw_core::gfx::{Clip, Display, Image, ImageData, Item, Path as XPath, Seg};
use fonts::Shaper;
use tiny_skia::{
    BlendMode, Color, FillRule, IntSize, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, PixmapRef,
    Stroke, StrokeDash, Transform,
};

/// 1/100 mm per inch.
const UNITS_PER_INCH: f32 = 2540.0;

/// The largest page we draw, in pixels (about A0 at 300 dpi).
pub const MAX_PIXELS: u64 = 160_000_000;

pub struct Raster {
    pub pixmap: Pixmap,
    /// Text runs on the page.
    pub text_runs: usize,
    /// Text runs left out (no fonts).
    pub skipped_text: usize,
    /// Characters no font had a glyph for (left blank).
    pub missing_glyphs: usize,
    /// Pictures that could not be decoded.
    pub skipped_images: usize,
}

impl Raster {
    pub fn png(&self) -> Result<Vec<u8>, String> {
        self.pixmap.encode_png().map_err(|e| e.to_string())
    }
}

/// Why a page list was refused.
#[derive(Debug, PartialEq)]
pub enum PagesError {
    /// Not "3", "2-5", "1,4,7-8" or "all".
    Syntax,
    /// Page `.0` (1-based) is past the document's `.1` pages.
    NoPage(usize, usize),
}

/// "3", "2-5", "1,4,7-8" or "all" (1-based; spaces around numbers allowed) as
/// 0-based page indexes, in order, without repeats, at least one.
pub fn parse_pages(spec: &str, count: usize) -> Result<Vec<usize>, PagesError> {
    let ranges: Vec<(usize, usize)> = if spec.trim() == "all" {
        vec![(1, count.max(1))]
    } else {
        let num = |s: &str| s.trim().parse::<usize>().map_err(|_| PagesError::Syntax);
        spec.split(',')
            .map(|part| match part.split_once('-') {
                Some((a, b)) => Ok((num(a)?, num(b)?)),
                None => num(part).map(|n| (n, n)),
            })
            .collect::<Result<_, _>>()?
    };
    let mut pages = Vec::new();
    for (a, b) in ranges {
        if a == 0 || b < a {
            return Err(PagesError::Syntax);
        }
        if b > count {
            return Err(PagesError::NoPage(b, count));
        }
        for p in a - 1..b {
            if !pages.contains(&p) {
                pages.push(p);
            }
        }
    }
    if pages.is_empty() {
        return Err(PagesError::NoPage(1, count));
    }
    Ok(pages)
}

/// Draws `d` at `dpi` on a white page, with text when `fonts` has any (they
/// are read on the first text run).
pub fn render(d: &Display, dpi: f32, fonts: &Fonts) -> Result<Raster, String> {
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

    // built on first use: a page may define clips and pictures it never draws with
    let mut masks: Vec<Option<Option<Mask>>> = vec![None; d.clips.len()];
    let mut pictures: Vec<Option<Option<Picture>>> = d.images.iter().map(|_| None).collect();
    let mut shaper: Option<Shaper> = None;
    let (mut text_runs, mut skipped_text, mut missing_glyphs, mut skipped_images) = (0, 0, 0, 0);

    for item in &d.items {
        let clip = match item {
            Item::Fill { clip, .. } | Item::Stroke { clip, .. } | Item::Text { clip, .. } | Item::Image { clip, .. } => *clip as usize,
        };
        let mask = match masks.get_mut(clip) {
            Some(slot) => slot.get_or_insert_with(|| clip_mask(&d.clips[clip], w as u32, h as u32, page)).as_ref(),
            None => None,
        };
        match item {
            Item::Fill { path, color, evenodd, mul, .. } => {
                let Some(p) = to_path(path) else { continue };
                let mut paint = solid(*color);
                if *mul {
                    paint.blend_mode = BlendMode::Multiply;
                }
                let rule = if *evenodd { FillRule::EvenOdd } else { FillRule::Winding };
                pixmap.fill_path(&p, &paint, rule, page, mask);
            }
            Item::Stroke { path, color, width, dash, cap, join, .. } => {
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
                pixmap.stroke_path(&p, &solid(*color), &stroke, page, mask);
            }
            Item::Text { .. } => {
                text_runs += 1;
                // the first run reads the fonts; with none, every run is left out
                if shaper.is_none() && !fonts.is_empty() {
                    shaper = Some(Shaper::new(fonts));
                }
                match shaper.as_mut() {
                    Some(sh) => missing_glyphs += draw_text(&mut pixmap, sh, item, page, mask),
                    None => skipped_text += 1,
                }
            }
            Item::Image { image, m, alpha, mul, .. } => {
                let k = *image as usize;
                let Some(slot) = pictures.get_mut(k) else { continue };
                let Some(pic) = slot.get_or_insert_with(|| d.images.get(k).and_then(Picture::of)) else {
                    skipped_images += 1;
                    continue;
                };
                let pic = pic.as_ref();
                // pixels → unit square → page (through m) → device
                let unit = Transform::from_scale(1.0 / pic.width() as f32, 1.0 / pic.height() as f32);
                let place = Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
                let paint = PixmapPaint {
                    opacity: alpha.clamp(0.0, 1.0),
                    blend_mode: if *mul { BlendMode::Multiply } else { BlendMode::SourceOver },
                    quality: tiny_skia::FilterQuality::Bilinear,
                };
                pixmap.draw_pixmap(0, 0, pic, &paint, page.pre_concat(place).pre_concat(unit), mask);
            }
        }
    }
    Ok(Raster { pixmap, text_runs, skipped_text, missing_glyphs, skipped_images })
}

/// Draws one text run; returns how many characters had no glyph.
fn draw_text(pixmap: &mut Pixmap, shaper: &mut Shaper, item: &Item, page: Transform, mask: Option<&Mask>) -> usize {
    let Item::Text { x, y, angle, size, sx, face, weight, italic, underline, strike, vertical, color, text, xs, .. } = item else {
        return 0;
    };
    let by_xs = xs.len() == text.chars().count();
    let run = page.pre_translate(*x, *y).pre_rotate(-*angle);
    let paint = solid(*color);
    let (mut pen, mut missing, mut end) = (0.0f32, 0usize, 0.0f32);
    for (i, c) in text.chars().enumerate() {
        if c.is_control() {
            continue;
        }
        let Some(g) = shaper.glyph(face, c) else {
            missing += 1;
            pen += size * 0.5;
            continue;
        };
        let at = if by_xs { xs[i] } else { pen };
        let adv = g.advance * size;
        if let Some(p) = &g.path {
            // em units (y up) → run space: scale by the size and flip y.
            // Vertical runs stack downward, each glyph centred on the line.
            let mut t = if *vertical { run.pre_translate(-adv * 0.5, at + size * 0.88) } else { run.pre_translate(at, 0.0) };
            t = t.pre_scale(size * if *vertical { 1.0 } else { *sx }, -*size);
            if *italic && !g.italic {
                t = t.pre_concat(Transform::from_row(1.0, 0.0, 0.2, 1.0, 0.0, 0.0));
            }
            pixmap.fill_path(p, &paint, FillRule::Winding, t, mask);
            if *weight >= 600 && !g.bold {
                // synthetic bold: a thin stroke around the outline (em units)
                let stroke = Stroke { width: 0.035, ..Stroke::default() };
                pixmap.stroke_path(p, &paint, &stroke, t, mask);
            }
        }
        pen += if *vertical { *size } else { adv * *sx };
        if !*vertical {
            end = end.max(at + adv * *sx);
        }
    }
    if (*underline || *strike) && !*vertical && end > 0.0 {
        let mut line = |dy: f32| {
            let mut b = PathBuilder::new();
            b.move_to(0.0, dy);
            b.line_to(end, dy);
            if let Some(p) = b.finish() {
                let stroke = Stroke { width: (size * 0.06).max(1.0), ..Stroke::default() };
                pixmap.stroke_path(&p, &paint, &stroke, run, mask);
            }
        };
        if *underline {
            line(size * 0.12);
        }
        if *strike {
            line(-size * 0.3);
        }
    }
    missing
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

/// A picture ready to draw. Opaque RGBA (scans, bitmap pages) is drawn from
/// the display list's own pixels: premultiplied and straight alpha agree at 255.
enum Picture<'a> {
    Borrowed(PixmapRef<'a>),
    Owned(Pixmap),
}

impl<'a> Picture<'a> {
    fn of(im: &'a Image) -> Option<Picture<'a>> {
        let (w, h) = (im.w, im.h);
        let n = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
        let rgba = match &im.data {
            ImageData::Rgba(px) if px.len() == n && px.chunks_exact(4).all(|p| p[3] == 255) => {
                return PixmapRef::from_bytes(px, w, h).map(Picture::Borrowed);
            }
            ImageData::Rgba(px) => {
                let mut pm = px.clone();
                // tiny-skia wants premultiplied alpha
                for p in pm.chunks_exact_mut(4) {
                    let a = p[3] as u16;
                    for c in &mut p[..3] {
                        *c = ((*c as u16 * a + 127) / 255) as u8;
                    }
                }
                pm
            }
            ImageData::Jpeg(bytes) => jpeg_rgba(bytes, w, h)?,
        };
        if rgba.len() != n {
            return None;
        }
        Pixmap::from_vec(rgba, IntSize::from_wh(w, h)?).map(Picture::Owned)
    }

    fn as_ref(&self) -> PixmapRef<'_> {
        match self {
            Picture::Borrowed(p) => *p,
            Picture::Owned(p) => p.as_ref(),
        }
    }
}

/// A JPEG of `w` × `h` as opaque RGBA.
fn jpeg_rgba(bytes: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut dec = jpeg_decoder::Decoder::new(bytes);
    let px = dec.decode().ok()?;
    let info = dec.info()?;
    if (info.width as u32, info.height as u32) != (w, h) {
        return None;
    }
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => px.iter().for_each(|&g| out.extend_from_slice(&[g, g, g, 255])),
        jpeg_decoder::PixelFormat::L16 => px.chunks_exact(2).for_each(|g| out.extend_from_slice(&[g[0], g[0], g[0], 255])),
        jpeg_decoder::PixelFormat::RGB24 => px.chunks_exact(3).for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255])),
        jpeg_decoder::PixelFormat::CMYK32 => px.chunks_exact(4).for_each(|p| {
            // Adobe JPEGs store CMYK inverted
            let k = p[3] as u16;
            let ch = |c: u8| ((c as u16 * k) / 255) as u8;
            out.extend_from_slice(&[ch(p[0]), ch(p[1]), ch(p[2]), 255]);
        }),
    }
    Some(out)
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

    fn no_fonts() -> Fonts {
        Fonts::default()
    }

    #[test]
    fn draws_a_filled_rect_at_dpi() {
        let mut d = page();
        d.items.push(Item::Fill { path: XPath::rect(0.0, 0.0, 1270.0, 1270.0), color: 0xff0000, evenodd: false, clip: 0, mul: false });
        let r = render(&d, 100.0, &no_fonts()).unwrap();
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
        let r = render(&d, 100.0, &no_fonts()).unwrap();
        assert_eq!(px(&r, 25, 50), [255, 255, 255, 255]); // left half untouched
        assert_eq!(px(&r, 55, 50)[0], 0); // first picture pixel: black
        assert_eq!(px(&r, 95, 50)[0], 255); // second: white
        assert_eq!((r.text_runs, r.skipped_text), (1, 1));
    }

    #[test]
    fn premultiplies_translucent_pictures() {
        let mut d = page();
        // one black pixel at half alpha over white: mid grey
        d.images.push(Image { w: 1, h: 1, data: ImageData::Rgba(vec![0, 0, 0, 128]) });
        d.items.push(Item::Image { image: 0, m: [2540.0, 0.0, 0.0, 2540.0, 0.0, 0.0], clip: 0, alpha: 1.0, mul: false });
        let g = px(&render(&d, 10.0, &no_fonts()).unwrap(), 5, 5)[0];
        assert!((120..=135).contains(&g), "{g}");
    }

    #[test]
    fn draws_text_when_a_font_is_there() {
        let f = Fonts::runner();
        if f.is_empty() {
            return; // no fonts on this machine
        }
        let mut d = page();
        d.items.push(Item::Text {
            x: 200.0, y: 1500.0, angle: 0.0, size: 1000.0, sx: 1.0, face: "MS Mincho".into(), weight: 400, italic: false,
            underline: false, strike: false, vertical: false, color: 0, text: "HIH".into(), xs: vec![], clip: 0,
        });
        let r = render(&d, 100.0, &f).unwrap();
        assert_eq!((r.skipped_text, r.missing_glyphs), (0, 0));
        let dark = (0..100).flat_map(|y| (0..100).map(move |x| (x, y))).filter(|&(x, y)| px(&r, x, y)[0] < 128).count();
        assert!(dark > 50, "dark pixels: {dark}");
        assert_eq!(px(&r, 5, 2), [255, 255, 255, 255]); // above the em box
    }

    #[test]
    fn names_meet_across_width_and_language() {
        assert_eq!(fonts::normalize("ＭＳ Ｐ明朝"), fonts::normalize("MS PMincho"));
        assert_eq!(fonts::normalize("@ＭＳ ゴシック"), "msgothic");
    }

    #[test]
    fn clips_to_the_intersection() {
        let mut d = page();
        d.clips.push(Clip(vec![XPath::rect(0.0, 0.0, 1270.0, 2540.0), XPath::rect(0.0, 0.0, 2540.0, 1270.0)]));
        d.items.push(Item::Fill { path: XPath::rect(0.0, 0.0, 2540.0, 2540.0), color: 0x000000, evenodd: false, clip: 1, mul: false });
        let r = render(&d, 100.0, &no_fonts()).unwrap();
        assert_eq!(px(&r, 25, 25)[0], 0);
        assert_eq!(px(&r, 75, 25)[0], 255);
        assert_eq!(px(&r, 25, 75)[0], 255);
    }

    #[test]
    fn refuses_huge_pages() {
        assert!(render(&Display::new(1_000_000.0, 1_000_000.0), 300.0, &no_fonts()).is_err());
    }

    #[test]
    fn parses_page_lists() {
        assert_eq!(parse_pages("all", 3), Ok(vec![0, 1, 2]));
        assert_eq!(parse_pages("2-3, 1,2", 5), Ok(vec![1, 2, 0]));
        assert_eq!(parse_pages("4", 3), Err(PagesError::NoPage(4, 3)));
        assert_eq!(parse_pages("all", 0), Err(PagesError::NoPage(1, 0)));
        for bad in ["0", "3-2", "a", "", "1,,2"] {
            assert_eq!(parse_pages(bad, 3), Err(PagesError::Syntax), "{bad}");
        }
    }
}
