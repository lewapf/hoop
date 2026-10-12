//! Remembers settings between sessions in ~/.local/state/hoop/prefs as plain
//! `key = value` lines, so a damaged or old file only loses the bad lines.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::lettering::{Align, EdgeKind, FillKind, PatchShape, Settings};
use crate::testsew::Tuning;

/// Prefs keys for the test sheet results, in the order of `Tuning::answers`.
const TUNING_KEYS: [&str; 5] = ["tune.satin_max", "tune.fill_min", "tune.rows", "tune.pull", "tune.letters_min"];

pub struct Prefs {
    file: PathBuf,
    values: HashMap<String, String>,
}

impl Prefs {
    pub fn load() -> Self {
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .unwrap_or_default();
        Prefs::at(state.join("hoop/prefs"))
    }

    pub fn at(file: PathBuf) -> Self {
        let mut values = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(&file) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once(" = ") {
                    values.insert(k.to_string(), unescape(v));
                }
            }
        }
        Prefs { file, values }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    pub fn num<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key)?.parse().ok()
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.to_string(), value.to_string());
    }

    /// Writes beside the file and renames, so a crash never leaves half a file.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut keys: Vec<&String> = self.values.keys().collect();
        keys.sort();
        let text: String = keys.into_iter().map(|k| format!("{k} = {}\n", escape(&self.values[k]))).collect();
        let tmp = self.file.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.file)
    }

    /// Forgets everything but the recent files.
    pub fn reset_keep_recent(&mut self) {
        self.values.retain(|k, _| k == "recent");
    }

    pub fn recent(&self) -> Vec<PathBuf> {
        self.get("recent").map(|r| r.split('\n').filter(|s| !s.is_empty()).map(PathBuf::from).collect()).unwrap_or_default()
    }

    pub fn add_recent(&mut self, path: &std::path::Path) {
        let path = &path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut list = self.recent();
        list.retain(|p| p != path);
        list.insert(0, path.to_path_buf());
        list.truncate(8);
        let joined: Vec<String> = list.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        self.set("recent", joined.join("\n"));
    }

    /// Results of the test sheet, one key per row; missing keys are rows
    /// not sewn yet.
    pub fn tuning(&self) -> Tuning {
        let mut t = Tuning::default();
        for (key, v) in TUNING_KEYS.iter().zip(t.answers()) {
            *v = self.num::<f32>(key).filter(|v| v.is_finite() && *v >= 0.0);
        }
        t
    }

    pub fn store_tuning(&mut self, t: &Tuning) {
        let mut t = *t;
        for (key, v) in TUNING_KEYS.iter().zip(t.answers()) {
            match v {
                Some(v) => self.set(key, v),
                None => {
                    self.values.remove(*key);
                }
            }
        }
    }

    pub fn store_lettering(&mut self, s: &Settings, font_name: &str) {
        self.set("letter.text", &s.text);
        self.set("letter.font", font_name);
        for (k, v) in [
            ("height", s.height_mm), ("spacing", s.spacing_mm), ("lines", s.line_spacing), ("arc", s.arc_deg), ("circle", s.circle_mm), ("envelope", s.envelope_mm),
            ("angle", s.fill_angle), ("rows", s.density_mm), ("stitch", s.stitch_mm), ("swing", s.wave_amp_mm),
            ("wave", s.wave_len_mm), ("fade", s.fade_mm), ("pull", s.pull_mm), ("slant", s.slant_deg), ("edge_width", s.edge_width_mm),
            ("patch_pad", s.patch_pad_mm), ("patch_border", s.patch_border_mm),
        ] {
            self.set(&format!("letter.{k}"), v);
        }
        self.set("letter.shape", format!("{:?}", s.shape));
        self.set("letter.fill", format!("{:?}", s.fill));
        self.set("letter.edge", format!("{:?}", s.edge));
        self.set("letter.align", format!("{:?}", s.align));
        self.set("letter.underlay", s.underlay);
        self.set("letter.underlay_kind", format!("{:?}", s.underlay_kind));
        self.set("letter.fill_thread", s.fill_thread);
        self.set("letter.edge_thread", s.edge_thread);
        self.set("letter.patch", format!("{:?}", s.patch));
        self.set("letter.patch_fill", format!("{:?}", s.patch_fill));
        self.set("letter.patch_thread", s.patch_thread);
        self.set("letter.patch_border_thread", s.patch_border_thread);
        self.set("letter.patch_applique", s.patch_applique);
        self.set("letter.patch_ith", s.patch_ith);
    }

    /// Lettering settings from the last session; the font is looked up by
    /// name by the caller, since font numbers change as fonts are installed.
    pub fn lettering(&self) -> Settings {
        let mut s = Settings::default();
        if let Some(t) = self.get("letter.text") {
            s.text = t.to_string();
        }
        let f = |k: &str, v: &mut f32| {
            if let Some(n) = self.num::<f32>(&format!("letter.{k}")).filter(|n| n.is_finite()) {
                *v = n;
            }
        };
        f("height", &mut s.height_mm);
        f("spacing", &mut s.spacing_mm);
        f("lines", &mut s.line_spacing);
        f("arc", &mut s.arc_deg);
        f("circle", &mut s.circle_mm);
        f("envelope", &mut s.envelope_mm);
        s.shape = match self.get("letter.shape") {
            Some(v) => crate::lettering::TextShape::ALL.into_iter().find(|t| format!("{t:?}") == v).unwrap_or(crate::lettering::TextShape::Straight),
            // Older settings only had the arc.
            None if s.arc_deg.abs() > 0.01 => crate::lettering::TextShape::Arc,
            None => crate::lettering::TextShape::Straight,
        };
        f("angle", &mut s.fill_angle);
        f("rows", &mut s.density_mm);
        f("stitch", &mut s.stitch_mm);
        f("swing", &mut s.wave_amp_mm);
        f("wave", &mut s.wave_len_mm);
        f("fade", &mut s.fade_mm);
        f("pull", &mut s.pull_mm);
        f("slant", &mut s.slant_deg);
        f("edge_width", &mut s.edge_width_mm);
        f("patch_pad", &mut s.patch_pad_mm);
        f("patch_border", &mut s.patch_border_mm);
        s.patch = match self.get("letter.patch") {
            Some("Rect") => PatchShape::Rect,
            Some("Rounded") => PatchShape::Rounded,
            Some("Oval") => PatchShape::Oval,
            Some("Circle") => PatchShape::Circle,
            Some("Fob") => PatchShape::Fob,
            _ => PatchShape::None,
        };
        if let Some(v) = self.get("letter.patch_fill").and_then(fill_kind) {
            s.patch_fill = v;
        }
        if let Some(v) = self.num("letter.patch_thread") {
            s.patch_thread = v;
        }
        if let Some(v) = self.num("letter.patch_border_thread") {
            s.patch_border_thread = v;
        }
        if let Some(v) = self.num("letter.patch_ith") {
            s.patch_ith = v;
        }
        if let Some(v) = self.num("letter.patch_applique") {
            s.patch_applique = v;
        }
        if let Some(v) = self.get("letter.fill").and_then(fill_kind) {
            s.fill = v;
        }
        if let Some(v) = self.get("letter.edge").and_then(edge_kind) {
            s.edge = v;
        }
        s.align = match self.get("letter.align") {
            Some("Left") => Align::Left,
            Some("Right") => Align::Right,
            _ => Align::Center,
        };
        if let Some(k) = self.get("letter.underlay_kind").and_then(|v| crate::lettering::Underlay::ALL.into_iter().find(|u| format!("{u:?}") == v)) {
            s.underlay_kind = k;
        }
        if let Some(v) = self.num("letter.underlay") {
            s.underlay = v;
        }
        if let Some(v) = self.num("letter.fill_thread") {
            s.fill_thread = v;
        }
        if let Some(v) = self.num("letter.edge_thread") {
            s.edge_thread = v;
        }
        s
    }
}

pub fn fill_kind(v: &str) -> Option<FillKind> {
    Some(match v {
        "None" => FillKind::None,
        "Tatami" => FillKind::Tatami,
        "Satin" => FillKind::Satin,
        "Wave" => FillKind::Wave,
        "Zigzag" => FillKind::Zigzag,
        "Crosshatch" => FillKind::Crosshatch,
        "Gradient" => FillKind::Gradient,
        "Open" => FillKind::Open,
        "Contour" => FillKind::Contour,
        "Column" => FillKind::Column,
        "Stipple" => FillKind::Stipple,
        "Cross" => FillKind::Cross,
        "Auto" => FillKind::Auto,
        _ => return None,
    })
}

pub fn edge_kind(v: &str) -> Option<EdgeKind> {
    Some(match v {
        "None" => EdgeKind::None,
        "Running" => EdgeKind::Running,
        "Triple" => EdgeKind::Triple,
        "Blanket" => EdgeKind::Blanket,
        "Scallop" => EdgeKind::Scallop,
        "Satin" => EdgeKind::Satin,
        _ => return None,
    })
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(v: &str) -> String {
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sheet_results_survive_a_round_trip() {
        let file = std::env::temp_dir().join(format!("hoop-tune-{}", std::process::id()));
        let mut p = Prefs::at(file.clone());
        let t = Tuning { pull: Some(0.2), letters_min: Some(7.0), ..Default::default() };
        p.store_tuning(&t);
        p.save().unwrap();
        assert_eq!(Prefs::at(file.clone()).tuning(), t);
        // Clearing a row forgets it.
        p.store_tuning(&Tuning { pull: Some(0.2), ..Default::default() });
        p.save().unwrap();
        assert_eq!(Prefs::at(file.clone()).tuning().letters_min, None);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn lettering_and_recent_files_survive_a_round_trip() {
        let file = std::env::temp_dir().join(format!("hoop-prefs-{}", std::process::id()));
        let mut p = Prefs::at(file.clone());
        let s = Settings { text: "Two\nlines \\ ok".into(), fill: FillKind::Wave, align: Align::Right, height_mm: 33.5, ..Default::default() };
        p.store_lettering(&s, "Lobster");
        p.add_recent(std::path::Path::new("/a.jef"));
        p.add_recent(std::path::Path::new("/b.svg"));
        p.add_recent(std::path::Path::new("/a.jef"));
        p.save().unwrap();
        let q = Prefs::at(file.clone());
        let back = q.lettering();
        assert_eq!(back.text, s.text);
        assert_eq!(back.fill, FillKind::Wave);
        assert_eq!(back.align, Align::Right);
        assert_eq!(back.height_mm, 33.5);
        assert_eq!(q.get("letter.font"), Some("Lobster"));
        assert_eq!(q.recent(), vec![PathBuf::from("/a.jef"), PathBuf::from("/b.svg")]);
        std::fs::remove_file(file).unwrap();
    }
}
