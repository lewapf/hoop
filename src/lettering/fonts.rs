//! Fonts: the bundled display fonts, installed system fonts, and reading
//! glyph outlines.

use std::path::PathBuf;

use super::*;

pub enum FontData {
    Bundled(&'static [u8]),
    File(PathBuf, u32),
}

pub struct FontSource {
    pub name: String,
    pub featured: bool,
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
}

macro_rules! bundled {
    ($($name:literal => $file:literal),* $(,)?) => {
        vec![$(FontSource {
            name: $name.into(),
            featured: true,
            data: FontData::Bundled(include_bytes!(concat!("../../assets/fonts/", $file))),
        }),*]
    };
}

/// Display fonts from Google Fonts (SIL Open Font License) that stitch well.
pub fn bundled_fonts() -> Vec<FontSource> {
    bundled![
        "Alfa Slab One" => "AlfaSlabOne-Regular.ttf",
        "Anton" => "Anton-Regular.ttf",
        "Bebas Neue" => "BebasNeue-Regular.ttf",
        "Bungee" => "Bungee-Regular.ttf",
        "Dancing Script" => "DancingScript[wght].ttf",
        "Lobster" => "Lobster-Regular.ttf",
        "Pacifico" => "Pacifico-Regular.ttf",
        "Righteous" => "Righteous-Regular.ttf",
    ]
}

/// Installed fonts, one entry per face, named "Family Style". Slow on systems
/// with many fonts, so the app runs it on a background thread.
pub fn system_fonts() -> Vec<FontSource> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = vec![PathBuf::from("/usr/share/fonts"), home.join(".local/share/fonts"), home.join(".fonts")];
    let mut found = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "ttf" | "otf") {
                continue;
            }
            let Ok(data) = std::fs::read(&path) else { continue };
            let Ok(face) = ttf_parser::Face::parse(&data, 0) else { continue };
            // Skip icon and symbol fonts: they have no Latin capital A.
            if face.glyph_index('A').is_none() {
                continue;
            }
            let name_of = |id: u16| face.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string());
            let family = name_of(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| name_of(ttf_parser::name_id::FAMILY));
            let style = name_of(ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY).or_else(|| name_of(ttf_parser::name_id::SUBFAMILY));
            if let Some(family) = family {
                let name = match style.as_deref() {
                    None | Some("Regular") => family,
                    Some(s) => format!("{family} {s}"),
                };
                found.push(FontSource { name, featured: false, data: FontData::File(path, 0) });
            }
        }
    }
    found.sort_by_key(|a| a.name.to_lowercase());
    found.dedup_by(|a, b| a.name == b.name);
    found
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
