//! Design library: the files in a designs folder with small pictures of
//! each, like the design browser in digitizing programs. Thumbnails are
//! drawn on a background thread and kept in ~/.cache/hoop/thumbs, keyed by
//! path, size and change time, so a folder opens instantly the second time.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use resvg::tiny_skia;

use crate::formats::{self, Cmd};

/// Thumbnail edge in pixels.
pub const SIZE: u32 = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Stitches,
    Svg,
    Picture,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
    pub modified: Option<std::time::SystemTime>,
    pub bytes: u64,
}

/// What a stitch file holds, shown under its picture and used for sorting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facts {
    pub w_mm: f32,
    pub h_mm: f32,
    pub stitches: usize,
    pub colors: usize,
}

pub fn kind_of(path: &Path) -> Option<Kind> {
    if path.is_dir() {
        return Some(Kind::Folder);
    }
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "jef" | "dst" | "exp" | "pes" | "pec" | "vp3" => Some(Kind::Stitches),
        "svg" | "svgz" => Some(Kind::Svg),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" => Some(Kind::Picture),
        _ => None,
    }
}

/// Folders first, then files Hoop can open, each sorted by name; hidden
/// files are left out.
pub fn scan(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Entry> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let meta = e.metadata().ok();
            Some(Entry { kind: kind_of(&path)?, path, name, modified: meta.as_ref().and_then(|m| m.modified().ok()), bytes: meta.map_or(0, |m| m.len()) })
        })
        .collect();
    out.sort_by(|a, b| (a.kind != Kind::Folder).cmp(&(b.kind != Kind::Folder)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Every file Hoop can open in `dir` and the folders below it, up to
/// `limit` of them, without the folders themselves.
pub fn scan_deep(dir: &Path, limit: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut todo = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = todo.pop() {
        for e in scan(&d) {
            if e.kind == Kind::Folder {
                if depth < 8 {
                    todo.push((e.path, depth + 1));
                }
            } else if out.len() < limit {
                out.push(e);
            }
        }
    }
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// RGBA pixels, SIZE × SIZE.
pub type Thumb = Vec<u8>;

fn cache_path(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok()?.hash(&mut h);
    SIZE.hash(&mut h);
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("hoop/thumbs").join(format!("{:016x}.png", h.finish())))
}

/// Size, stitches and colors of a stitch file, from the cache next to its
/// thumbnail or read now.
pub fn facts(path: &Path) -> Option<Facts> {
    if kind_of(path)? != Kind::Stitches {
        return None;
    }
    let cached = cache_path(path).map(|c| c.with_extension("txt"));
    if let Some(c) = &cached
        && let Ok(text) = std::fs::read_to_string(c)
    {
        let v: Vec<f32> = text.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        if let [w_mm, h_mm, stitches, colors] = v[..] {
            return Some(Facts { w_mm, h_mm, stitches: stitches as usize, colors: colors as usize });
        }
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    let p = formats::read(&name, &std::fs::read(path).ok()?).ok()?;
    let b = p.bounds();
    let f = Facts {
        w_mm: ((b.2 - b.0) / 10.0).max(0.0),
        h_mm: ((b.3 - b.1) / 10.0).max(0.0),
        stitches: p.cmds.iter().filter(|c| **c == Cmd::Stitch).count(),
        colors: p.blocks().len(),
    };
    if let Some(c) = &cached {
        if let Some(dir) = c.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(c, format!("{} {} {} {}", f.w_mm, f.h_mm, f.stitches, f.colors));
    }
    Some(f)
}

fn read_png(path: &Path) -> Option<Thumb> {
    let img = image::open(path).ok()?.to_rgba8();
    (img.width() == SIZE && img.height() == SIZE).then(|| img.into_raw())
}

fn write_png(path: &Path, px: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = image::save_buffer(path, px, SIZE, SIZE, image::ColorType::Rgba8);
}

/// A thumbnail for a design file, from the cache or drawn now.
pub fn thumbnail(path: &Path) -> Option<Thumb> {
    let cached = cache_path(path);
    if let Some(c) = &cached
        && let Some(t) = read_png(c)
    {
        return Some(t);
    }
    let t = match kind_of(path)? {
        Kind::Stitches => stitch_thumb(path)?,
        Kind::Svg => svg_thumb(path)?,
        Kind::Picture => picture_thumb(path)?,
        Kind::Folder => return None,
    };
    if let Some(c) = &cached {
        write_png(c, &t);
    }
    Some(t)
}

fn pixmap_rgba(p: &tiny_skia::Pixmap) -> Thumb {
    // tiny-skia stores premultiplied colors.
    p.pixels()
        .iter()
        .flat_map(|c| {
            let c = c.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

fn stitch_thumb(path: &Path) -> Option<Thumb> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let p = formats::read(&name, &std::fs::read(path).ok()?).ok()?;
    Some(draw_stitches(&p.points, &p.cmds, &p.blocks().iter().map(|b| b.thread.color).collect::<Vec<_>>()))
}

/// Draws stitches as thin anti-aliased lines, fitted with a margin.
pub fn draw_stitches(points: &[(f32, f32)], cmds: &[Cmd], colors: &[[u8; 3]]) -> Thumb {
    let mut pm = tiny_skia::Pixmap::new(SIZE, SIZE).unwrap();
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (&(x, y), _) in points.iter().zip(cmds).filter(|(_, c)| **c == Cmd::Stitch) {
        b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    if b.0 > b.2 {
        return pixmap_rgba(&pm);
    }
    let s = (SIZE as f32 - 12.0) / (b.2 - b.0).max(b.3 - b.1).max(1.0);
    let (ox, oy) = (SIZE as f32 / 2.0 - (b.0 + b.2) / 2.0 * s, SIZE as f32 / 2.0 - (b.1 + b.3) / 2.0 * s);
    let stroke = tiny_skia::Stroke { width: 1.0, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
    let mut block = 0;
    let mut pb = tiny_skia::PathBuilder::new();
    let flush = |pb: tiny_skia::PathBuilder, block: usize, pm: &mut tiny_skia::Pixmap| {
        if let Some(path) = pb.finish() {
            let c = colors.get(block).copied().unwrap_or([60, 60, 60]);
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(c[0], c[1], c[2], 255);
            paint.anti_alias = true;
            pm.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
        }
    };
    let mut pen = false;
    for i in 0..points.len() {
        let (x, y) = (points[i].0 * s + ox, points[i].1 * s + oy);
        match cmds[i] {
            Cmd::ColorChange => {
                flush(std::mem::replace(&mut pb, tiny_skia::PathBuilder::new()), block, &mut pm);
                block += 1;
                pen = false;
            }
            Cmd::Stitch => {
                if !pen && i > 0 {
                    pb.move_to(points[i - 1].0 * s + ox, points[i - 1].1 * s + oy);
                }
                if !pen && i == 0 {
                    pb.move_to(x, y);
                }
                pb.line_to(x, y);
                pen = true;
            }
            _ => pen = false,
        }
    }
    flush(pb, block, &mut pm);
    pixmap_rgba(&pm)
}

fn svg_thumb(path: &Path) -> Option<Thumb> {
    let data = std::fs::read(path).ok()?;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(&data, &opt).ok()?;
    let size = tree.size();
    let s = (SIZE as f32 - 8.0) / size.width().max(size.height());
    let mut pm = tiny_skia::Pixmap::new(SIZE, SIZE)?;
    let t = tiny_skia::Transform::from_row(s, 0.0, 0.0, s, (SIZE as f32 - size.width() * s) / 2.0, (SIZE as f32 - size.height() * s) / 2.0);
    resvg::render(&tree, t, &mut pm.as_mut());
    Some(pixmap_rgba(&pm))
}

fn picture_thumb(path: &Path) -> Option<Thumb> {
    let img = image::open(path).ok()?.thumbnail(SIZE, SIZE).to_rgba8();
    let mut out = vec![0u8; (SIZE * SIZE * 4) as usize];
    let (ox, oy) = ((SIZE - img.width()) / 2, (SIZE - img.height()) / 2);
    for (x, y, p) in img.enumerate_pixels() {
        let i = (((y + oy) * SIZE + x + ox) * 4) as usize;
        out[i..i + 4].copy_from_slice(&p.0);
    }
    Some(out)
}

/// Draws thumbnails one at a time on a background thread.
pub struct Painter {
    ask: Sender<PathBuf>,
    pub done: Receiver<(PathBuf, Option<Thumb>, Option<Facts>)>,
}

impl Painter {
    pub fn start(wake: impl Fn() + Send + 'static) -> Painter {
        let (ask, rx) = channel::<PathBuf>();
        let (tx, done) = channel();
        std::thread::spawn(move || {
            while let Ok(path) = rx.recv() {
                let t = thumbnail(&path);
                let f = facts(&path);
                if tx.send((path, t, f)).is_err() {
                    break;
                }
                wake();
            }
        });
        Painter { ask, done }
    }

    pub fn request(&self, path: PathBuf) {
        let _ = self.ask.send(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_stitch_thumbnails_with_thread_colors() {
        let p = formats::read("heart.jef", include_bytes!("../tests/fixtures/heart_5x5_fill.jef")).unwrap();
        let t = draw_stitches(&p.points, &p.cmds, &[[200, 0, 0]]);
        assert_eq!(t.len(), (SIZE * SIZE * 4) as usize);
        // The middle of the heart is red, a corner is empty.
        let at = |x: u32, y: u32| &t[((y * SIZE + x) * 4) as usize..((y * SIZE + x) * 4 + 4) as usize];
        assert_eq!(at(80, 80)[0], 200, "{:?}", at(80, 80));
        assert_eq!(at(2, 2)[3], 0);
    }

    #[test]
    fn facts_give_size_stitches_and_colors() {
        let dir = std::env::temp_dir().join(format!("hoop-facts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("heart.jef");
        std::fs::write(&file, include_bytes!("../tests/fixtures/heart_5x5_fill.jef")).unwrap();
        let f = facts(&file).unwrap();
        // A 5 × 5 inch heart.
        assert!((120.0..=135.0).contains(&f.w_mm) && (120.0..=135.0).contains(&f.h_mm), "{f:?}");
        assert!(f.stitches > 100 && f.colors == 1, "{f:?}");
        assert_eq!(facts(&dir.join("missing.svg")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_lists_folders_first_and_skips_other_files() {
        let dir = std::env::temp_dir().join(format!("hoop-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Zoo")).unwrap();
        for f in ["b.jef", "A.svg", "notes.txt", ".hidden.jef", "c.PNG"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        std::fs::write(dir.join("Zoo/d.dst"), b"").unwrap();
        let names: Vec<String> = scan(&dir).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Zoo", "A.svg", "b.jef", "c.PNG"]);
        let deep: Vec<String> = scan_deep(&dir, 100).into_iter().map(|e| e.name).collect();
        assert_eq!(deep, ["A.svg", "b.jef", "c.PNG", "d.dst"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
