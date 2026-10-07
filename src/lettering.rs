//! Turns text into stitches. Glyph outlines come from a font, get laid out in
//! 0.1 mm units (y down), optionally bent onto an arc, and are then filled
//! with tatami or satin rows and finished with a running or satin edge.

use std::path::PathBuf;

use ttf_parser::OutlineBuilder;

use crate::formats::{self, Cmd, Format, Pattern};

pub type Pt = (f32, f32);
pub type Contour = Vec<Pt>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillKind {
    None,
    Tatami,
    Satin,
    Wave,
    Zigzag,
    Crosshatch,
    Gradient,
    Open,
}

impl FillKind {
    /// Row spacing that suits the style, used when the user switches to it.
    pub fn default_rows_mm(self) -> f32 {
        match self {
            FillKind::Crosshatch => 1.6,
            FillKind::Open => 1.2,
            FillKind::Satin => 0.4,
            _ => 0.45,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    None,
    Running,
    Triple,
    Satin,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub text: String,
    pub font: usize,
    pub height_mm: f32,
    pub spacing_mm: f32,
    pub line_spacing: f32,
    pub arc_deg: f32,
    pub fill: FillKind,
    pub fill_angle: f32,
    pub density_mm: f32,
    pub stitch_mm: f32,
    pub wave_amp_mm: f32,
    pub wave_len_mm: f32,
    pub fade_mm: f32,
    pub underlay: bool,
    pub edge: EdgeKind,
    pub edge_width_mm: f32,
    /// Janome thread numbers (1-based index into the JEF chart).
    pub fill_thread: i32,
    pub edge_thread: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            text: "Hello".into(),
            font: 0,
            height_mm: 25.0,
            spacing_mm: 0.5,
            line_spacing: 1.3,
            arc_deg: 0.0,
            fill: FillKind::Tatami,
            fill_angle: 45.0,
            density_mm: 0.4,
            stitch_mm: 3.5,
            wave_amp_mm: 2.0,
            wave_len_mm: 12.0,
            fade_mm: 2.5,
            underlay: true,
            edge: EdgeKind::Satin,
            edge_width_mm: 1.6,
            fill_thread: 42,
            edge_thread: 1,
        }
    }
}

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
            FontData::File(path, index) => std::fs::read(path)
                .map(|b| (std::borrow::Cow::Owned(b), *index))
                .map_err(|e| format!("Couldn't read {}: {e}.", path.display())),
        }
    }
}

macro_rules! bundled {
    ($($name:literal => $file:literal),* $(,)?) => {
        vec![$(FontSource {
            name: $name.into(),
            featured: true,
            data: FontData::Bundled(include_bytes!(concat!("../assets/fonts/", $file))),
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
            let name_of = |id: u16| {
                face.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string())
            };
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
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found.dedup_by(|a, b| a.name == b.name);
    found
}

struct Outline {
    contours: Vec<Contour>,
    current: Contour,
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

pub fn dist(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Adds points so no edge is longer than `max`, which keeps straight edges
/// round once they are bent onto an arc.
pub fn densify(c: &Contour, max: f32) -> Contour {
    let mut out = Vec::with_capacity(c.len() * 2);
    for i in 0..c.len() {
        let (a, b) = (c[i], c[(i + 1) % c.len()]);
        out.push(a);
        let n = (dist(a, b) / max).floor() as usize;
        for k in 1..=n {
            let t = k as f32 / (n + 1) as f32;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

/// Lays the text out and returns the contours of each glyph in 0.1 mm, centered.
fn layout(face: &ttf_parser::Face, s: &Settings) -> Vec<Vec<Contour>> {
    let cap = face.capital_height().filter(|h| *h > 0).unwrap_or(face.ascender()) as f32;
    let k = s.height_mm * 10.0 / cap;
    let pitch = s.height_mm * 10.0 * s.line_spacing;
    let lines: Vec<&str> = s.text.lines().collect();
    let mut glyphs = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let mut pen = 0.0;
        let mut line_glyphs = Vec::new();
        for ch in line.chars() {
            let Some(id) = face.glyph_index(ch) else { continue };
            let mut o = Outline { contours: Vec::new(), current: Vec::new() };
            face.outline_glyph(id, &mut o);
            o.close();
            let y0 = row as f32 * pitch;
            let contours: Vec<Contour> =
                o.contours.into_iter().map(|c| c.into_iter().map(|(x, y)| (pen + x * k, y0 - y * k)).collect()).collect();
            if !contours.is_empty() {
                line_glyphs.push(contours);
            }
            pen += face.glyph_hor_advance(id).unwrap_or(0) as f32 * k + s.spacing_mm * 10.0;
        }
        let width = pen - s.spacing_mm * 10.0;
        for g in &mut line_glyphs {
            for c in g.iter_mut() {
                for p in c.iter_mut() {
                    p.0 -= width / 2.0;
                }
            }
        }
        glyphs.extend(line_glyphs);
    }

    let all = || glyphs.iter().flatten().flatten();
    let (mut minx, mut maxx) = (f32::MAX, f32::MIN);
    for p in all() {
        minx = minx.min(p.0);
        maxx = maxx.max(p.0);
    }
    let bend = s.arc_deg.to_radians();
    if bend.abs() > 0.01 && maxx > minx {
        // Bend the baseline of the first line onto a circle whose arc length
        // over the text width matches the requested angle.
        let r = (maxx - minx) / bend;
        for g in &mut glyphs {
            for c in g.iter_mut() {
                *c = densify(c, 10.0);
                for p in c.iter_mut() {
                    let phi = p.0 / r;
                    let rr = r - p.1;
                    *p = (rr * phi.sin(), r - rr * phi.cos());
                }
            }
        }
    }

    let (mut b0, mut b1, mut b2, mut b3) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b0 = b0.min(p.0);
        b1 = b1.min(p.1);
        b2 = b2.max(p.0);
        b3 = b3.max(p.1);
    }
    let (cx, cy) = ((b0 + b2) / 2.0, (b1 + b3) / 2.0);
    for p in glyphs.iter_mut().flatten().flatten() {
        p.0 -= cx;
        p.1 -= cy;
    }
    glyphs
}

/// Builds the stitch list with absolute positions.
pub struct Stitcher {
    pub points: Vec<Pt>,
    pub cmds: Vec<Cmd>,
    /// True until the current thread has sewn its first stitch, at the start
    /// and after every thread change. A fresh thread must not tie off or
    /// stitch from where the previous thread stopped.
    fresh: bool,
    /// The needle just jumped; tie in toward the next stitch.
    tie_in: bool,
}

/// Length of tie stitches in 0.1 mm.
const TIE: f32 = 5.0;

impl Stitcher {
    pub fn new() -> Self {
        Stitcher { points: Vec::new(), cmds: Vec::new(), fresh: true, tie_in: false }
    }

    pub fn last(&self) -> Option<Pt> {
        self.points.last().copied()
    }

    fn jump(&mut self, p: Pt) {
        self.points.push(p);
        self.cmds.push(Cmd::Jump);
        self.tie_in = true;
        // The needle is now where this thread starts.
        self.fresh = false;
    }

    pub fn stitch(&mut self, p: Pt) {
        let Some(l) = self.last() else {
            self.jump(p);
            return;
        };
        if self.fresh {
            self.fresh = false;
            if dist(l, p) >= 0.5 {
                self.jump(p);
            } else {
                self.tie_in = true;
            }
            return;
        }
        if dist(l, p) < 0.5 {
            return;
        }
        if self.tie_in {
            self.tie_in = false;
            self.tie(l, p);
        }
        self.points.push(p);
        self.cmds.push(Cmd::Stitch);
    }

    /// Three tiny stitches back and forth along a stitch's own line, so the
    /// thread can't pull out and the knot stays hidden under the stitching.
    fn tie(&mut self, at: Pt, toward: Pt) {
        let d = dist(at, toward);
        let v = if d > 0.1 { ((toward.0 - at.0) / d * TIE, (toward.1 - at.1) / d * TIE) } else { (TIE, 0.0) };
        for p in [(at.0 + v.0, at.1 + v.1), at, (at.0 + v.0, at.1 + v.1), at] {
            self.points.push(p);
            self.cmds.push(Cmd::Stitch);
        }
    }

    fn tie_off(&mut self) {
        if self.fresh || self.tie_in {
            return;
        }
        if let Some(l) = self.last() {
            let prev = self.points.len().checked_sub(2).map(|i| self.points[i]).unwrap_or(l);
            self.tie(l, prev);
        }
    }

    /// Moves to `p` without sewing. Short hops are stitched instead, and real
    /// jumps get tie stitches on both ends.
    pub fn travel(&mut self, p: Pt, _next: Pt) {
        match self.last() {
            None => self.jump(p),
            Some(_) if self.fresh => {
                self.fresh = false;
                self.jump(p);
            }
            Some(l) if dist(l, p) <= 25.0 => self.stitch(p),
            Some(_) => {
                self.tie_off();
                self.jump(p);
            }
        }
    }

    pub fn color_change(&mut self) {
        if let Some(l) = self.last() {
            self.tie_off();
            self.points.push(l);
            self.cmds.push(Cmd::ColorChange);
            self.fresh = true;
            self.tie_in = false;
        }
    }

    pub fn finish(&mut self) {
        self.tie_off();
    }
}

/// Points along a closed contour every `step`, starting at index `start`.
fn walk(c: &Contour, start: usize, step: f32) -> Vec<Pt> {
    let n = c.len();
    let mut out = vec![c[start]];
    let mut carry = 0.0;
    for i in 0..n {
        let (a, b) = (c[(start + i) % n], c[(start + i + 1) % n]);
        let len = dist(a, b);
        let mut t = step - carry;
        while t < len {
            out.push((a.0 + (b.0 - a.0) * t / len, a.1 + (b.1 - a.1) * t / len));
            t += step;
        }
        carry = len - (t - step);
    }
    out.push(c[start]);
    out
}

fn nearest_index(c: &Contour, to: Option<Pt>) -> usize {
    let Some(to) = to else { return 0 };
    (0..c.len()).min_by(|&a, &b| dist(c[a], to).total_cmp(&dist(c[b], to))).unwrap_or(0)
}

pub fn run_contour(st: &mut Stitcher, c: &Contour, step: f32, triple: bool) {
    let start = nearest_index(c, st.last());
    let pts = walk(c, start, step);
    st.travel(pts[0], pts.get(1).copied().unwrap_or(pts[0]));
    for w in pts.windows(2) {
        if triple {
            st.stitch(w[1]);
            st.stitch(w[0]);
        }
        st.stitch(w[1]);
    }
}

/// Points along an open polyline every `step`, ending exactly at its end.
fn walk_open(c: &[Pt], step: f32) -> Vec<Pt> {
    let mut out = vec![c[0]];
    let mut carry = 0.0;
    for w in c.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        let mut t = step - carry;
        while t < len {
            out.push((a.0 + (b.0 - a.0) * t / len, a.1 + (b.1 - a.1) * t / len));
            t += step;
        }
        carry = len - (t - step);
    }
    if out.last().is_some_and(|l| dist(*l, c[c.len() - 1]) > 1.0) {
        out.push(c[c.len() - 1]);
    }
    out
}

/// Runs along an open line, starting from whichever end is closer.
pub fn run_path(st: &mut Stitcher, c: &[Pt], step: f32, triple: bool) {
    if c.len() < 2 {
        return;
    }
    let mut c = c.to_vec();
    if let Some(l) = st.last() {
        if dist(l, c[c.len() - 1]) < dist(l, c[0]) {
            c.reverse();
        }
    }
    let pts = walk_open(&c, step);
    st.travel(pts[0], pts.get(1).copied().unwrap_or(pts[0]));
    for w in pts.windows(2) {
        if triple {
            st.stitch(w[1]);
            st.stitch(w[0]);
        }
        st.stitch(w[1]);
    }
}

/// A satin column along an open line, for strokes.
pub fn satin_path(st: &mut Stitcher, c: &[Pt], width: f32) {
    if c.len() < 2 {
        return;
    }
    let mut c = c.to_vec();
    if let Some(l) = st.last() {
        if dist(l, c[c.len() - 1]) < dist(l, c[0]) {
            c.reverse();
        }
    }
    let pts = walk_open(&c, 4.0);
    let n = pts.len();
    if n < 2 {
        return;
    }
    // A running pass along the line first holds the satin in place.
    st.travel(pts[0], pts[1]);
    for p in walk_open(&c, 25.0).into_iter().skip(1) {
        st.stitch(p);
    }
    for p in walk_open(&c, 25.0).into_iter().rev().skip(1) {
        st.stitch(p);
    }
    for i in 0..n {
        let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(0.01);
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        st.stitch((pts[i].0 + nx * side, pts[i].1 + ny * side));
    }
}

pub fn satin_contour(st: &mut Stitcher, c: &Contour, width: f32) {
    let start = nearest_index(c, st.last());
    let pts = walk(c, start, 4.0);
    let n = pts.len();
    if n < 3 {
        return;
    }
    let mut zig = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (pts[if i == 0 { n - 2 } else { i - 1 }], pts[if i == n - 1 { 1 } else { i + 1 }]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(0.01);
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        zig.push((pts[i].0 + nx * side, pts[i].1 + ny * side));
    }
    st.travel(pts[0], zig[0]);
    // A quick running pass first holds the satin in place.
    for p in walk(c, start, 25.0).into_iter().skip(1) {
        st.stitch(p);
    }
    for p in zig {
        st.stitch(p);
    }
}

/// Fill rows across a glyph at `angle`, using the nonzero winding rule so
/// overlapping contours in script fonts fill correctly.
/// Spans along the row at height `y` that lie inside the contours.
fn row_spans(contours: &[Contour], y: f32, even_odd: bool) -> Vec<(f32, f32)> {
    let mut hits: Vec<(f32, i32)> = Vec::new();
    for c in contours {
        for i in 0..c.len() {
            let (p, q) = (c[i], c[(i + 1) % c.len()]);
            if (p.1 <= y && q.1 > y) || (q.1 <= y && p.1 > y) {
                let x = p.0 + (y - p.1) / (q.1 - p.1) * (q.0 - p.0);
                hits.push((x, if q.1 > p.1 { 1 } else { -1 }));
            }
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let inside = |w: i32| if even_odd { w % 2 != 0 } else { w != 0 };
    let mut spans = Vec::new();
    let mut winding = 0;
    let mut start = 0.0;
    for (x, d) in hits {
        let was = inside(winding);
        winding += d;
        let now = inside(winding);
        if !was && now {
            start = x;
        } else if was && !now {
            spans.push((start, x));
        }
    }
    spans
}

/// Removes the parts of `spans` covered by `cut`. Both are sorted and disjoint.
fn subtract(spans: Vec<(f32, f32)>, cut: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for (mut a, b) in spans {
        for &(c0, c1) in cut {
            if c1 <= a || c0 >= b {
                continue;
            }
            if c0 > a {
                out.push((a, c0));
            }
            a = a.max(c1);
        }
        if b > a {
            out.push((a, b));
        }
    }
    out
}

pub fn fill_glyph(st: &mut Stitcher, glyph: &[Contour], kind: FillKind, angle: f32, s: &Settings) {
    fill_shape(st, glyph, false, &[], kind, angle, s);
}

/// Fills a shape with rows. `mask` holds shapes stacked on top, whose area is
/// left unstitched so layers don't pile up.
pub fn fill_shape(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[Contour], kind: FillKind, angle: f32, s: &Settings) {
    let a = angle.to_radians();
    let (cos, sin) = (a.cos(), a.sin());
    let rot = |p: Pt| (p.0 * cos + p.1 * sin, -p.0 * sin + p.1 * cos);
    let unrot = |p: Pt| (p.0 * cos - p.1 * sin, p.0 * sin + p.1 * cos);
    // Wave and zigzag rows are straight rows in a space sheared by the wave,
    // so the outline is warped the same way before scanning and the stitches
    // are unwarped afterwards.
    let (amp, len) = (s.wave_amp_mm * 10.0, s.wave_len_mm.max(1.0) * 10.0);
    let warp = move |x: f32| match kind {
        FillKind::Wave => amp * (x / len * std::f32::consts::TAU).sin(),
        FillKind::Zigzag => {
            let u = x / len;
            amp * (4.0 * (u - (u + 0.5).floor()).abs() - 1.0)
        }
        _ => 0.0,
    };
    let warped = matches!(kind, FillKind::Wave | FillKind::Zigzag);
    let to_row_space = |c: &Contour| -> Contour {
        let r: Contour = c.iter().map(|&p| rot(p)).collect();
        if warped { densify(&r, 5.0).into_iter().map(|(x, y)| (x, y - warp(x))).collect() } else { r }
    };
    let rm: Vec<Contour> = mask.iter().map(to_row_space).collect();
    let rc: Vec<Contour> = glyph
        .iter()
        .map(|c| {
            let r: Contour = c.iter().map(|&p| rot(p)).collect();
            if warped { densify(&r, 5.0).into_iter().map(|(x, y)| (x, y - warp(x))).collect() } else { r }
        })
        .collect();
    let (mut y0, mut y1) = (f32::MAX, f32::MIN);
    for p in rc.iter().flatten() {
        y0 = y0.min(p.1);
        y1 = y1.max(p.1);
    }
    let spacing = s.density_mm * 10.0;
    let fade = s.fade_mm.max(s.density_mm) * 10.0;
    let spacing_at = |y: f32| match kind {
        FillKind::Gradient => spacing + (fade - spacing) * ((y - y0) / (y1 - y0).max(1.0)).clamp(0.0, 1.0),
        _ => spacing,
    };
    let mut rows: Vec<(f32, Vec<(f32, f32, bool)>)> = Vec::new();
    let mut y = y0 + spacing / 2.0;
    while y < y1 {
        let mut spans = row_spans(&rc, y, even_odd);
        if !rm.is_empty() {
            spans = subtract(spans, &row_spans(&rm, y, false));
        }
        let segs: Vec<(f32, f32, bool)> = spans.into_iter().filter(|(a, b)| b - a > 2.0).map(|(a, b)| (a, b, false)).collect();
        rows.push((y, segs));
        y += spacing_at(y);
    }

    // Group rows into columns of overlapping segments so the needle works
    // through each part of the letter before jumping to the next.
    // Curved rows need shorter stitches to follow the curve.
    let stitch_len = if warped { (s.stitch_mm * 10.0).min(len / 8.0).max(12.0) } else { s.stitch_mm * 10.0 };
    loop {
        let Some(first) = rows.iter().position(|r| r.1.iter().any(|s| !s.2)) else { break };
        let mut ri = first;
        let mut si = rows[ri].1.iter().position(|s| !s.2).unwrap();
        let mut forward = match st.last() {
            Some(l) => {
                let (x0, x1, _) = rows[ri].1[si];
                let ly = rot(l);
                (ly.0 - x0).abs() <= (ly.0 - x1).abs()
            }
            None => true,
        };
        let mut started = false;
        loop {
            rows[ri].1[si].2 = true;
            let (x0, x1, _) = rows[ri].1[si];
            let yy = rows[ri].0;
            let (a, b) = if forward { (x0, x1) } else { (x1, x0) };
            let mut xs = vec![a];
            match kind {
                FillKind::Satin => {
                    let pieces = ((b - a).abs() / 70.0).ceil().max(1.0) as usize;
                    for k in 1..pieces {
                        xs.push(a + (b - a) * k as f32 / pieces as f32);
                    }
                }
                _ => {
                    // Brick pattern: each row's needle points shift by a third.
                    let phase = (ri % 3) as f32 * stitch_len / 3.0;
                    let mut g = ((x0.min(x1) - phase) / stitch_len).ceil() * stitch_len + phase;
                    let mut inner = Vec::new();
                    while g < x0.max(x1) {
                        if g - x0.min(x1) > 5.0 && x0.max(x1) - g > 5.0 {
                            inner.push(g);
                        }
                        g += stitch_len;
                    }
                    if kind == FillKind::Zigzag {
                        // Put a needle point on every chevron corner, or the
                        // stitches cut across the points and leave gaps.
                        let half = len / 2.0;
                        let mut c = (x0.min(x1) / half).ceil() * half;
                        while c < x0.max(x1) {
                            inner.retain(|g: &f32| (g - c).abs() > 4.0);
                            inner.push(c);
                            c += half;
                        }
                        inner.sort_by(|a, b| a.total_cmp(b));
                    }
                    if !forward {
                        inner.reverse();
                    }
                    xs.extend(inner);
                }
            }
            xs.push(b);
            let pts: Vec<Pt> = xs.iter().map(|&x| unrot((x, yy + warp(x)))).collect();
            if !started {
                st.travel(pts[0], pts[1]);
                started = true;
            } else {
                st.stitch(pts[0]);
            }
            for p in &pts[1..] {
                st.stitch(*p);
            }
            forward = !forward;
            // Continue in the next row with the unused segment overlapping this one most.
            let Some(next) = rows.get(ri + 1) else { break };
            let best = next
                .1
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.2)
                .map(|(i, s)| (i, s.1.min(x1) - s.0.max(x0)))
                .filter(|(_, overlap)| *overlap > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((i, _)) => {
                    ri += 1;
                    si = i;
                }
                None => break,
            }
        }
    }
}

pub fn generate(font: &FontSource, s: &Settings) -> Result<Pattern, String> {
    let (bytes, index) = font.bytes()?;
    let face = ttf_parser::Face::parse(&bytes, index).map_err(|_| format!("{} isn't a font Hoop can read.", font.name))?;
    let glyphs = layout(&face, s);
    if glyphs.is_empty() {
        return Err("Type some text to stitch.".into());
    }
    let mut st = Stitcher::new();
    let edge = |st: &mut Stitcher, g: &[Contour]| {
        for c in g {
            match s.edge {
                EdgeKind::None => {}
                EdgeKind::Running => run_contour(st, c, 25.0, false),
                EdgeKind::Triple => run_contour(st, c, 25.0, true),
                EdgeKind::Satin => satin_contour(st, c, s.edge_width_mm * 10.0),
            }
        }
    };
    let fill = |st: &mut Stitcher, g: &[Contour]| {
        if s.fill == FillKind::None {
            return;
        }
        if s.underlay {
            for c in g {
                run_contour(st, c, 25.0, false);
            }
        }
        fill_glyph(st, g, s.fill, s.fill_angle, s);
        if s.fill == FillKind::Crosshatch {
            fill_glyph(st, g, s.fill, s.fill_angle + 90.0, s);
        }
    };
    let two_threads = s.fill != FillKind::None && s.edge != EdgeKind::None && s.fill_thread != s.edge_thread;
    let mut threads = vec![formats::jef_thread(if s.fill == FillKind::None { s.edge_thread } else { s.fill_thread }, 0)];
    if two_threads {
        for g in &glyphs {
            fill(&mut st, g);
        }
        st.color_change();
        threads.push(formats::jef_thread(s.edge_thread, 1));
        for g in &glyphs {
            edge(&mut st, g);
        }
    } else {
        for g in &glyphs {
            fill(&mut st, g);
            edge(&mut st, g);
        }
    }
    st.finish();
    if !st.cmds.contains(&Cmd::Stitch) {
        return Err("Pick a fill or an edge so there is something to stitch.".into());
    }
    Ok(Pattern { format: Format::Lettering, points: st.points, cmds: st.cmds, threads })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_come_out_at_the_requested_height() {
        let font = &bundled_fonts()[1]; // Anton
        let s = Settings { text: "HI".into(), height_mm: 30.0, edge: EdgeKind::None, ..Default::default() };
        let p = generate(font, &s).unwrap();
        let b = p.bounds();
        assert!(((b.3 - b.1) / 10.0 - 30.0).abs() < 1.0, "height {}", (b.3 - b.1) / 10.0);
        assert_eq!(p.blocks().len(), 1);
    }

    #[test]
    fn separate_edge_thread_adds_a_color_change() {
        let font = &bundled_fonts()[0];
        let s = Settings { text: "O".into(), ..Default::default() };
        let p = generate(font, &s).unwrap();
        assert_eq!(p.blocks().len(), 2);
        assert_eq!(p.threads[0].name, "Cardinal Red");
        assert_eq!(p.threads[1].name, "Black");
    }

    #[test]
    fn every_fill_style_stitches_inside_the_letters() {
        let font = &bundled_fonts()[1];
        for fill in [FillKind::Tatami, FillKind::Satin, FillKind::Wave, FillKind::Zigzag, FillKind::Crosshatch, FillKind::Gradient, FillKind::Open] {
            let s = Settings { text: "HO".into(), fill, density_mm: fill.default_rows_mm(), edge: EdgeKind::None, underlay: false, ..Default::default() };
            let p = generate(font, &s).unwrap();
            let solid = generate(font, &Settings { fill: FillKind::Tatami, ..s.clone() }).unwrap().bounds();
            let b = p.bounds();
            assert!(p.count(Cmd::Stitch) > 100, "{fill:?}");
            // Stitches stay within the letters (allowing for rounding at the edge).
            assert!(b.0 >= solid.0 - 5.0 && b.2 <= solid.2 + 5.0 && b.1 >= solid.1 - 5.0 && b.3 <= solid.3 + 5.0, "{fill:?} {b:?} {solid:?}");
        }
    }

    #[test]
    fn a_new_thread_never_stitches_where_the_last_one_stopped() {
        let mut st = Stitcher::new();
        st.travel((0.0, 0.0), (100.0, 0.0));
        st.stitch((100.0, 0.0));
        st.color_change();
        let end = st.points.len();
        st.travel((300.0, 0.0), (400.0, 0.0));
        st.stitch((400.0, 0.0));
        // After the change: one jump to the new start, then stitches near it.
        assert_eq!(st.cmds[end], Cmd::Jump);
        assert!(st.points[end..].iter().all(|p| p.0 >= 300.0), "{:?}", &st.points[end..]);
        // The first thread was tied off at its end before the change.
        assert!(st.points[..end].iter().filter(|p| p.0 > 90.0).count() >= 3);
    }

    #[test]
    fn empty_text_is_reported() {
        let s = Settings { text: "  ".into(), ..Default::default() };
        assert!(generate(&bundled_fonts()[0], &s).is_err());
    }

    #[test]
    fn walk_spaces_points_evenly() {
        let square = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let pts = walk(&square, 0, 25.0);
        assert_eq!(pts.len(), 17);
        assert!(pts.windows(2).all(|w| (dist(w[0], w[1]) - 25.0).abs() < 0.01));
    }
}

#[cfg(test)]
mod samples {
    use super::*;

    /// Writes sample lettering as JEF files for a visual check:
    /// HOOP_OUT=/tmp/dir cargo test samples -- --ignored
    #[test]
    #[ignore]
    fn write_samples() {
        let Some(out) = std::env::var_os("HOOP_OUT") else { return };
        let fonts = bundled_fonts();
        let base = Settings { text: "HOOP".into(), height_mm: 45.0, spacing_mm: 1.5, ..Default::default() };
        let cases: Vec<(String, usize, Settings)> = [FillKind::Wave, FillKind::Zigzag, FillKind::Crosshatch, FillKind::Gradient]
            .into_iter()
            .map(|fill| (format!("{fill:?}").to_lowercase(), 1, Settings { fill, density_mm: fill.default_rows_mm(), fill_angle: 0.0, ..base.clone() }))
            .collect();
        for (name, font, s) in cases {
            let p = generate(&fonts[font], &s).unwrap();
            let threads: Vec<i32> = (0..p.blocks().len()).map(|i| if i == 0 { s.fill_thread } else { s.edge_thread }).collect();
            let b = p.bounds();
            let pts: Vec<Pt> = p.points.iter().map(|&(x, y)| (x - (b.0 + b.2) / 2.0, y - (b.1 + b.3) / 2.0)).collect();
            std::fs::write(std::path::Path::new(&out).join(format!("{name}.jef")), formats::write_jef(&pts, &p.cmds, &threads)).unwrap();
        }
    }
}
