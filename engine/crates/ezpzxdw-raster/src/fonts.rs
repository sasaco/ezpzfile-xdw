//! Fonts for drawing text: TrueType/OpenType files (and collections) read with
//! `ttf-parser`, matched by the face name a page asks for. Nothing is bundled:
//! the caller adds font data, or [`Fonts::system`] looks for a short list of
//! well-known files (MS Mincho/Gothic, Meiryo, Yu, Noto CJK, Arial, ...).

use std::sync::Arc;
use tiny_skia::{Path, PathBuilder};
use ttf_parser::{name_id, Face, OutlineBuilder};

struct Entry {
    data: Arc<Vec<u8>>,
    index: u32,
    /// Normalized family and full names.
    names: Vec<String>,
}

#[derive(Default)]
pub struct Fonts {
    entries: Vec<Entry>,
}

/// Fullwidth to halfwidth, lowercase, no spaces/punctuation, Japanese
/// generic words in Latin ("ＭＳ Ｐ明朝" and "MS PMincho" meet).
pub fn normalize(s: &str) -> String {
    let mut o = String::new();
    for c in s.trim_start_matches('@').chars() {
        let c = match c as u32 {
            0xff01..=0xff5e => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            _ => c,
        };
        if c.is_alphanumeric() {
            o.extend(c.to_lowercase());
        }
    }
    o.replace("明朝", "mincho").replace("ゴシック", "gothic")
}

impl Fonts {
    pub fn new() -> Fonts {
        Fonts::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds every face of a font file (a collection adds all its fonts).
    /// Returns how many faces were added.
    pub fn add_data(&mut self, data: Vec<u8>) -> usize {
        let data = Arc::new(data);
        let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        let mut added = 0;
        for index in 0..n {
            let Ok(face) = Face::parse(&data, index) else { continue };
            let mut names = Vec::new();
            for nm in face.names() {
                if matches!(nm.name_id, name_id::FAMILY | name_id::FULL_NAME | name_id::TYPOGRAPHIC_FAMILY | name_id::POST_SCRIPT_NAME) {
                    if let Some(s) = nm.to_string() {
                        let s = normalize(&s);
                        if !s.is_empty() && !names.contains(&s) {
                            names.push(s);
                        }
                    }
                }
            }
            self.entries.push(Entry { data: data.clone(), index, names });
            added += 1;
        }
        added
    }

    /// Reads a font file; false when it cannot be read or holds no font.
    pub fn add_file(&mut self, path: &std::path::Path) -> bool {
        match std::fs::metadata(path) {
            Ok(m) if m.is_file() && m.len() <= 64 << 20 => {}
            _ => return false,
        }
        std::fs::read(path).map(|d| self.add_data(d) > 0).unwrap_or(false)
    }

    /// Adds every font file directly in `dir`.
    pub fn add_dir(&mut self, dir: &std::path::Path) -> usize {
        let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        files.sort();
        files
            .iter()
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("ttf" | "ttc" | "otf")))
            .filter(|p| self.add_file(p))
            .count()
    }

    /// A short list of well-known fonts of this PC (Japanese first), plus
    /// the folders in `EZPZXDW_FONTS` (separated like PATH).
    pub fn system() -> Fonts {
        let mut f = Fonts::new();
        if let Some(v) = std::env::var_os("EZPZXDW_FONTS") {
            for d in std::env::split_paths(&v) {
                if d.is_dir() {
                    f.add_dir(&d);
                } else {
                    f.add_file(&d);
                }
            }
        }
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();
        if let Some(w) = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")) {
            dirs.push(std::path::Path::new(&w).join("Fonts"));
        }
        for d in ["/usr/share/fonts", "/usr/share/fonts/truetype", "/usr/share/fonts/opentype", "/Library/Fonts", "/System/Library/Fonts"] {
            dirs.push(d.into());
        }
        const WANTED: &[&str] = &[
            "msmincho.ttc", "msgothic.ttc", "meiryo.ttc", "meiryob.ttc", "yumin.ttf", "yugothr.ttc", "yugothb.ttc", "arial.ttf",
            "arialbd.ttf", "times.ttf", "timesbd.ttf", "timesi.ttf", "calibri.ttf", "cour.ttf", "symbol.ttf",
        ];
        for d in &dirs {
            for w in WANTED {
                // Windows file names are case-insensitive; elsewhere try as written
                f.add_file(&d.join(w));
            }
            if !cfg!(windows) {
                for sub in ["noto", "opentype/noto", "truetype/noto", "truetype/dejavu", "truetype/liberation", "ipa", "ipafont"] {
                    f.add_dir(&d.join(sub));
                }
            }
        }
        f
    }

    /// Face names (normalized) for diagnostics.
    pub fn names(&self) -> Vec<String> {
        self.entries.iter().filter_map(|e| e.names.first().cloned()).collect()
    }

    /// Faces to try for `face`, best first: those named so, then all others
    /// (so a character the named font lacks can come from another).
    pub(crate) fn candidates(&self, face: &str) -> Vec<usize> {
        let want = normalize(face);
        let mut best: Vec<usize> = Vec::new();
        if !want.is_empty() {
            for (i, e) in self.entries.iter().enumerate() {
                if e.names.iter().any(|n| *n == want) {
                    best.push(i);
                }
            }
            if best.is_empty() {
                // "msmincho" for "mspmincho" and the like: same style word
                let style = if want.contains("mincho") || want.contains("serif") || want.contains("times") { "mincho" } else { "gothic" };
                for (i, e) in self.entries.iter().enumerate() {
                    if e.names.iter().any(|n| n.contains(style)) {
                        best.push(i);
                    }
                }
            }
        }
        for i in 0..self.entries.len() {
            if !best.contains(&i) {
                best.push(i);
            }
        }
        best
    }

    pub(crate) fn face(&self, i: usize) -> Option<Face<'_>> {
        let e = self.entries.get(i)?;
        Face::parse(&e.data, e.index).ok()
    }
}

/// A glyph's outline in em units (y up), and its advance.
pub(crate) struct Glyph {
    pub path: Option<Path>,
    pub advance: f32,
}

struct Pb(PathBuilder, f32);
impl OutlineBuilder for Pb {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x * self.1, y * self.1)
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x * self.1, y * self.1)
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1 * self.1, y1 * self.1, x * self.1, y * self.1)
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1 * self.1, y1 * self.1, x2 * self.1, y2 * self.1, x * self.1, y * self.1)
    }
    fn close(&mut self) {
        self.0.close()
    }
}

/// The glyph of `c` in `face`, if the font has one.
pub(crate) fn glyph(face: &Face, c: char) -> Option<Glyph> {
    let id = face.glyph_index(c)?;
    let upem = face.units_per_em().max(1) as f32;
    let k = 1.0 / upem;
    let mut pb = Pb(PathBuilder::new(), k);
    let has = face.outline_glyph(id, &mut pb).is_some();
    let advance = face.glyph_hor_advance(id).map(|a| a as f32 * k).unwrap_or(0.5);
    Some(Glyph { path: if has { pb.0.finish() } else { None }, advance })
}
