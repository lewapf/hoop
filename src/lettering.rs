//! Turns text into stitches. Glyph outlines come from a font, get laid out in
//! 0.1 mm units (y down), optionally bent onto an arc, and are then filled
//! with tatami or satin rows and finished with a running or satin edge.

use std::path::PathBuf;

use ttf_parser::OutlineBuilder;

use crate::formats::{self, Cmd, Format, Pattern};
use crate::route::Region;

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
    Contour,
    /// A meandering line that never crosses itself, as in quilting.
    Stipple,
    /// An X in every cell of a grid.
    Cross,
    /// Satin columns that turn with the shape.
    Column,
    /// Columns where the shape is narrow enough for satin, tatami elsewhere.
    Auto,
}

impl FillKind {
    /// Row spacing that suits the style, used when the user switches to it.
    pub fn default_rows_mm(self) -> f32 {
        match self {
            FillKind::Crosshatch => 1.6,
            FillKind::Open => 1.2,
            FillKind::Satin | FillKind::Column => 0.4,
            FillKind::Stipple => 2.0,
            FillKind::Cross => 2.5,
            _ => 0.45,
        }
    }
}

/// The style's name as shown, in English; translate with `i18n::tr`.
pub fn fill_name(k: FillKind) -> &'static str {
    match k {
        FillKind::None => n!("None"),
        FillKind::Tatami => n!("Tatami"),
        FillKind::Satin => n!("Satin"),
        FillKind::Wave => n!("Wave"),
        FillKind::Zigzag => n!("Zigzag"),
        FillKind::Crosshatch => n!("Hatch"),
        FillKind::Gradient => n!("Gradient"),
        FillKind::Open => n!("Open"),
        FillKind::Contour => n!("Contour"),
        FillKind::Stipple => n!("Stipple"),
        FillKind::Cross => n!("Cross-stitch"),
        FillKind::Column => n!("Column"),
        FillKind::Auto => n!("Auto"),
    }
}

/// Whether a fill sews closely enough to hide a travel run under it.
pub fn covers(kind: FillKind, rows_mm: f32) -> bool {
    matches!(kind, FillKind::Tatami | FillKind::Satin | FillKind::Wave | FillKind::Zigzag | FillKind::Contour | FillKind::Column | FillKind::Auto) && rows_mm <= 0.8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchShape {
    None,
    Rect,
    Rounded,
    Oval,
    Circle,
    /// A rounded tag with a tab and a satin eyelet for a key ring.
    Fob,
}

/// What is sewn under a fill to hold the fabric and lift the top stitches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underlay {
    /// Edge run, plus a sparse tatami across wide shapes.
    Auto,
    /// A running stitch just inside the outline.
    Edge,
    /// Edge run and a sparse tatami at right angles to the fill.
    Tatami,
    /// Edge run and a loose zigzag across the shape.
    Zigzag,
    /// Edge run and two loose zigzags crossing each other, for pile fabrics.
    Double,
}

impl Underlay {
    pub const ALL: [Underlay; 5] = [Underlay::Auto, Underlay::Edge, Underlay::Tatami, Underlay::Zigzag, Underlay::Double];

    pub fn name(self) -> &'static str {
        match self {
            Underlay::Auto => n!("Auto"),
            Underlay::Edge => n!("Edge run"),
            Underlay::Tatami => n!("Tatami"),
            Underlay::Zigzag => n!("Zigzag"),
            Underlay::Double => n!("Double zigzag"),
        }
    }
}

/// The line the text sits on, or the envelope it is bent into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextShape {
    Straight,
    /// Along an arc of `arc_deg`.
    Arc,
    /// Badge: the first line round the top of a circle, the second round
    /// the bottom (both upright), more lines straight in the middle.
    Circle,
    /// The top edge bows up.
    Arch,
    /// The bottom edge bows down.
    Valley,
    /// Both edges bow out.
    Bulge,
    /// Both edges bow in.
    Pinch,
    /// The whole text bows up, letters kept upright.
    Bridge,
    Wave,
    /// Letters grow from left to right.
    Grow,
    /// Three initials with the middle one bigger.
    Monogram,
}

impl TextShape {
    pub const ALL: [TextShape; 11] = [
        TextShape::Straight, TextShape::Arc, TextShape::Circle, TextShape::Arch, TextShape::Valley, TextShape::Bulge,
        TextShape::Pinch, TextShape::Bridge, TextShape::Wave, TextShape::Grow, TextShape::Monogram,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TextShape::Straight => n!("Straight"),
            TextShape::Arc => n!("Arc"),
            TextShape::Circle => n!("Circle badge"),
            TextShape::Arch => n!("Arch"),
            TextShape::Valley => n!("Valley"),
            TextShape::Bulge => n!("Bulge"),
            TextShape::Pinch => n!("Pinch"),
            TextShape::Bridge => n!("Bridge"),
            TextShape::Wave => n!("Wave"),
            TextShape::Grow => n!("Grow"),
            TextShape::Monogram => n!("Monogram"),
        }
    }

    /// Whether the shape takes the envelope amount.
    pub fn bends(self) -> bool {
        matches!(self, TextShape::Arch | TextShape::Valley | TextShape::Bulge | TextShape::Pinch | TextShape::Bridge | TextShape::Wave | TextShape::Grow)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    None,
    Running,
    Triple,
    Satin,
    /// E-stitch: a run with short stitches into the shape, for appliqué.
    Blanket,
    /// Half circles along the edge.
    Scallop,
}

/// Sews the edge style round one outline.
pub fn edge_contour(st: &mut Stitcher, c: &Contour, s: &Settings) {
    match s.edge {
        EdgeKind::None => {}
        EdgeKind::Running => run_contour(st, c, 25.0, false),
        EdgeKind::Triple => run_contour(st, c, 25.0, true),
        EdgeKind::Satin => satin_contour(st, c, s.edge_width_mm * 10.0),
        EdgeKind::Blanket => crate::special::blanket_contour(st, c, s.edge_width_mm * 10.0),
        EdgeKind::Scallop => crate::special::scallop_contour(st, c, s.edge_width_mm * 10.0, false),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub text: String,
    pub font: usize,
    pub height_mm: f32,
    pub spacing_mm: f32,
    pub line_spacing: f32,
    pub arc_deg: f32,
    pub shape: TextShape,
    /// Inner diameter of the circle badge.
    pub circle_mm: f32,
    /// How far envelopes bend the text.
    pub envelope_mm: f32,
    pub fill: FillKind,
    pub fill_angle: f32,
    pub density_mm: f32,
    pub stitch_mm: f32,
    pub wave_amp_mm: f32,
    pub wave_len_mm: f32,
    pub fade_mm: f32,
    /// Extra length added to both ends of every fill row, because stitches
    /// pull the fabric in and fills come out narrower than drawn.
    pub pull_mm: f32,
    /// Italic slant in degrees.
    pub slant_deg: f32,
    pub align: Align,
    /// A background shape behind the text, for badges and name patches.
    pub patch: PatchShape,
    pub patch_pad_mm: f32,
    pub patch_fill: FillKind,
    pub patch_border_mm: f32,
    pub patch_thread: i32,
    pub patch_border_thread: i32,
    /// Fabric appliqué instead of a stitched background.
    pub patch_applique: bool,
    /// Made in the hoop: a stop for backing fabric under the hoop and a
    /// tack-down before the border, so the piece is finished front and back.
    pub patch_ith: bool,
    pub underlay: bool,
    pub underlay_kind: Underlay,
    /// Widest part, in mm, that the Auto style still sews as a satin column.
    pub column_max_mm: f32,
    pub edge: EdgeKind,
    pub edge_width_mm: f32,
    /// Janome thread numbers (1-based index into the JEF chart).
    pub fill_thread: i32,
    pub edge_thread: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            text: t!("Hello").into(),
            font: 0,
            height_mm: 25.0,
            spacing_mm: 0.5,
            line_spacing: 1.3,
            arc_deg: 0.0,
            shape: TextShape::Straight,
            circle_mm: 60.0,
            envelope_mm: 8.0,
            fill: FillKind::Tatami,
            fill_angle: 45.0,
            density_mm: 0.4,
            stitch_mm: 3.5,
            wave_amp_mm: 2.0,
            wave_len_mm: 12.0,
            fade_mm: 2.5,
            pull_mm: 0.2,
            slant_deg: 0.0,
            align: Align::Center,
            patch: PatchShape::None,
            patch_pad_mm: 5.0,
            patch_fill: FillKind::Tatami,
            patch_border_mm: 3.0,
            patch_thread: 2,
            patch_border_thread: 30,
            patch_applique: false,
            patch_ith: false,
            underlay: true,
            underlay_kind: Underlay::Auto,
            column_max_mm: 7.0,
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
                .map_err(|e| t!("Couldn't read {v0}: {e}.", v0 = path.display(), e = e)),
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
/// Puts the first line round the top of a circle and the second round the
/// bottom, both reading left to right with letters upright; further lines
/// stay straight in the middle.
fn circle_badge(glyphs: &mut [Vec<Contour>], line_of: &[usize], s: &Settings, pitch: f32) {
    let cap = s.height_mm * 10.0;
    let r_in = (s.circle_mm * 10.0 / 2.0).max(cap);
    let r_out = r_in + cap;
    let middle: Vec<usize> = line_of.iter().copied().filter(|&l| l >= 2).collect();
    let (first_mid, last_mid) = (middle.iter().copied().min().unwrap_or(2), middle.iter().copied().max().unwrap_or(2));
    // Middle lines are stacked around the center: baseline of the middle of them at cap/2.
    let mid_shift = -((first_mid + last_mid) as f32 / 2.0) * pitch + cap / 2.0;
    for (g, &line) in glyphs.iter_mut().zip(line_of) {
        let base = line as f32 * pitch;
        for c in g.iter_mut() {
            if line < 2 {
                *c = densify(c, 8.0);
            }
            for p in c.iter_mut() {
                let rel = p.1 - base;
                *p = match line {
                    0 => {
                        let rho = r_in - rel;
                        let th = p.0 / r_in;
                        (rho * th.sin(), -rho * th.cos())
                    }
                    1 => {
                        let rho = r_out + rel;
                        let th = -p.0 / r_out;
                        (-rho * th.sin(), rho * th.cos())
                    }
                    _ => (p.0, p.1 + mid_shift),
                };
            }
        }
    }
}

/// Bends the text's box: `amount` is how far the edges move, in 0.1 mm.
fn envelope(glyphs: &mut [Vec<Contour>], shape: TextShape, amount: f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let (w, h) = ((b.2 - b.0).max(1.0), (b.3 - b.1).max(1.0));
    let cy = (b.1 + b.3) / 2.0;
    for g in glyphs.iter_mut() {
        for c in g.iter_mut() {
            *c = densify(c, 8.0);
            for p in c.iter_mut() {
                let u = (p.0 - b.0) / w;
                let v = (p.1 - b.1) / h;
                let bow = 4.0 * u * (1.0 - u);
                p.1 = match shape {
                    TextShape::Arch => p.1 - amount * bow * (1.0 - v),
                    TextShape::Valley => p.1 + amount * bow * v,
                    TextShape::Bulge => p.1 + amount * bow * (2.0 * v - 1.0),
                    TextShape::Pinch => p.1 - amount * bow * (2.0 * v - 1.0) * (h / (h + 2.0 * amount)).min(0.9),
                    TextShape::Bridge => p.1 - amount * bow,
                    TextShape::Wave => p.1 + amount * (u * std::f32::consts::TAU).sin(),
                    TextShape::Grow => cy + (p.1 - cy) * (1.0 + (amount / h).min(0.9) * (2.0 * u - 1.0)),
                    _ => p.1,
                };
            }
        }
    }
}

fn layout(face: &ttf_parser::Face, s: &Settings) -> Vec<Vec<Contour>> {
    let cap = face.capital_height().filter(|h| *h > 0).unwrap_or(face.ascender()) as f32;
    let k = s.height_mm * 10.0 / cap;
    let pitch = s.height_mm * 10.0 * s.line_spacing;
    let lines: Vec<&str> = s.text.lines().collect();
    let slant = s.slant_deg.to_radians().tan();
    let kern = |a: ttf_parser::GlyphId, b: ttf_parser::GlyphId| -> f32 {
        let Some(table) = face.tables().kern else { return 0.0 };
        table
            .subtables
            .into_iter()
            .filter(|t| t.horizontal && !t.variable)
            .find_map(|t| t.glyphs_kerning(a, b))
            .unwrap_or(0) as f32
    };
    let cap_h = s.height_mm * 10.0;
    let mut rows: Vec<(f32, Vec<Vec<Contour>>)> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let mut pen = 0.0;
        let mut prev = None;
        let mut line_glyphs = Vec::new();
        let y0 = row as f32 * pitch;
        let count = line.chars().filter(|c| !c.is_whitespace()).count();
        let mut nth = 0;
        for ch in line.chars() {
            let Some(id) = face.glyph_index(ch) else { continue };
            // A monogram's middle initial is half again as tall, centered
            // on the others.
            let big = s.shape == TextShape::Monogram && count % 2 == 1 && !ch.is_whitespace() && nth == count / 2;
            if !ch.is_whitespace() {
                nth += 1;
            }
            let kk = if big { k * 1.5 } else { k };
            let drop = if big { cap_h * 0.25 } else { 0.0 };
            if let Some(p) = prev {
                pen += kern(p, id) * k;
            }
            prev = Some(id);
            let mut o = Outline { contours: Vec::new(), current: Vec::new() };
            face.outline_glyph(id, &mut o);
            o.close();
            let contours: Vec<Contour> = o
                .contours
                .into_iter()
                .map(|c| c.into_iter().map(|(x, y)| (pen + x * kk + y * kk * slant, y0 + drop - y * kk)).collect())
                .collect();
            if !contours.is_empty() {
                line_glyphs.push(contours);
            }
            pen += face.glyph_hor_advance(id).unwrap_or(0) as f32 * kk + s.spacing_mm * 10.0;
        }
        rows.push((pen - s.spacing_mm * 10.0, line_glyphs));
    }
    let widest = rows.iter().map(|r| r.0).fold(0.0, f32::max);
    let mut glyphs = Vec::new();
    let mut line_of = Vec::new();
    for (row, (width, mut line_glyphs)) in rows.into_iter().enumerate() {
        // Badge lines are centered on the circle whatever the alignment.
        let align = if s.shape == TextShape::Circle && row < 2 { Align::Center } else { s.align };
        let shift = match align {
            Align::Left => -widest / 2.0,
            Align::Center => -width / 2.0,
            Align::Right => widest / 2.0 - width,
        };
        for p in line_glyphs.iter_mut().flatten().flatten() {
            p.0 += shift;
        }
        line_of.extend(std::iter::repeat_n(row, line_glyphs.len()));
        glyphs.extend(line_glyphs);
    }
    match s.shape {
        TextShape::Circle => circle_badge(&mut glyphs, &line_of, s, pitch),
        sh if sh.bends() => envelope(&mut glyphs, sh, s.envelope_mm * 10.0),
        _ => {}
    }

    let all = || glyphs.iter().flatten().flatten();
    let (mut minx, mut maxx) = (f32::MAX, f32::MIN);
    for p in all() {
        minx = minx.min(p.0);
        maxx = maxx.max(p.0);
    }
    let bend = s.arc_deg.to_radians();
    if s.shape == TextShape::Arc && bend.abs() > 0.01 && maxx > minx {
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
    /// The shape being sewn, set by the caller around one shape's stitches:
    /// moves within it run through the shape instead of jumping.
    pub region: Option<Region>,
}

/// Length of tie stitches in 0.1 mm.
const TIE: f32 = 5.0;
/// Stitches shorter than 0.3 mm are skipped: the needle lands in its own
/// hole, which cuts the thread or the fabric (tie stitches are longer).
pub const MIN_STITCH: f32 = 3.0;

impl Stitcher {
    pub fn new() -> Self {
        Stitcher { points: Vec::new(), cmds: Vec::new(), fresh: true, tie_in: false, region: None }
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
        if dist(l, p) < MIN_STITCH {
            return;
        }
        if self.tie_in {
            self.tie_in = false;
            self.tie(l, p);
        }
        if let Some(r) = &mut self.region {
            r.mark(l, p);
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

    /// Moves to `p` without sewing. Short hops are stitched instead, moves
    /// within the current shape run through it, and real jumps get tie
    /// stitches on both ends.
    pub fn travel(&mut self, p: Pt, _next: Pt) {
        match self.last() {
            None => self.jump(p),
            Some(_) if self.fresh => {
                self.fresh = false;
                self.jump(p);
            }
            Some(l) if dist(l, p) <= 25.0 => self.stitch(p),
            Some(l) => match self.region.as_ref().and_then(|r| r.route(l, p)) {
                Some(path) => {
                    for q in path {
                        self.stitch(q);
                    }
                }
                None => {
                    self.tie_off();
                    self.jump(p);
                }
            },
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

/// Stitch rows for satin of `width` along a line: (point on one side,
/// point on the other). The stitch direction turns gradually over a
/// width's length around corners, rows are kept every 0.4 mm on whichever
/// side moves most, so the outside of a corner stays covered, and where
/// the inside crowds every other stitch is shortened.
fn satin_rows(line: &[Pt], closed: bool, width: f32) -> Vec<(Pt, Pt)> {
    let mut path = line.to_vec();
    if closed && !path.is_empty() {
        path.push(path[0]);
    }
    let fine = walk_open(&path, 1.0);
    let n = fine.len();
    if n < 2 {
        return Vec::new();
    }
    let half = width / 2.0;
    let reach = (half.max(3.0)).round() as usize;
    let mut out: Vec<(Pt, Pt)> = Vec::new();
    let mut last: Option<(Pt, Pt, Pt)> = None;
    let step = 4.0;
    for i in 0..n {
        let (a, b) = if closed {
            let m = n - 1;
            (fine[(i + m - reach % m) % m], fine[(i + reach) % m])
        } else {
            (fine[i.saturating_sub(reach)], fine[(i + reach).min(n - 1)])
        };
        let d = dist(a, b).max(1e-3);
        let nrm = (-(b.1 - a.1) / d * half, (b.0 - a.0) / d * half);
        let c = fine[i];
        let (l, r) = ((c.0 + nrm.0, c.1 + nrm.1), (c.0 - nrm.0, c.1 - nrm.1));
        let keep = match last {
            None => true,
            Some((lc, ll, lr)) => dist(c, lc).max(dist(l, ll)).max(dist(r, lr)) >= step || i == n - 1,
        };
        if keep {
            out.push((l, r));
            last = Some((c, l, r));
        }
    }
    // Shorten every other stitch where one side crowds.
    for k in 2..out.len() {
        let (l, r) = out[k];
        let (pl, pr) = out[k - 2];
        let c = ((l.0 + r.0) / 2.0, (l.1 + r.1) / 2.0);
        let pull = |p: Pt| (c.0 + (p.0 - c.0) * 0.6, c.1 + (p.1 - c.1) * 0.6);
        if k % 4 >= 2 {
            if dist(l, pl) < step * 0.5 {
                out[k].0 = pull(l);
            }
            if dist(r, pr) < step * 0.5 {
                out[k].1 = pull(r);
            }
        }
    }
    out
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
    let rows = satin_rows(&c, false, width);
    if rows.len() < 2 {
        return;
    }
    // A running pass along the line first holds the satin in place.
    let run = walk_open(&c, 25.0);
    st.travel(run[0], run.get(1).copied().unwrap_or(run[0]));
    for p in run.iter().skip(1) {
        st.stitch(*p);
    }
    for p in run.iter().rev().skip(1) {
        st.stitch(*p);
    }
    for (i, (l, r)) in rows.iter().enumerate() {
        st.stitch(if i % 2 == 0 { *l } else { *r });
    }
}

pub fn satin_contour(st: &mut Stitcher, c: &Contour, width: f32) {
    let start = nearest_index(c, st.last());
    let mut ring: Contour = c[start..].to_vec();
    ring.extend_from_slice(&c[..start]);
    let rows = satin_rows(&ring, true, width);
    if rows.len() < 3 {
        return;
    }
    st.travel(ring[0], rows[0].0);
    // A quick running pass first holds the satin in place.
    for p in walk(c, start, 25.0).into_iter().skip(1) {
        st.stitch(p);
    }
    for (i, (l, r)) in rows.iter().enumerate() {
        st.stitch(if i % 2 == 0 { *l } else { *r });
    }
}

/// Fill rows across a glyph at `angle`, using the nonzero winding rule so
/// overlapping contours in script fonts fill correctly.
/// Spans along the row at height `y` that lie inside the contours.
pub fn row_spans(contours: &[Contour], y: f32, even_odd: bool) -> Vec<(f32, f32)> {
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

/// A shape stacked on top of a fill, cut out of it: its contours and whether
/// it uses the even-odd rule.
pub type MaskShape = (Vec<Contour>, bool);

/// Where a row crosses any of the mask shapes. Each shape is tested on its
/// own, so shapes drawn in opposite directions don't cancel where they meet.
pub(crate) fn mask_spans(mask: &[MaskShape], y: f32) -> Vec<(f32, f32)> {
    let mut all: Vec<(f32, f32)> = mask.iter().flat_map(|(cs, eo)| row_spans(cs, y, *eo)).collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(all.len());
    for (a, b) in all {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Removes the parts of `spans` covered by `cut`. Both are sorted and disjoint.
pub(crate) fn subtract(spans: Vec<(f32, f32)>, cut: &[(f32, f32)]) -> Vec<(f32, f32)> {
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

/// Area of a shape in square units, measured with the fill's own row scan
/// so holes and the fill rule count the same way.
pub fn shape_area(glyph: &[Contour], even_odd: bool) -> f32 {
    let (mut y0, mut y1) = (f32::MAX, f32::MIN);
    for p in glyph.iter().flatten() {
        (y0, y1) = (y0.min(p.1), y1.max(p.1));
    }
    let step = ((y1 - y0) / 400.0).max(1.0);
    let mut area = 0.0;
    let mut y = y0 + step / 2.0;
    while y < y1 {
        area += row_spans(glyph, y, even_odd).iter().map(|(a, b)| b - a).sum::<f32>() * step;
        y += step;
    }
    area
}

/// Typical width of a shape: twice its area over its outline length. A
/// long strip w wide gives about w; a disc gives its radius.
pub fn thickness(glyph: &[Contour], even_odd: bool) -> f32 {
    let outline: f32 = glyph.iter().map(|c| (0..c.len()).map(|i| dist(c[i], c[(i + 1) % c.len()])).sum::<f32>()).sum();
    2.0 * shape_area(glyph, even_odd) / outline.max(1e-3)
}

/// What goes under a fill to hold the fabric: a running stitch around the
/// shape and, for wide fills, a sparse fill across the top rows, inset
/// 0.5 mm so it never shows at the edges.
pub fn underlay(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], kind: FillKind, angle: f32, s: &Settings) {
    let kind = resolve(kind, glyph, even_odd, mask, s);
    if matches!(kind, FillKind::Column | FillKind::Stipple | FillKind::Cross) {
        // Columns lay their own underlay; stipple and cross-stitch are
        // open, so an outline under them would show.
        return;
    }
    // The fill covers underlay, so its travel may cross anything inside.
    if let Some(r) = &mut st.region {
        r.strict = false;
    }
    for c in glyph {
        run_contour(st, c, 25.0, false);
    }
    let thick = thickness(glyph, even_odd) > 30.0;
    let open = matches!(kind, FillKind::Open | FillKind::Crosshatch | FillKind::None);
    let pass = |st: &mut Stitcher, k: FillKind, a: f32, rows: f32| {
        let u = Settings { density_mm: rows, stitch_mm: 3.0, pull_mm: -0.5, ..s.clone() };
        fill_shape(st, glyph, even_odd, mask, k, a, &u);
    };
    match s.underlay_kind {
        Underlay::Auto if !open && kind != FillKind::Satin && thick => pass(st, FillKind::Tatami, angle + 90.0, 2.5),
        Underlay::Tatami if !open => pass(st, FillKind::Tatami, angle + 90.0, 2.5),
        Underlay::Zigzag if !open => pass(st, FillKind::Satin, angle + 90.0, 2.0),
        Underlay::Double if !open => {
            pass(st, FillKind::Satin, angle + 45.0, 2.2);
            pass(st, FillKind::Satin, angle - 45.0, 2.2);
        }
        _ => {}
    }
    if let Some(r) = &mut st.region {
        r.strict = true;
    }
}

pub fn fill_glyph(st: &mut Stitcher, glyph: &[Contour], kind: FillKind, angle: f32, s: &Settings) {
    fill_shape(st, glyph, false, &[], kind, angle, s);
}

/// Fills a shape with rows. `mask` holds shapes stacked on top, whose area is
/// left unstitched so layers don't pile up.
/// The style a shape really gets: Auto becomes a column when the shape's
/// widest part fits a satin stitch, tatami otherwise.
pub fn resolve(kind: FillKind, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) -> FillKind {
    if kind != FillKind::Auto {
        return kind;
    }
    if crate::column::widest(glyph, even_odd, mask) <= s.column_max_mm * 10.0 { FillKind::Column } else { FillKind::Tatami }
}

pub fn fill_shape(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], kind: FillKind, angle: f32, s: &Settings) {
    if let Some(r) = &mut st.region {
        r.clear_sewn();
    }
    let kind = resolve(kind, glyph, even_odd, mask, s);
    match kind {
        FillKind::Column => return crate::column::column_fill(st, glyph, even_odd, mask, s),
        FillKind::Stipple => return crate::special::stipple_fill(st, glyph, even_odd, mask, s),
        FillKind::Cross => return crate::special::cross_fill(st, glyph, even_odd, mask, s),
        _ => {}
    }
    if kind == FillKind::Contour {
        contour_fill(st, glyph, even_odd, mask, s);
        return;
    }
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
    let rm: Vec<MaskShape> = mask.iter().map(|(cs, eo)| (cs.iter().map(to_row_space).collect(), *eo)).collect();
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
            spans = subtract(spans, &mask_spans(&rm, y));
        }
        let pull = s.pull_mm * 10.0;
        // A negative pull insets rows (used by underlay); drop rows it empties.
        let segs: Vec<(f32, f32, bool)> =
            spans.into_iter().filter(|(a, b)| b - a > 2.0).map(|(a, b)| (a - pull, b + pull, false)).filter(|(a, b, _)| b - a > 2.0).collect();
        rows.push((y, segs));
        y += spacing_at(y);
    }

    // Group rows into columns of overlapping segments so the needle works
    // through each part of the letter before jumping to the next.
    // Curved rows need shorter stitches to follow the curve.
    let stitch_len = if warped { (s.stitch_mm * 10.0).min(len / 8.0).max(12.0) } else { s.stitch_mm * 10.0 };
    loop {
        // Start the next column at the head (no unsewn part above it) nearest
        // the needle, so the move there is short and stays inside.
        let here = st.last().map(rot);
        let mut best: Option<(f32, usize, usize)> = None;
        for (ri, row) in rows.iter().enumerate() {
            for (si, seg) in row.1.iter().enumerate() {
                if seg.2 || ri > 0 && rows[ri - 1].1.iter().any(|o| !o.2 && o.1 > seg.0 && o.0 < seg.1) {
                    continue;
                }
                let d = here.map_or(0.0, |h| dist(h, (seg.0, row.0)).min(dist(h, (seg.1, row.0))));
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, ri, si));
                }
            }
            if here.is_none() && best.is_some() {
                break;
            }
        }
        let Some((_, mut ri, mut si)) = best else { break };
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
                // The step to the next row runs along the edge; where the
                // edge slants away it can get long, so split it.
                if let Some(l) = st.last() {
                    let step = stitch_len.max(25.0);
                    let n = (dist(l, pts[0]) / step).ceil() as usize;
                    for k in 1..n {
                        let t = k as f32 / n as f32;
                        st.stitch((l.0 + (pts[0].0 - l.0) * t, l.1 + (pts[0].1 - l.1) * t));
                    }
                }
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

/// The laid-out letter outlines, as `generate` stitches them.
#[cfg(test)]
pub(crate) fn outlines(font: &FontSource, s: &Settings) -> Vec<Vec<Contour>> {
    let (bytes, index) = font.bytes().unwrap();
    let face = ttf_parser::Face::parse(&bytes, index).unwrap();
    layout(&face, s)
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
            edge_contour(st, c, s);
        }
    };
    let fill = |st: &mut Stitcher, g: &[Contour]| {
        if s.fill == FillKind::None {
            return;
        }
        st.region = Region::new(g, false, &[], &[]);
        if s.underlay {
            underlay(st, g, false, &[], s.fill, s.fill_angle, s);
        }
        fill_glyph(st, g, s.fill, s.fill_angle, s);
        if s.fill == FillKind::Crosshatch {
            fill_glyph(st, g, s.fill, s.fill_angle + 90.0, s);
        }
        st.region = None;
    };
    // Thread changes happen only when the thread really changes, except for
    // appliqué stops, where the machine must halt even in the same thread.
    let mut threads: Vec<crate::formats::Thread> = Vec::new();
    let mut current: Option<i32> = None;
    let mut use_thread = |st: &mut Stitcher, t: i32, stop_as: Option<&str>| {
        if current != Some(t) || stop_as.is_some() {
            if !threads.is_empty() {
                st.color_change();
            }
            let mut thread = formats::jef_thread(t, threads.len());
            if let Some(name) = stop_as {
                thread.name = name.to_string();
                thread.code = String::new();
            }
            threads.push(thread);
            current = if stop_as.is_some() { None } else { Some(t) };
        }
    };

    let patch = patch_outline(&glyphs, s);
    if let Some(outline) = &patch {
        if s.patch_applique {
            // Placement line shows where the fabric goes; the machine stops,
            // the fabric is laid on, the tack-down holds it, and the machine
            // stops again so the extra can be trimmed before the cover stitch.
            use_thread(&mut st, s.patch_thread, Some(t!("Placement line")));
            run_contour(&mut st, outline, 25.0, false);
            use_thread(&mut st, s.patch_thread, Some(t!("Tack-down (lay the fabric first)")));
            run_contour(&mut st, outline, 20.0, false);
            run_contour(&mut st, outline, 20.0, false);
        } else if s.patch_fill != FillKind::None {
            use_thread(&mut st, s.patch_thread, None);
            let mask: Vec<MaskShape> = glyphs.iter().map(|g| (g.clone(), false)).collect();
            let cover = if covers(s.fill, s.density_mm) { &mask[..] } else { &[] };
            st.region = Region::new(std::slice::from_ref(outline), false, &mask, cover);
            run_contour(&mut st, outline, 25.0, false);
            let bg = Settings { density_mm: s.patch_fill.default_rows_mm(), underlay: false, ..s.clone() };
            fill_shape(&mut st, std::slice::from_ref(outline), false, &mask, s.patch_fill, s.fill_angle + 90.0, &bg);
            if s.patch_fill == FillKind::Crosshatch {
                // Lace: the second layer of the lattice.
                fill_shape(&mut st, std::slice::from_ref(outline), false, &mask, s.patch_fill, s.fill_angle, &bg);
            }
            st.region = None;
        }
    }

    let text_fill = s.fill != FillKind::None;
    let text_edge = s.edge != EdgeKind::None;
    if text_fill && text_edge && s.fill_thread != s.edge_thread {
        use_thread(&mut st, s.fill_thread, None);
        for g in &glyphs {
            fill(&mut st, g);
        }
        use_thread(&mut st, s.edge_thread, None);
        for g in &glyphs {
            edge(&mut st, g);
        }
    } else if text_fill || text_edge {
        use_thread(&mut st, if text_fill { s.fill_thread } else { s.edge_thread }, None);
        for g in &glyphs {
            fill(&mut st, g);
            edge(&mut st, g);
        }
    }

    // In the hoop: backing fabric goes under the hoop and is held by a
    // tack-down that the border then covers.
    if let (Some(outline), true) = (&patch, s.patch_ith) {
        use_thread(&mut st, s.patch_border_thread, Some(t!("Back (tape backing fabric under the hoop)")));
        run_contour(&mut st, outline, 20.0, false);
    }
    // The border goes on last so it covers the edge of the background fill.
    if let Some(outline) = &patch {
        // Appliqué needs a border wide enough to cover the trimmed fabric edge.
        let border = if s.patch_applique { s.patch_border_mm.max(2.5) } else { s.patch_border_mm };
        if border > 0.0 {
            use_thread(&mut st, s.patch_border_thread, None);
            satin_contour(&mut st, outline, border * 10.0);
        }
        if let Some(eye) = fob_eyelet(&glyphs, s) {
            // Satin round the key-ring hole; cut it open after sewing.
            satin_contour(&mut st, &eye, 12.0);
        }
    }
    st.finish();
    if !st.cmds.contains(&Cmd::Stitch) {
        return Err("Pick a fill or an edge so there is something to stitch.".into());
    }
    Ok(Pattern { format: Format::Lettering, points: st.points, cmds: st.cmds, threads })
}

/// The patch shape around the laid-out text, if one is chosen.
fn patch_outline(glyphs: &[Vec<Contour>], s: &Settings) -> Option<Contour> {
    if s.patch == PatchShape::None {
        return None;
    }
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let pad = s.patch_pad_mm * 10.0;
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let (hw, hh) = ((b.2 - b.0) / 2.0, (b.3 - b.1) / 2.0);
    let ellipse = |a: f32, bb: f32| -> Contour {
        (0..96).map(|i| {
            let t = i as f32 / 96.0 * std::f32::consts::TAU;
            (cx + a * t.cos(), cy + bb * t.sin())
        }).collect()
    };
    Some(match s.patch {
        PatchShape::None => return None,
        PatchShape::Rect | PatchShape::Rounded => {
            let (w, h) = (hw + pad, hh + pad);
            let r = if s.patch == PatchShape::Rounded { (pad * 1.6).min(w).min(h) } else { 0.0 };
            let mut c = Vec::new();
            for (qx, qy, start) in [(w - r, -h + r, -90.0f32), (w - r, h - r, 0.0), (-w + r, h - r, 90.0), (-w + r, -h + r, 180.0)] {
                for i in 0..=12 {
                    let t = (start + i as f32 * 7.5).to_radians();
                    c.push((cx + qx + r * t.cos(), cy + qy + r * t.sin()));
                }
            }
            c.dedup_by(|a, b| dist(*a, *b) < 0.5);
            c
        }
        // An ellipse with the text box's proportions that passes outside its corners.
        PatchShape::Oval => ellipse(hw * std::f32::consts::SQRT_2 + pad, hh * std::f32::consts::SQRT_2 + pad),
        PatchShape::Circle => {
            let r = (hw * hw + hh * hh).sqrt() + pad;
            ellipse(r, r)
        }
        PatchShape::Fob => {
            // A pill round the text with a tab to the left for the ring.
            let h = hh + pad;
            let (left, right) = (cx - hw - pad - 2.0 * h, cx + hw + pad);
            let mut c = Vec::new();
            for i in 0..=24 {
                let t = (-90.0 + i as f32 * 7.5f32).to_radians();
                c.push((right - h + h * t.cos(), cy + h * t.sin()));
            }
            for i in 0..=24 {
                let t = (90.0 + i as f32 * 7.5f32).to_radians();
                c.push((left + h + h * t.cos(), cy + h * t.sin()));
            }
            c
        }
    })
}

/// The key-ring hole of a fob: a circle in the middle of its tab.
fn fob_eyelet(glyphs: &[Vec<Contour>], s: &Settings) -> Option<Contour> {
    if s.patch != PatchShape::Fob {
        return None;
    }
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let pad = s.patch_pad_mm * 10.0;
    let h = (b.3 - b.1) / 2.0 + pad;
    let (cx, cy) = (b.0 - pad - h, (b.1 + b.3) / 2.0);
    let r = (h * 0.45).clamp(25.0, 45.0);
    Some((0..48).map(|i| {
        let t = i as f32 / 48.0 * std::f32::consts::TAU;
        (cx + r * t.cos(), cy + r * t.sin())
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_shapes_drawn_opposite_ways_both_cut() {
        // Two overlapping squares, one clockwise and one not: together their
        // winding cancels where they overlap, but both must still be cut.
        let a: Contour = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let b: Contour = vec![(50.0, 0.0), (50.0, 100.0), (150.0, 100.0), (150.0, 0.0)];
        assert_eq!(row_spans(&[a.clone(), b.clone()], 50.0, false), vec![(0.0, 50.0), (100.0, 150.0)]);
        assert_eq!(mask_spans(&[(vec![a], false), (vec![b], false)], 50.0), vec![(0.0, 150.0)]);
    }

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
        for fill in [FillKind::Tatami, FillKind::Satin, FillKind::Wave, FillKind::Zigzag, FillKind::Crosshatch, FillKind::Gradient, FillKind::Open, FillKind::Contour] {
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
    fn patch_adds_background_and_border_threads_around_the_text() {
        let font = &bundled_fonts()[1];
        let s = Settings { text: "AB".into(), patch: PatchShape::Rounded, ..Default::default() };
        let p = generate(font, &s).unwrap();
        // background, text fill, text edge, border
        assert_eq!(p.blocks().len(), 4);
        let plain = generate(font, &Settings { patch: PatchShape::None, ..s.clone() }).unwrap().bounds();
        let b = p.bounds();
        assert!(b.2 - b.0 > plain.2 - plain.0 + 2.0 * 50.0, "patch is wider than text plus padding");
        // A border in the same thread as the edge doesn't add a change.
        let same = Settings { patch_border_thread: s.edge_thread, patch_fill: FillKind::None, ..s.clone() };
        assert_eq!(generate(font, &same).unwrap().blocks().len(), 2);
    }

    #[test]
    fn applique_adds_two_stops_before_the_text() {
        let font = &bundled_fonts()[1];
        let s = Settings { text: "AB".into(), patch: PatchShape::Oval, patch_applique: true, patch_thread: 1, edge_thread: 1, fill_thread: 1, patch_border_thread: 1, ..Default::default() };
        let p = generate(font, &s).unwrap();
        let names: Vec<&str> = p.threads.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names[0], "Placement line");
        assert!(names[1].starts_with("Tack-down"));
        // The rest is one thread, so no further stops.
        assert_eq!(p.blocks().len(), 3, "{names:?}");
    }

    #[test]
    fn empty_text_is_reported() {
        let s = Settings { text: "  ".into(), ..Default::default() };
        assert!(generate(&bundled_fonts()[0], &s).is_err());
    }

    #[test]
    fn distance_transform_is_exact_on_a_line() {
        let f = [0.0, 1e12, 1e12, 1e12, 0.0, 1e12];
        let mut out = [0.0; 6];
        edt_1d(&f, &mut out);
        assert_eq!(out, [0.0, 1.0, 4.0, 1.0, 0.0, 1.0]);
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
        let cases: Vec<(String, usize, Settings)> = vec![
            ("contour".into(), 0, Settings { text: "GO".into(), height_mm: 45.0, fill: FillKind::Contour, density_mm: 0.5, edge: EdgeKind::None, ..Default::default() }),
            ("column".into(), std::env::var("F").ok().and_then(|v| v.parse().ok()).unwrap_or(0), Settings { text: std::env::var("HOOP_TEXT").unwrap_or("Bremen".into()), height_mm: std::env::var("HOOP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0), fill: FillKind::Column, density_mm: 0.4, edge: EdgeKind::None, ..Default::default() }),
        ];
        let only = std::env::var("HOOP_CASE").ok();
        let cases: Vec<_> = cases.into_iter().filter(|c| only.as_ref().is_none_or(|o| *o == c.0)).collect();
        for (name, font, s) in cases {
            let p = generate(&fonts[font], &s).unwrap();
            let threads: Vec<i32> = p.threads.iter().map(|t| formats::nearest_jef(t.color)).collect();
            let b = p.bounds();
            let pts: Vec<Pt> = p.points.iter().map(|&(x, y)| (x - (b.0 + b.2) / 2.0, y - (b.1 + b.3) / 2.0)).collect();
            std::fs::write(std::path::Path::new(&out).join(format!("{name}.jef")), formats::write_jef(&pts, &p.cmds, &threads)).unwrap();
        }
    }
}

/// One-click looks for lettering. They change only the stitch style, never
/// the text, font, size or threads.
pub const PRESETS: [(&str, &str, fn(&mut Settings)); 10] = [
    (n!("Classic"), n!("Tatami fill with a satin border"), |s| {
        style(s, FillKind::Tatami, 45.0, EdgeKind::Satin);
        s.edge_width_mm = 1.6;
    }),
    (n!("Satin"), n!("Satin columns that follow each stroke, as digitizers sew lettering. Best for small text"), |s| style(s, FillKind::Column, 90.0, EdgeKind::None)),
    (n!("Outline"), n!("Only a triple-run outline"), |s| style(s, FillKind::None, 45.0, EdgeKind::Triple)),
    (n!("Bold"), n!("Tatami with a wide satin border"), |s| {
        style(s, FillKind::Tatami, 45.0, EdgeKind::Satin);
        s.edge_width_mm = 2.8;
    }),
    (n!("Vintage"), n!("Open crosshatch inside a thin border"), |s| {
        style(s, FillKind::Crosshatch, 45.0, EdgeKind::Satin);
        s.edge_width_mm = 1.2;
    }),
    (n!("Wavy"), n!("Wave fill with a satin border"), |s| {
        style(s, FillKind::Wave, 0.0, EdgeKind::Satin);
        s.edge_width_mm = 1.4;
    }),
    (n!("Shaded"), n!("Gradient fill with a running outline"), |s| {
        style(s, FillKind::Gradient, 90.0, EdgeKind::Running);
        s.fade_mm = 3.0;
    }),
    (n!("Key fob"), n!("In the hoop: an appliqué tag with a key-ring eyelet, finished on both sides"), |s| {
        style(s, FillKind::Column, 90.0, EdgeKind::None);
        s.height_mm = 10.0;
        s.patch = PatchShape::Fob;
        s.patch_pad_mm = 4.0;
        s.patch_border_mm = 3.0;
        s.patch_applique = true;
        s.patch_fill = FillKind::None;
        s.patch_ith = true;
    }),
    (n!("Coaster"), n!("In the hoop: a round coaster, appliqué front and backed fabric"), |s| {
        style(s, FillKind::Column, 90.0, EdgeKind::None);
        s.patch = PatchShape::Circle;
        s.patch_pad_mm = 12.0;
        s.patch_border_mm = 3.5;
        s.patch_applique = true;
        s.patch_fill = FillKind::None;
        s.patch_ith = true;
    }),
    (n!("Lace"), n!("Free-standing lace: a lattice that holds together once the water-soluble stabilizer is rinsed away"), |s| {
        style(s, FillKind::Column, 90.0, EdgeKind::None);
        s.patch = PatchShape::Circle;
        s.patch_pad_mm = 8.0;
        s.patch_border_mm = 3.0;
        s.patch_applique = false;
        s.patch_fill = FillKind::Crosshatch;
        s.patch_ith = false;
    }),
];

pub fn style(s: &mut Settings, fill: FillKind, angle: f32, edge: EdgeKind) {
    s.fill = fill;
    s.fill_angle = angle;
    s.edge = edge;
    s.density_mm = fill.default_rows_mm();
    s.underlay = !matches!(fill, FillKind::Open | FillKind::Crosshatch);
}

/// Rings that follow the outline inward, one row spacing apart. The shape is
/// rasterized, every inside cell gets its distance to the edge, and the rings
/// are the lines of equal distance, traced with marching squares.
fn contour_fill(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
    let spacing = (s.density_mm * 10.0).max(2.0);
    let cell = (spacing / 3.0).clamp(1.0, 3.0);
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyph.iter().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    if b.2 <= b.0 || b.3 <= b.1 {
        return;
    }
    // One empty cell of border keeps every ring closed.
    let (x0, y0) = (b.0 - 2.0 * cell, b.1 - 2.0 * cell);
    let w = ((b.2 - b.0) / cell) as usize + 5;
    let h = ((b.3 - b.1) / cell) as usize + 5;
    let mut field = vec![0.0f32; w * h];
    for j in 0..h {
        let y = y0 + (j as f32 + 0.5) * cell;
        let mut spans = row_spans(glyph, y, even_odd);
        if !mask.is_empty() {
            spans = subtract(spans, &mask_spans(mask, y));
        }
        for (a, bb) in spans {
            let i0 = (((a - x0) / cell) - 0.5).ceil().max(0.0) as usize;
            let i1 = ((((bb - x0) / cell) - 0.5).floor() as isize).min(w as isize - 1);
            for i in i0..=(i1.max(-1) as usize).min(w - 1) {
                if (i as isize) <= i1 {
                    field[j * w + i] = f32::MAX;
                }
            }
        }
    }
    // Exact distance to the nearest outside cell, in cells (Felzenszwalb and
    // Huttenlocher's squared distance transform, columns then rows).
    for v in field.iter_mut() {
        *v = if *v > 0.0 { 1e12 } else { 0.0 };
    }
    let mut buf = vec![0.0f32; w.max(h)];
    let mut out = vec![0.0f32; w.max(h)];
    for i in 0..w {
        for j in 0..h {
            buf[j] = field[j * w + i];
        }
        edt_1d(&buf[..h], &mut out[..h]);
        for j in 0..h {
            field[j * w + i] = out[j];
        }
    }
    for j in 0..h {
        buf[..w].copy_from_slice(&field[j * w..j * w + w]);
        edt_1d(&buf[..w], &mut out[..w]);
        for i in 0..w {
            field[j * w + i] = out[i].sqrt();
        }
    }
    let deepest = field.iter().copied().fold(0.0, f32::max) * cell;
    let center = |i: usize, j: usize| (x0 + (i as f32 + 0.5) * cell, y0 + (j as f32 + 0.5) * cell);
    // Rings curve tightly, so stitches stay short enough to follow them.
    let stitch_len = (s.stitch_mm * 10.0).min(20.0);
    let mut level = spacing / 2.0;
    while level < deepest {
        let lv = level / cell;
        let mut rings = iso_lines(&field, w, h, lv, &center);
        // Nearest ring first, so the needle moves inward without long jumps.
        while !rings.is_empty() {
            let from = st.last().unwrap_or(rings[0][0]);
            let (ri, pi) = rings
                .iter()
                .enumerate()
                .flat_map(|(r, ring)| ring.iter().enumerate().map(move |(p, pt)| (r, p, dist(*pt, from))))
                .min_by(|a, b| a.2.total_cmp(&b.2))
                .map(|(r, p, _)| (r, p))
                .unwrap();
            let ring = rings.swap_remove(ri);
            if ring.len() < 3 {
                continue;
            }
            let pts = walk(&ring, pi, stitch_len);
            st.travel(pts[0], pts.get(1).copied().unwrap_or(pts[0]));
            for p in &pts[1..] {
                st.stitch(*p);
            }
        }
        level += spacing;
    }
}

/// One dimension of the squared Euclidean distance transform: `out[q]` is the
/// smallest `(q - p)² + f[p]`.
pub(crate) fn edt_1d(f: &[f32], out: &mut [f32]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f32; n + 1];
    let mut k = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let sx = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            if sx <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if sx <= z[k] {
                // k == 0: replace the only parabola.
                v[0] = q;
                z[1] = f32::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = sx;
            z[k + 1] = f32::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        *o = d * d + f[v[k]];
    }
}

/// Closed lines where `field` equals `level`, from marching squares over cell
/// centers. Points come back in world units through `center`.
pub fn iso_lines(field: &[f32], w: usize, h: usize, level: f32, center: &dyn Fn(usize, usize) -> Pt) -> Vec<Contour> {
    use std::collections::HashMap;
    let v = |i: usize, j: usize| field[j * w + i].min(1e6);
    // An edge point is keyed by the cell edge it lies on, so neighbors agree.
    type Key = (usize, usize, bool);
    let lerp = |a: Pt, b: Pt, fa: f32, fb: f32| {
        let t = ((level - fa) / (fb - fa)).clamp(0.0, 1.0);
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    };
    let mut next: HashMap<Key, Key> = HashMap::new();
    let mut pos: HashMap<Key, Pt> = HashMap::new();
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let (a, b, c, d) = (v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1));
            let idx = (a > level) as u8 | ((b > level) as u8) << 1 | ((c > level) as u8) << 2 | ((d > level) as u8) << 3;
            if idx == 0 || idx == 15 {
                continue;
            }
            // Edges: top (i,j)-(i+1,j), right, bottom, left.
            let top: Key = (i, j, true);
            let bottom: Key = (i, j + 1, true);
            let left: Key = (i, j, false);
            let right: Key = (i + 1, j, false);
            let (pa, pb, pc, pd) = (center(i, j), center(i + 1, j), center(i + 1, j + 1), center(i, j + 1));
            pos.entry(top).or_insert_with(|| lerp(pa, pb, a, b));
            pos.entry(right).or_insert_with(|| lerp(pb, pc, b, c));
            pos.entry(bottom).or_insert_with(|| lerp(pd, pc, d, c));
            pos.entry(left).or_insert_with(|| lerp(pa, pd, a, d));
            // Segments oriented with the inside on a consistent side.
            let segs: &[(Key, Key)] = match idx {
                1 => &[(left, top)],
                2 => &[(top, right)],
                3 => &[(left, right)],
                4 => &[(right, bottom)],
                5 => &[(left, top), (right, bottom)],
                6 => &[(top, bottom)],
                7 => &[(left, bottom)],
                8 => &[(bottom, left)],
                9 => &[(bottom, top)],
                10 => &[(top, right), (bottom, left)],
                11 => &[(bottom, right)],
                12 => &[(right, left)],
                13 => &[(right, top)],
                14 => &[(top, left)],
                _ => &[],
            };
            for &(p, q) in segs {
                next.insert(p, q);
            }
        }
    }
    let mut rings = Vec::new();
    while let Some(&start) = next.keys().next() {
        let mut ring = Vec::new();
        let mut k = start;
        while let Some(n) = next.remove(&k) {
            ring.push(pos[&k]);
            k = n;
            if k == start {
                break;
            }
        }
        if ring.len() > 2 {
            rings.push(ring);
        }
    }
    rings
}
