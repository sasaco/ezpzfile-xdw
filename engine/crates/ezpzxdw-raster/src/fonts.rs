//! Fonts for drawing text: TrueType/OpenType files (and collections) read with
//! `ttf-parser`, matched by the face name a page asks for. Nothing is bundled:
//! [`Fonts::runner`] lists where to look, and the files are read only when the
//! first text run is drawn (a scanned page never reads them).

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use tiny_skia::{Path, PathBuilder};
use ttf_parser::{name_id, Face, OutlineBuilder};

struct Entry {
    data: Arc<Vec<u8>>,
    index: u32,
    /// Normalized family and full names.
    names: Vec<String>,
}

enum Source {
    Data(Arc<Vec<u8>>),
    File(PathBuf),
    Dir(PathBuf),
}

/// Where fonts come from, read on first use. `Fonts::default()` has none: text is not drawn.
#[derive(Default)]
pub struct Fonts {
    sources: Vec<Source>,
    entries: OnceLock<Vec<Entry>>,
}

/// Font files above this size are not read.
const MAX_FONT_FILE: u64 = 64 << 20;

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

/// Serif faces fall back to Mincho, the rest to Gothic. The same words as the
/// web editor's face list (web/editor/app.js).
fn is_serif(normalized: &str) -> bool {
    ["mincho", "serif", "times", "century"].iter().any(|w| normalized.contains(w))
}

impl Fonts {
    /// Fonts given as data (always read).
    pub fn add_data(&mut self, data: Vec<u8>) {
        self.sources.push(Source::Data(Arc::new(data)));
    }

    /// A font file, or every font file directly in a folder.
    pub fn add_path(&mut self, path: impl Into<PathBuf>) {
        let p = path.into();
        self.sources.push(if p.is_dir() { Source::Dir(p) } else { Source::File(p) });
    }

    /// Where a runner looks, in order: the `fonts` folder beside the program
    /// (what an installer ships), the paths in `EZPZXDW_FONTS` (separated like
    /// PATH), then a short list of well-known fonts of this PC (Japanese first).
    /// A file name found twice is read once.
    pub fn runner() -> Fonts {
        let mut f = Fonts::default();
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("fonts"))) {
            f.add_path(dir);
        }
        if let Some(v) = std::env::var_os("EZPZXDW_FONTS") {
            for p in std::env::split_paths(&v) {
                f.add_path(p);
            }
        }
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(w) = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")) {
            dirs.push(FsPath::new(&w).join("Fonts"));
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
                f.sources.push(Source::File(d.join(w)));
            }
            if !cfg!(windows) {
                for sub in ["noto", "opentype/noto", "truetype/noto", "truetype/dejavu", "truetype/liberation", "ipa", "ipafont"] {
                    f.sources.push(Source::Dir(d.join(sub)));
                }
            }
        }
        f
    }

    /// No fonts at all (reads the sources on first call).
    pub fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }

    fn entries(&self) -> &[Entry] {
        self.entries.get_or_init(|| {
            let (mut entries, mut seen) = (Vec::new(), Vec::<String>::new());
            let mut file = |p: &FsPath, entries: &mut Vec<Entry>| {
                let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                if seen.contains(&name) || !std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX_FONT_FILE) {
                    return;
                }
                if let Ok(data) = std::fs::read(p) {
                    if add_faces(entries, Arc::new(data)) > 0 {
                        seen.push(name);
                    }
                }
            };
            for s in &self.sources {
                match s {
                    Source::Data(d) => {
                        add_faces(&mut entries, d.clone());
                    }
                    Source::File(p) => file(p, &mut entries),
                    Source::Dir(d) => {
                        let Ok(rd) = std::fs::read_dir(d) else { continue };
                        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                        files.sort();
                        for p in files.iter().filter(|p| {
                            matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("ttf" | "ttc" | "otf"))
                        }) {
                            file(p, &mut entries);
                        }
                    }
                }
            }
            entries
        })
    }

    /// Faces to try for `face`, best first: those named so (or of the same
    /// style), then all others, so a character the named font lacks can come
    /// from another.
    fn candidates(&self, face: &str) -> Vec<usize> {
        let entries = self.entries();
        let want = normalize(face);
        let mut taken = vec![false; entries.len()];
        let mut best = Vec::with_capacity(entries.len());
        let mut take = |pred: &dyn Fn(&Entry) -> bool, best: &mut Vec<usize>| {
            for (i, e) in entries.iter().enumerate() {
                if !taken[i] && pred(e) {
                    taken[i] = true;
                    best.push(i);
                }
            }
        };
        if !want.is_empty() {
            take(&|e| e.names.iter().any(|n| *n == want), &mut best);
            if best.is_empty() {
                // "msmincho" for "mspmincho" and the like: same style word
                let style = if is_serif(&want) { "mincho" } else { "gothic" };
                take(&|e| e.names.iter().any(|n| n.contains(style)), &mut best);
            }
        }
        take(&|_| true, &mut best);
        best
    }
}

/// Adds every face of a font file (a collection adds all its fonts); how many.
fn add_faces(entries: &mut Vec<Entry>, data: Arc<Vec<u8>>) -> usize {
    let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    let before = entries.len();
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
        entries.push(Entry { data: data.clone(), index, names });
    }
    entries.len() - before
}

/// A glyph's outline in em units (y up), its advance, and whether its face is
/// bold / italic already (so no synthetic style is added).
pub(crate) struct Glyph {
    pub path: Option<Path>,
    pub advance: f32,
    pub bold: bool,
    pub italic: bool,
}

/// Glyph lookup for one page: each face is parsed once, each (face, char) and
/// each face name is resolved once.
pub(crate) struct Shaper<'f> {
    fonts: &'f Fonts,
    faces: Vec<Option<Option<Face<'f>>>>,
    glyphs: HashMap<(usize, char), Option<Rc<Glyph>>>,
    candidates: HashMap<String, Rc<Vec<usize>>>,
}

impl<'f> Shaper<'f> {
    pub fn new(fonts: &'f Fonts) -> Shaper<'f> {
        Shaper { fonts, faces: Vec::new(), glyphs: HashMap::new(), candidates: HashMap::new() }
    }

    /// The glyph for `c`, from the faces `face_name` asks for first.
    pub fn glyph(&mut self, face_name: &str, c: char) -> Option<Rc<Glyph>> {
        let cands = match self.candidates.get(face_name) {
            Some(c) => c.clone(),
            None => {
                let c = Rc::new(self.fonts.candidates(face_name));
                self.candidates.insert(face_name.to_string(), c.clone());
                c
            }
        };
        cands.iter().find_map(|&k| self.glyph_in(k, c))
    }

    fn glyph_in(&mut self, k: usize, c: char) -> Option<Rc<Glyph>> {
        if let Some(g) = self.glyphs.get(&(k, c)) {
            return g.clone();
        }
        let entries = self.fonts.entries();
        if self.faces.len() < entries.len() {
            self.faces.resize_with(entries.len(), || None);
        }
        let face = self.faces[k].get_or_insert_with(|| Face::parse(&entries[k].data, entries[k].index).ok());
        let g = face.as_ref().and_then(|f| glyph(f, c)).map(Rc::new);
        self.glyphs.insert((k, c), g.clone());
        g
    }
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
fn glyph(face: &Face, c: char) -> Option<Glyph> {
    let id = face.glyph_index(c)?;
    let k = 1.0 / face.units_per_em().max(1) as f32;
    let mut pb = Pb(PathBuilder::new(), k);
    let has = face.outline_glyph(id, &mut pb).is_some();
    let advance = face.glyph_hor_advance(id).map(|a| a as f32 * k).unwrap_or(0.5);
    Some(Glyph { path: if has { pb.0.finish() } else { None }, advance, bold: face.is_bold(), italic: face.is_italic() })
}
