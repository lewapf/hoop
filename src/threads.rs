//! Thread charts: Janome's built in, other brands read from GIMP palette
//! files (.gpl), the format Ink/Stitch and Inkscape use. Hoop can fetch the
//! common brands from the Ink/Stitch project into its own folder; their
//! charts are GPL, so they are downloaded, not shipped. "My threads" marks
//! the spools the owner has, and matching prefers those.

use std::path::{Path, PathBuf};

use crate::formats::{self, Thread};

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub name: String,
    pub threads: Vec<Thread>,
}

/// Charts that can be fetched: (name shown, Ink/Stitch file name).
pub const DOWNLOADS: &[(&str, &str)] = &[
    ("Madeira Rayon", "InkStitch Madeira Rayon.gpl"),
    ("Madeira Polyneon", "InkStitch Madeira Polyneon.gpl"),
    ("Isacord Polyester", "InkStitch Isacord Polyester.gpl"),
    ("Robison-Anton Rayon", "InkStitch Robison-Anton Rayon.gpl"),
    ("Robison-Anton Polyester", "InkStitch Robison-Anton Polyester.gpl"),
    ("Sulky Rayon", "InkStitch Sulky Rayon.gpl"),
    ("Brother Embroidery", "InkStitch Brother Embroidery.gpl"),
    ("Gunold Polyester", "InkStitch Gunold Polyester.gpl"),
    ("Marathon Rayon", "InkStitch Marathon Rayon.gpl"),
    ("Mettler Poly Sheen", "InkStitch Mettler Poly Sheen.gpl"),
    ("Floriani Polyester", "InkStitch Floriani Polyester.gpl"),
    ("Gutermann Creativ Dekor", "InkStitch Gutermann Creativ Dekor.gpl"),
];

const SOURCE: &str = "https://raw.githubusercontent.com/inkstitch/inkstitch/main/palettes/";

pub fn janome() -> Chart {
    Chart { name: t!("Janome").into(), threads: formats::jef_palette().map(|(_, t)| t).collect() }
}

/// Reads a GIMP palette. Each color line is "r g b name… code"; the last
/// word is the thread number when it has a digit in it.
pub fn parse_gpl(text: &str, fallback_name: &str) -> Option<Chart> {
    let mut lines = text.lines();
    if !lines.next()?.trim_start_matches('\u{feff}').starts_with("GIMP Palette") {
        return None;
    }
    let mut name = fallback_name.to_string();
    let mut threads = Vec::new();
    for line in lines {
        let l = line.trim();
        if let Some(n) = l.strip_prefix("Name:") {
            let n = n.trim();
            name = n.strip_prefix("Ink/Stitch:").unwrap_or(n).trim().to_string();
            continue;
        }
        if l.is_empty() || l.starts_with('#') || l.starts_with("Columns:") {
            continue;
        }
        let mut words = l.split_whitespace();
        let rgb: Vec<u8> = words.by_ref().take(3).filter_map(|w| w.parse().ok()).collect();
        if rgb.len() != 3 {
            continue;
        }
        let rest: Vec<&str> = words.collect();
        let (title, code) = match rest.split_last() {
            Some((last, head)) if last.chars().any(|c| c.is_ascii_digit()) && !head.is_empty() => (head.join(" "), last.to_string()),
            _ => (rest.join(" "), String::new()),
        };
        threads.push(Thread { color: [rgb[0], rgb[1], rgb[2]], name: title, code });
    }
    (!threads.is_empty()).then_some(Chart { name, threads })
}

/// Hoop's own chart folder: ~/.local/share/hoop/threads.
pub fn user_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("hoop/threads"))
}

/// Janome first, then charts from Hoop's folder and Inkscape's palettes
/// (where Ink/Stitch installs them), one per name.
pub fn load_all() -> Vec<Chart> {
    let mut charts = vec![janome()];
    let mut dirs: Vec<PathBuf> = user_dir().into_iter().collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".config/inkscape/palettes"));
    }
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "gpl")).collect();
        files.sort();
        for f in files {
            let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let Some(c) = std::fs::read_to_string(&f).ok().and_then(|t| parse_gpl(&t, stem.trim_start_matches("InkStitch "))) else { continue };
            if !charts.iter().any(|k| k.name == c.name) {
                charts.push(c);
            }
        }
    }
    charts
}

/// Fetches one Ink/Stitch chart into Hoop's folder with curl.
pub fn download(file: &str) -> Result<PathBuf, String> {
    let dir = user_dir().ok_or(t!("No home folder."))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(file);
    let url = format!("{SOURCE}{}", file.replace(' ', "%20"));
    let out = std::process::Command::new("curl").args(["-fsSL", "--max-time", "30", "-o"]).arg(&target).arg(&url).output().map_err(|e| t!("Couldn't run curl: {e}", e = e))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&target);
        return Err(t!("Download failed: {v0}", v0 = String::from_utf8_lossy(&out.stderr).trim()));
    }
    let ok = std::fs::read_to_string(&target).ok().and_then(|t| parse_gpl(&t, file)).is_some();
    if !ok {
        let _ = std::fs::remove_file(&target);
        return Err(t!("The download wasn't a thread chart.").into());
    }
    Ok(target)
}

pub fn is_downloaded(file: &str) -> bool {
    user_dir().is_some_and(|d| Path::new(&d).join(file).is_file())
}

/// CIE L*a*b* of an sRGB color (D65).
fn lab(c: [u8; 3]) -> (f32, f32, f32) {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

/// How far apart two colors look (ΔE 1976; under about 3 is hard to tell apart).
pub fn distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (p, q) = (lab(a), lab(b));
    ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2) + (p.2 - q.2).powi(2)).sqrt()
}

/// The chart thread closest to a color. With `own` (thread codes) given,
/// an owned thread is picked unless it looks clearly different, so the
/// list asks for spools already on the shelf.
pub fn nearest<'a>(chart: &'a Chart, color: [u8; 3], own: &[String]) -> Option<&'a Thread> {
    let best = chart.threads.iter().min_by(|a, b| distance(a.color, color).total_cmp(&distance(b.color, color)))?;
    let owned = chart.threads.iter().filter(|t| own.contains(&key(t))).min_by(|a, b| distance(a.color, color).total_cmp(&distance(b.color, color)));
    match owned {
        Some(o) if distance(o.color, color) <= distance(best.color, color) + 6.0 => Some(o),
        _ => Some(best),
    }
}

/// What identifies a thread in "My threads": its code, or its name when it has none.
pub fn key(t: &Thread) -> String {
    if t.code.is_empty() { t.name.clone() } else { t.code.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "GIMP Palette\nName: Ink/Stitch: Madeira Rayon\nColumns: 4\n# RGB Value\t\tColor Name Number\n 40\t 41\t 42\t   Black   1000\n227\t229\t251\t  Bright White   1001\n200 30 40 Red\n";

    #[test]
    fn reads_gimp_palettes_with_names_and_numbers() {
        let c = parse_gpl(SAMPLE, "x").unwrap();
        assert_eq!(c.name, "Madeira Rayon");
        assert_eq!(c.threads.len(), 3);
        assert_eq!(c.threads[1], Thread { color: [227, 229, 251], name: "Bright White".into(), code: "1001".into() });
        assert_eq!(c.threads[2].code, "");
        assert!(parse_gpl("hello", "x").is_none());
    }

    #[test]
    fn prefers_owned_threads_when_close() {
        let c = parse_gpl(SAMPLE, "x").unwrap();
        // Near black: the black spool.
        assert_eq!(nearest(&c, [30, 30, 30], &[]).unwrap().code, "1000");
        // A dark red is nearest the red, even when only black is owned.
        assert_eq!(nearest(&c, [190, 30, 40], &["1000".into()]).unwrap().name, "Red");
        // Off-white, close to both whites: the owned one wins.
        let chart = Chart { name: "t".into(), threads: vec![
            Thread { color: [255, 255, 255], name: "White".into(), code: "1".into() },
            Thread { color: [245, 240, 230], name: "Ivory".into(), code: "2".into() },
        ] };
        assert_eq!(nearest(&chart, [250, 248, 240], &["1".into()]).unwrap().code, "1");
        assert_eq!(nearest(&chart, [245, 240, 230], &[]).unwrap().code, "2");
    }

    #[test]
    fn janome_chart_is_built_in() {
        let j = janome();
        assert!(j.threads.iter().any(|t| t.name == "Cardinal Red" && t.code == "244"));
    }
}
