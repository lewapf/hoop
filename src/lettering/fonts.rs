//! Fonts: the bundled display fonts, fonts downloaded from Google Fonts,
//! installed system fonts, and reading glyph outlines.

use std::path::{Path, PathBuf};

use super::*;

#[derive(Clone)]
pub enum FontData {
    Bundled(&'static [u8]),
    File(PathBuf, u32),
}

/// What a bundled font is for; the font list groups them by it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontGroup {
    Block,
    Serif,
    Script,
    Rounded,
}

impl FontGroup {
    pub const ALL: [FontGroup; 4] = [FontGroup::Block, FontGroup::Serif, FontGroup::Script, FontGroup::Rounded];

    pub fn name(self) -> &'static str {
        match self {
            FontGroup::Block => t!("Block"),
            FontGroup::Serif => t!("Serif, slab and varsity"),
            FontGroup::Script => t!("Script"),
            FontGroup::Rounded => t!("Rounded and fun"),
        }
    }
}

#[derive(Clone)]
pub struct FontSource {
    pub name: String,
    pub featured: bool,
    /// Set for bundled fonts.
    pub group: Option<FontGroup>,
    pub data: FontData,
}

impl FontSource {
    pub fn bytes(&self) -> Result<(std::borrow::Cow<'static, [u8]>, u32), String> {
        match &self.data {
            FontData::Bundled(b) => Ok((std::borrow::Cow::Borrowed(*b), 0)),
            FontData::File(path, index) => {
                std::fs::read(path).map(|b| (std::borrow::Cow::Owned(b), *index)).map_err(|e| t!("Couldn't read {v0}: {e}.", v0 = path.display(), e = e))
            }
        }
    }

    /// Whether it came from Hoop's Google Fonts window.
    pub fn downloaded(&self) -> bool {
        matches!(&self.data, FontData::File(path, _) if download_dir().is_some_and(|d| path.starts_with(d)))
    }
}

macro_rules! bundled {
    ($($name:literal => $file:literal, $group:ident),* $(,)?) => {
        vec![$(FontSource {
            name: $name.into(),
            featured: true,
            group: Some(FontGroup::$group),
            data: FontData::Bundled(include_bytes!(concat!("../../assets/fonts/", $file))),
        }),*]
    };
}

/// Display fonts from Google Fonts (SIL Open Font License or Apache 2.0)
/// that stitch well. The first ones keep their places: projects and tests
/// may refer to them by number.
pub fn bundled_fonts() -> Vec<FontSource> {
    bundled![
        "Alfa Slab One" => "AlfaSlabOne-Regular.ttf", Serif,
        "Anton" => "Anton-Regular.ttf", Block,
        "Bebas Neue" => "BebasNeue-Regular.ttf", Block,
        "Bungee" => "Bungee-Regular.ttf", Rounded,
        "Dancing Script" => "DancingScript[wght].ttf", Script,
        "Lobster" => "Lobster-Regular.ttf", Script,
        "Pacifico" => "Pacifico-Regular.ttf", Script,
        "Righteous" => "Righteous-Regular.ttf", Rounded,
        "Abril Fatface" => "AbrilFatface-Regular.ttf", Serif,
        "Archivo Black" => "ArchivoBlack-Regular.ttf", Block,
        "Black Ops One" => "BlackOpsOne-Regular.ttf", Block,
        "Cinzel Decorative" => "CinzelDecorative-Black.ttf", Serif,
        "Courgette" => "Courgette-Regular.ttf", Script,
        "Graduate" => "Graduate-Regular.ttf", Serif,
        "Kaushan Script" => "KaushanScript-Regular.ttf", Script,
        "Luckiest Guy" => "LuckiestGuy-Regular.ttf", Rounded,
        "Titan One" => "TitanOne-Regular.ttf", Rounded,
        "Ultra" => "Ultra-Regular.ttf", Serif,
        "UnifrakturMaguntia" => "UnifrakturMaguntia-Book.ttf", Serif,
        "Yellowtail" => "Yellowtail-Regular.ttf", Script,
    ]
}

/// Where the Google Fonts window puts fonts: ~/.local/share/fonts/hoop, so
/// other programs find them too.
pub fn download_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("fonts/hoop"))
}

/// A font's name as the font list shows it, "Family Style", or None for
/// files that aren't fonts or have no Latin capital A (icon and symbol
/// fonts).
pub fn face_name(data: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    face.glyph_index('A')?;
    let name_of = |id: u16| face.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string());
    let family = name_of(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| name_of(ttf_parser::name_id::FAMILY))?;
    let style = name_of(ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY).or_else(|| name_of(ttf_parser::name_id::SUBFAMILY));
    Some(match style.as_deref() {
        None | Some("Regular") => family,
        Some(s) => format!("{family} {s}"),
    })
}

/// Whether a font name matches what was typed in a font search: letters
/// and digits only, any case, so "blackops" and "Black-Ops" both find
/// "Black Ops One".
pub fn font_matches(name: &str, query: &str) -> bool {
    let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    key(name).contains(&key(query))
}

/// One font file as a list entry.
pub fn font_file(path: &Path) -> Option<FontSource> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "ttf" | "otf") {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    Some(FontSource { name: face_name(&data)?, featured: false, group: None, data: FontData::File(path.to_path_buf(), 0) })
}

/// Fonts from the Google Fonts window, by name. Quick: there are few.
pub fn downloaded_fonts() -> Vec<FontSource> {
    let Some(dir) = download_dir() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<FontSource> = entries.flatten().filter_map(|e| font_file(&e.path())).collect();
    found.sort_by_key(|a| a.name.to_lowercase());
    found.dedup_by(|a, b| a.name == b.name);
    found
}

/// Installed fonts, one entry per face, named "Family Style", without the
/// downloaded ones. Slow on systems with many fonts, so the app runs it on
/// a background thread.
pub fn system_fonts() -> Vec<FontSource> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = vec![PathBuf::from("/usr/share/fonts"), home.join(".local/share/fonts"), home.join(".fonts")];
    let skip = download_dir();
    let mut found = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                if skip.as_ref() != Some(&path) {
                    dirs.push(path);
                }
                continue;
            }
            found.extend(font_file(&path));
        }
    }
    found.sort_by_key(|a| a.name.to_lowercase());
    found.dedup_by(|a, b| a.name == b.name);
    found
}

/// About how tall, in mm, capital letters of this font can be stitched
/// before thin strokes get too narrow to hold (see `symbols::smallest_mm`),
/// judged on "Hag": a capital, a round and a descender.
pub fn smallest_height_mm(font: &FontSource) -> Option<f32> {
    let s = Settings { text: "Hag".into(), height_mm: 10.0, ..Default::default() };
    let glyphs = outlines(font, &s).ok()?;
    let pts = glyphs.iter().flatten().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts.clone() {
        (x0, y0, x1, y1) = (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1));
    }
    let across = (x1 - x0).max(y1 - y0);
    if !across.is_finite() || across <= 0.0 {
        return None;
    }
    let mut d = String::new();
    for c in glyphs.iter().flatten() {
        for (i, p) in c.iter().enumerate() {
            d += &format!("{}{:.1} {:.1} ", if i == 0 { 'M' } else { 'L' }, p.0 - x0, p.1 - y0);
        }
        d += "Z ";
    }
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.1}" height="{h:.1}"><path d="{d}"/></svg>"#, w = x1 - x0, h = y1 - y0);
    // The sample is `across` tenths of a mm at 10 mm capitals.
    crate::symbols::smallest_across(&svg, 320).map(|mm| (mm * 100.0 / across * 2.0).round() / 2.0)
}

/// Draws one line of text in the font, `px` tall from ascender to
/// descender, as white with coverage in alpha, for font list previews.
pub fn render_line(data: &[u8], text: &str, px: f32) -> Option<egui::ColorImage> {
    use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    let (asc, desc) = (face.ascender() as f32, face.descender() as f32);
    let scale = px / (asc - desc).max(1.0);
    let mut pb = PathBuilder::new();
    let mut x = 0.0f32;
    for ch in text.chars() {
        let Some(g) = face.glyph_index(ch) else { continue };
        let mut o = Outline { contours: Vec::new(), current: Vec::new() };
        face.outline_glyph(g, &mut o);
        ttf_parser::OutlineBuilder::close(&mut o);
        for c in &o.contours {
            for (i, &(gx, gy)) in c.iter().enumerate() {
                let (px_, py) = ((x + gx) * scale, (asc - gy) * scale);
                if i == 0 { pb.move_to(px_, py) } else { pb.line_to(px_, py) }
            }
            pb.close();
        }
        x += face.glyph_hor_advance(g).unwrap_or(0) as f32;
        if x * scale > 1200.0 {
            break;
        }
    }
    let w = ((x * scale).ceil() as u32).clamp(1, 1200);
    let h = px.ceil() as u32;
    let mut pm = Pixmap::new(w, h)?;
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 255, 255, 255);
        paint.anti_alias = true;
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    let rgba: Vec<u8> = pm.pixels().iter().flat_map(|p| [255, 255, 255, p.alpha()]).collect();
    Some(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba))
}

pub(crate) struct Outline {
    pub(crate) contours: Vec<Contour>,
    pub(crate) current: Contour,
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.current.push((x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push((x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = *self.current.last().unwrap_or(&(x, y));
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            let u = 1.0 - t;
            self.current.push((u * u * p0.0 + 2.0 * u * t * x1 + t * t * x, u * u * p0.1 + 2.0 * u * t * y1 + t * t * y));
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = *self.current.last().unwrap_or(&(x, y));
        for i in 1..=10 {
            let t = i as f32 / 10.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.current.push((a * p0.0 + b * x1 + c * x2 + d * x, a * p0.1 + b * y1 + c * y2 + d * y));
        }
    }
    fn close(&mut self) {
        if self.current.len() > 2 {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.current.clear();
    }
}
