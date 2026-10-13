//! Google Fonts: the catalog of open-licensed families, small preview files
//! and whole fonts, fetched with curl. Previews and the catalog are cached
//! in ~/.cache/hoop/google-fonts; added fonts go to
//! `lettering::download_dir()` with their license next to them.

use std::path::{Path, PathBuf};

const CATALOG: &str = "https://fonts.google.com/metadata/fonts";
const CSS: &str = "https://fonts.googleapis.com/css2";
const LICENSES: &str = "https://raw.githubusercontent.com/google/fonts/main/";

#[derive(Clone, Debug)]
pub struct Family {
    pub name: String,
    /// "Display", "Handwriting", "Serif", "Sans Serif" or "Monospace".
    pub category: String,
    /// Rank: 1 is the most used.
    pub popularity: u32,
    /// "YYYY-MM-DD".
    pub added: String,
    /// Upright weights, lightest first.
    pub weights: Vec<u16>,
    /// Google's 1–10 stroke thickness of the heaviest weight, or a guess
    /// from the weight where Google gives none (most families).
    pub thickness: f32,
}

impl Family {
    /// The weight to offer first: bold when there is one, since heavier
    /// letters stitch smaller, otherwise the regular one.
    pub fn default_weight(&self) -> u16 {
        [700, 400].into_iter().find(|w| self.weights.contains(w)).or(self.weights.last().copied()).unwrap_or(400)
    }
}

pub const CATEGORIES: [&str; 5] = ["Display", "Handwriting", "Serif", "Sans Serif", "Monospace"];

pub fn category_name(c: &str) -> &'static str {
    match c {
        "Display" => t!("Display"),
        "Handwriting" => t!("Handwriting"),
        "Serif" => t!("Serif"),
        "Sans Serif" => t!("Sans serif"),
        "Monospace" => t!("Monospace"),
        _ => t!("Other"),
    }
}

pub fn weight_name(w: u16) -> &'static str {
    match w {
        100 => t!("Thin"),
        200 => t!("Extra light"),
        300 => t!("Light"),
        400 => t!("Regular"),
        500 => t!("Medium"),
        600 => t!("Semibold"),
        700 => t!("Bold"),
        800 => t!("Extra bold"),
        900 => t!("Black"),
        _ => t!("Extra black"),
    }
}

fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("hoop/google-fonts"))
}

fn curl(url: &str) -> Result<Vec<u8>, String> {
    let out = std::process::Command::new("curl").args(["-fsSL", "--max-time", "30", url]).output().map_err(|e| t!("Couldn't run curl: {e}", e = e))?;
    if !out.status.success() {
        return Err(t!("Download failed: {v0}", v0 = String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

/// Percent-encodes text for a URL query.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The families that are open source, not brand fonts, and have Latin
/// letters, most used first. Uses a cached copy younger than a week, or
/// any cached copy when offline.
pub fn catalog() -> Result<Vec<Family>, String> {
    let cached = cache_dir().map(|d| d.join("catalog.json"));
    let fresh = cached.as_ref().and_then(|p| std::fs::metadata(p).ok()?.modified().ok()?.elapsed().ok()).is_some_and(|age| age.as_secs() < 7 * 24 * 3600);
    let text = match cached.as_ref().filter(|_| fresh).and_then(|p| std::fs::read(p).ok()) {
        Some(t) => t,
        None => match curl(CATALOG) {
            Ok(t) => {
                if let Some(p) = &cached {
                    let _ = std::fs::create_dir_all(p.parent().unwrap());
                    let _ = std::fs::write(p, &t);
                }
                t
            }
            Err(e) => cached.and_then(|p| std::fs::read(p).ok()).ok_or(e)?,
        },
    };
    parse_catalog(&text)
}

pub fn parse_catalog(text: &[u8]) -> Result<Vec<Family>, String> {
    // The reply may start with a ")]}'" line against JSON hijacking.
    let start = text.iter().position(|&b| b == b'{').unwrap_or(0);
    let v: serde_json::Value = serde_json::from_slice(&text[start..]).map_err(|_| t!("The font list from Google Fonts couldn't be read.").to_string())?;
    let list = v["familyMetadataList"].as_array().ok_or(t!("The font list from Google Fonts couldn't be read."))?;
    let mut out = Vec::new();
    for f in list {
        let latin = f["subsets"].as_array().is_some_and(|s| s.iter().any(|x| x == "latin"));
        if !latin || f["isOpenSource"] == false || f["isBrandFont"] == true {
            continue;
        }
        let Some(name) = f["family"].as_str() else { continue };
        let mut weights = Vec::new();
        let mut thickness = 0f32;
        if let Some(fonts) = f["fonts"].as_object() {
            for (key, info) in fonts {
                let Ok(w) = key.parse::<u16>() else { continue }; // "400i" is italic
                weights.push(w);
                // Where both are known, 400 is about 5 and 900 about 7.
                let t = info["thickness"].as_f64().map_or(2.5 + w as f32 / 200.0, |t| t as f32);
                thickness = thickness.max(t);
            }
        }
        if weights.is_empty() {
            continue;
        }
        weights.sort();
        out.push(Family {
            name: name.into(),
            category: f["category"].as_str().unwrap_or("").into(),
            popularity: f["popularity"].as_u64().unwrap_or(u64::from(u32::MAX)) as u32,
            added: f["dateAdded"].as_str().unwrap_or("").into(),
            weights,
            thickness,
        });
    }
    out.sort_by_key(|f| f.popularity);
    Ok(out)
}

/// The address of a TrueType file for one weight, asked from the Google
/// Fonts CSS service (which answers curl with TrueType). With `text`, the
/// file holds only those letters.
fn ttf_url(family: &str, weight: u16, text: Option<&str>) -> Result<String, String> {
    let mut url = format!("{CSS}?family={}:wght@{weight}", encode(family));
    if let Some(t) = text {
        url += &format!("&text={}", encode(t));
    }
    let css = String::from_utf8_lossy(&curl(&url)?).into_owned();
    let start = css.find("url(").ok_or(t!("Google Fonts sent no font file."))? + 4;
    let end = css[start..].find(')').ok_or(t!("Google Fonts sent no font file."))? + start;
    Ok(css[start..end].to_string())
}

fn slug(family: &str) -> String {
    family.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// The letters a preview file holds: the family name, "Hag" (which
/// `smallest_height_mm` measures) and any sample text.
pub fn preview_text(family: &str, sample: &str) -> String {
    let mut chars: Vec<char> = format!("{family}Hag{sample}").chars().filter(|c| !c.is_whitespace()).collect();
    chars.sort();
    chars.dedup();
    chars.into_iter().collect()
}

/// A small font file holding just `preview_text`, cached.
pub fn preview(family: &str, weight: u16, sample: &str) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let dir = cache_dir().ok_or(t!("No home folder."))?.join("previews");
    let text = preview_text(family, sample);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    let path = dir.join(format!("{}-{weight}-{:08x}.ttf", slug(family), h.finish() as u32));
    if path.is_file() {
        return Ok(path);
    }
    let bytes = curl(&ttf_url(family, weight, Some(&text))?)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Where an added weight lives.
pub fn installed_path(family: &str, weight: u16) -> Option<PathBuf> {
    crate::lettering::download_dir().map(|d| d.join(format!("{}-{weight}.ttf", slug(family))))
}

pub fn is_installed(family: &str, weight: u16) -> bool {
    installed_path(family, weight).is_some_and(|p| p.is_file())
}

/// Downloads one weight into the fonts folder, with the family's license
/// from the Google Fonts repository beside it.
pub fn install(family: &str, weight: u16) -> Result<PathBuf, String> {
    let path = installed_path(family, weight).ok_or(t!("No home folder."))?;
    let bytes = curl(&ttf_url(family, weight, None)?)?;
    if crate::lettering::face_name(&bytes).is_none() {
        return Err(t!("The download wasn't a font Hoop can use.").into());
    }
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let folder = family.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
    let license = dir.join(format!("{}-LICENSE.txt", slug(family)));
    if !license.is_file() {
        for file in ["ofl/{}/OFL.txt", "apache/{}/LICENSE.txt", "ufl/{}/UFL.txt"] {
            if let Ok(text) = curl(&format!("{LICENSES}{}", file.replace("{}", &folder))) {
                let _ = std::fs::write(&license, text);
                break;
            }
        }
    }
    refresh_fontconfig(dir);
    Ok(path)
}

/// Removes an added weight; the license stays while other weights remain.
pub fn remove(family: &str, weight: u16) -> Result<(), String> {
    let path = installed_path(family, weight).ok_or(t!("No home folder."))?;
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    let dir = path.parent().unwrap();
    let others = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with(&format!("{}-", slug(family))) && e.path().extension().is_some_and(|x| x == "ttf"))
        })
        .unwrap_or(false);
    if !others {
        let _ = std::fs::remove_file(dir.join(format!("{}-LICENSE.txt", slug(family))));
    }
    refresh_fontconfig(dir);
    Ok(())
}

/// Lets other programs see the change at once.
fn refresh_fontconfig(dir: &Path) {
    let _ = std::process::Command::new("fc-cache").arg(dir).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_catalog_and_keeps_open_latin_families() {
        let json = br#")]}'
{"familyMetadataList": [
 {"family": "Kaushan Script", "category": "Handwriting", "popularity": 279, "dateAdded": "2011-01-01",
  "subsets": ["menu", "latin"], "isOpenSource": true, "isBrandFont": false,
  "fonts": {"400": {"thickness": 5}}},
 {"family": "Montserrat", "category": "Sans Serif", "popularity": 7, "dateAdded": "2011-01-01",
  "subsets": ["latin"], "isOpenSource": true, "isBrandFont": false,
  "fonts": {"100": {"thickness": 1}, "400": {"thickness": 6}, "400i": {}, "700": {"thickness": null}, "900": {"thickness": 8}}},
 {"family": "Brand", "category": "Display", "popularity": 1, "subsets": ["latin"], "isOpenSource": true, "isBrandFont": true, "fonts": {"400": {}}},
 {"family": "Noto Sans Thai", "category": "Sans Serif", "popularity": 2, "subsets": ["thai"], "isOpenSource": true, "isBrandFont": false, "fonts": {"400": {}}}
]}"#;
        let fams = parse_catalog(json).unwrap();
        assert_eq!(fams.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Montserrat", "Kaushan Script"]);
        assert_eq!(fams[0].weights, [100, 400, 700, 900]);
        assert_eq!(fams[0].thickness, 8.0);
        assert_eq!(fams[1].thickness, 5.0);
        assert_eq!(fams[0].default_weight(), 700);
        assert_eq!(fams[1].default_weight(), 400);
    }

    #[test]
    fn encodes_family_names_for_urls() {
        assert_eq!(encode("Kaushan Script"), "Kaushan%20Script");
        assert_eq!(encode("Ł&a"), "%C5%81%26a");
        assert_eq!(slug("Kaushan Script"), "KaushanScript");
    }

    /// Fetches a preview and a whole font from Google Fonts (needs the
    /// network): cargo test google_fonts_download -- --ignored
    #[test]
    #[ignore]
    fn google_fonts_download() {
        let tmp = std::env::temp_dir().join("hoop-gfonts-test");
        unsafe {
            std::env::set_var("XDG_CACHE_HOME", tmp.join("cache"));
            std::env::set_var("XDG_DATA_HOME", tmp.join("data"));
        }
        let fams = catalog().unwrap();
        assert!(fams.len() > 1000);
        let p = preview("Kaushan Script", 400, "Ola").unwrap();
        let font = crate::lettering::FontSource { name: "preview".into(), featured: false, group: None, data: crate::lettering::FontData::File(p, 0) };
        assert!(crate::lettering::smallest_height_mm(&font).is_some());
        let path = install("Kaushan Script", 400).unwrap();
        assert!(path.is_file() && path.with_file_name("KaushanScript-LICENSE.txt").is_file());
        remove("Kaushan Script", 400).unwrap();
        assert!(!path.is_file());
        let _ = std::fs::remove_dir_all(tmp);
    }
}
