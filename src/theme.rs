//! Follows the Omarchy theme. A theme switch replaces files and symlinks under
//! the theme directory, so instead of holding a file watch that a swap would
//! orphan, Hoop re-reads colors.toml whenever its modification time changes.

use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub accent: [u8; 3],
    pub background: [u8; 3],
    pub foreground: [u8; 3],
}

impl Default for Palette {
    /// Used when Omarchy isn't installed.
    fn default() -> Self {
        Palette { accent: [0xff, 0xd6, 0x0a], background: [0x1a, 0x1a, 0x1c], foreground: [0xe8, 0xe6, 0xe3] }
    }
}

pub struct Theme {
    file: PathBuf,
    stamp: Option<SystemTime>,
    pub palette: Palette,
}

fn parse_hex(v: &str) -> Option<[u8; 3]> {
    let v = v.trim().trim_matches('"').trim_matches('\'').trim_start_matches('#');
    if v.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(v, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

pub fn parse(text: &str) -> Palette {
    let mut p = Palette::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let Some(color) = parse_hex(value) else { continue };
        match key.trim() {
            "accent" => p.accent = color,
            "background" => p.background = color,
            "foreground" => p.foreground = color,
            _ => {}
        }
    }
    p
}

impl Theme {
    /// `dir` is Omarchy's current-theme state directory; tests pass a temp one.
    pub fn new(dir: PathBuf) -> Self {
        let mut t = Theme { file: dir.join("theme/colors.toml"), stamp: None, palette: Palette::default() };
        t.refresh();
        t
    }

    pub fn omarchy() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Theme::new(home.join(".local/state/omarchy/current"))
    }

    /// Re-reads the colors when the file changed. Returns true when they did.
    pub fn refresh(&mut self) -> bool {
        let stamp = std::fs::metadata(&self.file).and_then(|m| m.modified()).ok();
        if stamp == self.stamp {
            return false;
        }
        self.stamp = stamp;
        let palette = std::fs::read_to_string(&self.file).map(|t| parse(&t)).unwrap_or_default();
        let changed = palette != self.palette;
        self.palette = palette;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_colors_and_follows_changes() {
        let dir = std::env::temp_dir().join(format!("hoop-theme-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("theme")).unwrap();
        let file = dir.join("theme/colors.toml");
        std::fs::write(&file, "accent = \"#112233\"\nbackground = \"#000000\"\n").unwrap();
        let mut t = Theme::new(dir.clone());
        assert_eq!(t.palette.accent, [0x11, 0x22, 0x33]);
        assert_eq!(t.palette.foreground, Palette::default().foreground);

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&file, "accent = '#445566'\n").unwrap();
        let f = std::fs::File::options().write(true).open(&file).unwrap();
        f.set_modified(SystemTime::now() + std::time::Duration::from_secs(1)).unwrap();
        assert!(t.refresh());
        assert_eq!(t.palette.accent, [0x44, 0x55, 0x66]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn falls_back_without_omarchy() {
        let t = Theme::new(PathBuf::from("/nonexistent/hoop"));
        assert_eq!(t.palette, Palette::default());
    }
}
