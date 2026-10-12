//! Turns SVG artwork into stitches. usvg resolves the file into plain paths
//! with absolute transforms; filled paths become fill regions, stroked paths
//! become running or satin lines, and every color becomes one thread.

use std::collections::HashSet;
use std::path::Path;

use crate::formats::{self, Cmd, Format, Pattern};
use crate::lettering::{self, Contour, EdgeKind, FillKind, Pt, Settings, Stitcher};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Fill { even_odd: bool },
    Stroke { width: f32 },
}

/// One painted path. Coordinates are SVG user units, y down.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    /// Polylines with whether each one is closed.
    pub lines: Vec<(Contour, bool)>,
    pub layer: usize,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub color: [u8; 3],
    pub background: bool,
    pub shapes: usize,
}

#[derive(Clone, Debug)]
pub struct Artwork {
    pub name: String,
    pub shapes: Vec<Shape>,
    pub layers: Vec<Layer>,
    /// Bounds of everything in user units: min x, min y, max x, max y.
    pub bounds: (f32, f32, f32, f32),
    pub skipped_images: usize,
}

impl Artwork {
    pub fn aspect(&self) -> f32 {
        (self.bounds.3 - self.bounds.1) / (self.bounds.2 - self.bounds.0).max(1e-3)
    }
}

/// Per-color choices in the order of `Artwork::layers`.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerChoice {
    pub thread: i32,
    pub enabled: bool,
    pub look: Look,
}

/// How one thread's shapes are stitched, overriding the main fill style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Default,
    Fill(FillKind),
    Outline,
    /// Sewn on as a piece of fabric: placement line, stop, tack-down, stop, satin cover.
    Applique,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtSettings {
    pub width_mm: f32,
    /// Fill style, edge and stitch settings shared with lettering. Its text,
    /// font and thread fields are unused here.
    pub style: Settings,
    pub knockout: bool,
    pub min_detail_mm: f32,
    pub stroke: EdgeKind,
    pub stroke_min_mm: f32,
    pub group_colors: bool,
    pub layers: Vec<LayerChoice>,
    /// The shapes as the user arranged them, in stitch order (first is sewn
    /// first and lies at the bottom).
    pub shapes: Vec<ShapeEdit>,
}

/// One shape on the design: a shape of the drawing, or its own geometry once
/// its points were edited or it was added from the built-in shapes, plus
/// how it is moved, scaled and turned. Offsets are SVG user units.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeEdit {
    pub source: usize,
    pub own: Option<Shape>,
    pub name: String,
    pub layer: usize,
    /// How this shape is stitched, overriding its thread's look.
    pub look: Option<Look>,
    pub hidden: bool,
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub rotate: f32,
}

impl ShapeEdit {
    pub fn of(art: &Artwork, i: usize) -> Self {
        let sh = &art.shapes[i];
        let kind = match sh.kind {
            Kind::Fill { .. } => "Fill",
            Kind::Stroke { .. } => "Line",
        };
        ShapeEdit { source: i, own: None, name: format!("{kind} {}", i + 1), layer: sh.layer, look: None, hidden: false, dx: 0.0, dy: 0.0, scale: 1.0, rotate: 0.0 }
    }

    pub fn base<'a>(&'a self, art: &'a Artwork) -> &'a Shape {
        match &self.own {
            Some(own) => own,
            None => &art.shapes[self.source],
        }
    }

    pub fn moved(&self) -> bool {
        self.dx != 0.0 || self.dy != 0.0 || self.scale != 1.0 || self.rotate != 0.0
    }

    /// The outline with the move, scale and turn applied, in user units.
    /// Scale and turn go around the middle of the shape.
    pub fn lines(&self, art: &Artwork) -> Vec<(Contour, bool)> {
        let sh = self.base(art);
        if !self.moved() {
            return sh.lines.clone();
        }
        let (cx, cy) = middle(&sh.lines);
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let k = self.scale;
        let f = |p: Pt| {
            let (x, y) = ((p.0 - cx) * k, (p.1 - cy) * k);
            (x * cos - y * sin + cx + self.dx, x * sin + y * cos + cy + self.dy)
        };
        sh.lines.iter().map(|(c, closed)| (c.iter().map(|&p| f(p)).collect(), *closed)).collect()
    }

    /// Bakes the move into the points so they can be edited one by one.
    pub fn bake(&mut self, art: &Artwork) {
        if self.own.is_some() && !self.moved() {
            return;
        }
        let mut sh = self.base(art).clone();
        sh.lines = self.lines(art);
        self.own = Some(sh);
        (self.dx, self.dy, self.scale, self.rotate) = (0.0, 0.0, 1.0, 0.0);
    }
}

/// Bounding box of some lines: min x, min y, max x, max y. Empty lines give
/// an inverted box.
pub fn extent(lines: &[(Contour, bool)]) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in lines.iter().flat_map(|l| l.0.iter()) {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    b
}

/// Middle of the bounding box of some lines.
pub fn middle(lines: &[(Contour, bool)]) -> Pt {
    let b = extent(lines);
    if b.0 > b.2 { (0.0, 0.0) } else { ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0) }
}

/// How user units map to stitch units (0.1 mm): scale and the frame's middle.
pub fn frame(art: &Artwork, s: &ArtSettings) -> (f32, Pt) {
    let scale = s.width_mm * 10.0 / (art.bounds.2 - art.bounds.0).max(1e-3);
    (scale, ((art.bounds.0 + art.bounds.2) / 2.0, (art.bounds.1 + art.bounds.3) / 2.0))
}

/// The topmost visible shape under a point in stitch units, within `slop`.
pub fn shape_at(art: &Artwork, s: &ArtSettings, p: Pt, slop: f32) -> Option<usize> {
    let (scale, c) = frame(art, s);
    let q = (p.0 / scale + c.0, p.1 / scale + c.1);
    let slop = slop / scale;
    (0..s.shapes.len()).rev().find(|&i| {
        let e = &s.shapes[i];
        if e.hidden || !s.layers.get(e.layer).is_none_or(|l| l.enabled) {
            return false;
        }
        let lines = e.lines(art);
        let inside = matches!(e.base(art).kind, Kind::Fill { .. }) && {
            let mut wind = false;
            for (c, _) in &lines {
                for k in 0..c.len() {
                    let (a, b) = (c[k], c[(k + 1) % c.len()]);
                    if (a.1 > q.1) != (b.1 > q.1) && q.0 < a.0 + (q.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                        wind = !wind;
                    }
                }
            }
            wind
        };
        inside || lines.iter().any(|(c, closed)| near_line(c, *closed, q) <= slop.max(stroke_half(e.base(art))))
    })
}

fn stroke_half(sh: &Shape) -> f32 {
    match sh.kind {
        Kind::Stroke { width } => width / 2.0,
        Kind::Fill { .. } => 0.0,
    }
}

/// Distance from a point to a polyline.
pub fn near_line(c: &Contour, closed: bool, q: Pt) -> f32 {
    let n = c.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    let mut best = c.first().map_or(f32::MAX, |&a| lettering::dist(a, q));
    for k in 0..segs {
        let (a, b) = (c[k], c[(k + 1) % n]);
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let t = (((q.0 - a.0) * vx + (q.1 - a.1) * vy) / (vx * vx + vy * vy).max(1e-9)).clamp(0.0, 1.0);
        best = best.min(lettering::dist((a.0 + vx * t, a.1 + vy * t), q));
    }
    best
}

/// Built-in shapes that can be added to any artwork.
pub const BUILT_IN: [&str; 7] = ["Circle", "Square", "Heart", "Star", "Shield", "Banner", "Line"];

/// One built-in shape, `size` user units across, centered on `at`.
pub fn built_in(name: &str, at: Pt, size: f32, layer: usize) -> Shape {
    use std::f32::consts::{PI, TAU};
    let r = size / 2.0;
    let mut kind = Kind::Fill { even_odd: false };
    let pts: Vec<Pt> = match name {
        "Circle" => (0..72).map(|k| (k as f32 * TAU / 72.0).sin_cos()).map(|(s, c)| (c * r, s * r)).collect(),
        "Square" => {
            // Rounded corners stitch cleaner than sharp ones.
            let (h, rc) = (r, r * 0.12);
            let mut v = Vec::new();
            for (cx, cy, a0) in [(h - rc, h - rc, 0.0), (-h + rc, h - rc, 0.5 * PI), (-h + rc, -h + rc, PI), (h - rc, -h + rc, 1.5 * PI)] {
                for k in 0..=6 {
                    let a = a0 + k as f32 * PI / 12.0;
                    v.push((cx + rc * a.cos(), cy + rc * a.sin()));
                }
            }
            v
        }
        "Heart" => (0..96)
            .map(|k| {
                let t = k as f32 * TAU / 96.0;
                let x = 16.0 * t.sin().powi(3);
                let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                (x * r / 17.0, -y * r / 17.0 - r * 0.1)
            })
            .collect(),
        "Star" => (0..10)
            .map(|k| {
                let a = -PI / 2.0 + k as f32 * PI / 5.0;
                let rr = if k % 2 == 0 { r } else { r * 0.42 };
                (rr * a.cos(), rr * a.sin())
            })
            .collect(),
        "Shield" => {
            let mut v = vec![(-r * 0.85, -r), (r * 0.85, -r)];
            for k in 0..=16 {
                let t = k as f32 / 16.0;
                // Straight sides that curve in to a point at the bottom.
                let y = -r * 0.2 + t * r * 1.2;
                let x = r * 0.85 * (1.0 - t.powf(1.8));
                v.push((x, y));
            }
            for k in (0..16).rev() {
                let t = k as f32 / 16.0;
                v.push((-r * 0.85 * (1.0 - t.powf(1.8)), -r * 0.2 + t * r * 1.2));
            }
            v
        }
        "Banner" => {
            let (h, notch) = (r * 0.3, r * 0.15);
            let mut v = Vec::new();
            for k in 0..=24 {
                let x = -r + k as f32 * r / 12.0;
                v.push((x, -h + (x / r * PI).cos() * -r * 0.08));
            }
            v.push((r - notch, 0.0));
            for k in (0..=24).rev() {
                let x = -r + k as f32 * r / 12.0;
                v.push((x, h + (x / r * PI).cos() * -r * 0.08));
            }
            v.push((-r + notch, 0.0));
            v
        }
        _ => {
            kind = Kind::Stroke { width: size / 40.0 };
            vec![(-r, 0.0), (r, 0.0)]
        }
    };
    let closed = !matches!(kind, Kind::Stroke { .. });
    Shape { kind, lines: vec![(pts.into_iter().map(|p| (p.0 + at.0, p.1 + at.1)).collect(), closed)], layer }
}

/// An empty artwork to build from the built-in shapes: a 100 mm frame and a
/// few starting colors.
pub fn blank() -> Artwork {
    let colors = [[0xd0, 0x20, 0x30], [0x1d, 0x2a, 0x5c], [0xe8, 0xb0, 0x20], [0x20, 0x20, 0x20], [0xf4, 0xf4, 0xf0], [0x2a, 0x8a, 0x3e]];
    Artwork {
        name: "Shapes".into(),
        shapes: Vec::new(),
        layers: colors.iter().map(|&color| Layer { color, background: false, shapes: 0 }).collect(),
        bounds: (0.0, 0.0, 377.95, 377.95),
        skipped_images: 0,
    }
}

/// SVG colors grouped by the thread they use, in order of first use.
pub fn thread_groups(art: &Artwork, s: &ArtSettings) -> Vec<(i32, Vec<usize>)> {
    let mut groups: Vec<(i32, Vec<usize>)> = Vec::new();
    for (i, l) in s.layers.iter().enumerate() {
        // Backgrounds stay on their own row so they can be switched off alone.
        let key = if art.layers[i].background { -l.thread } else { l.thread };
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    for g in &mut groups {
        g.0 = g.0.abs();
    }
    groups
}

/// Merges the closest threads until at most `max` remain, keeping the thread
/// used by more shapes in each merge.
pub fn reduce_threads(art: &Artwork, s: &mut ArtSettings, max: usize) {
    loop {
        let mut used: Vec<(i32, usize)> = Vec::new();
        for (i, l) in s.layers.iter().enumerate() {
            if !l.enabled {
                continue;
            }
            match used.iter_mut().find(|u| u.0 == l.thread) {
                Some(u) => u.1 += art.layers[i].shapes,
                None => used.push((l.thread, art.layers[i].shapes)),
            }
        }
        if used.len() <= max.max(1) {
            return;
        }
        let color = |t: i32| formats::jef_thread(t, 0).color;
        let d = |a: [u8; 3], b: [u8; 3]| {
            let (r, g, bb) = (a[0] as i32 - b[0] as i32, a[1] as i32 - b[1] as i32, a[2] as i32 - b[2] as i32);
            2 * r * r + 4 * g * g + 3 * bb * bb
        };
        let mut best = (i32::MAX, 0, 1);
        for i in 0..used.len() {
            for j in i + 1..used.len() {
                let dist = d(color(used[i].0), color(used[j].0));
                if dist < best.0 {
                    best = (dist, i, j);
                }
            }
        }
        let (keep, drop) = if used[best.1].1 >= used[best.2].1 { (used[best.1].0, used[best.2].0) } else { (used[best.2].0, used[best.1].0) };
        for l in &mut s.layers {
            if l.thread == drop {
                l.thread = keep;
            }
        }
    }
}

impl ArtSettings {
    pub fn for_artwork(art: &Artwork) -> Self {
        let width = (art.bounds.2 - art.bounds.0) * 25.4 / 96.0;
        // Start at the artwork's own size when it's sensible, else 80 mm wide.
        let width_mm = if (20.0..=180.0).contains(&width) { width.round() } else { 80.0 };
        ArtSettings {
            width_mm,
            style: Settings { edge: EdgeKind::None, underlay: true, fill_angle: 45.0, ..Default::default() },
            knockout: true,
            min_detail_mm: 1.0,
            stroke: EdgeKind::Satin,
            stroke_min_mm: 1.0,
            group_colors: true,
            layers: art
                .layers
                .iter()
                .map(|l| LayerChoice { thread: formats::nearest_jef(l.color), enabled: !l.background, look: Look::Default })
                .collect(),
            shapes: (0..art.shapes.len()).map(|i| ShapeEdit::of(art, i)).collect(),
        }
    }
}

fn flatten(path: &usvg::tiny_skia_path::Path, t: usvg::Transform) -> Vec<(Contour, bool)> {
    use usvg::tiny_skia_path::PathSegment;
    let map = |p: usvg::tiny_skia_path::Point| (t.sx * p.x + t.kx * p.y + t.tx, t.ky * p.x + t.sy * p.y + t.ty);
    let mut out = Vec::new();
    let mut cur: Contour = Vec::new();
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.clear();
                cur.push(map(p));
            }
            PathSegment::LineTo(p) => cur.push(map(p)),
            PathSegment::QuadTo(c, p) => {
                let p0 = *cur.last().unwrap_or(&map(p));
                let (c, p) = (map(c), map(p));
                for i in 1..=8 {
                    let k = i as f32 / 8.0;
                    let u = 1.0 - k;
                    cur.push((u * u * p0.0 + 2.0 * u * k * c.0 + k * k * p.0, u * u * p0.1 + 2.0 * u * k * c.1 + k * k * p.1));
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let p0 = *cur.last().unwrap_or(&map(p));
                let (c1, c2, p) = (map(c1), map(c2), map(p));
                for i in 1..=12 {
                    let k = i as f32 / 12.0;
                    let u = 1.0 - k;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * k, 3.0 * u * k * k, k * k * k);
                    cur.push((a * p0.0 + b * c1.0 + c * c2.0 + d * p.0, a * p0.1 + b * c1.1 + c * c2.1 + d * p.1));
                }
            }
            PathSegment::Close => {
                if cur.len() > 2 {
                    out.push((std::mem::take(&mut cur), true));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

fn paint_color(paint: &usvg::Paint) -> [u8; 3] {
    let rgb = |c: usvg::Color| [c.red, c.green, c.blue];
    match paint {
        usvg::Paint::Color(c) => rgb(*c),
        // Gradients become one thread in their average color.
        usvg::Paint::LinearGradient(g) => average(g.stops().iter().map(|s| rgb(s.color()))),
        usvg::Paint::RadialGradient(g) => average(g.stops().iter().map(|s| rgb(s.color()))),
        usvg::Paint::Pattern(_) => [128, 128, 128],
    }
}

fn average(colors: impl Iterator<Item = [u8; 3]>) -> [u8; 3] {
    let (mut sum, mut n) = ([0u32; 3], 0u32);
    for c in colors {
        for i in 0..3 {
            sum[i] += c[i] as u32;
        }
        n += 1;
    }
    let n = n.max(1);
    [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]
}

struct Collect {
    shapes: Vec<(Kind, Vec<(Contour, bool)>, [u8; 3])>,
    images: usize,
}

fn walk_group(g: &usvg::Group, out: &mut Collect) {
    for node in g.children() {
        match node {
            usvg::Node::Group(g) => walk_group(g, out),
            usvg::Node::Text(t) => walk_group(t.flattened(), out),
            usvg::Node::Image(_) => out.images += 1,
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let lines = flatten(p.data(), p.abs_transform());
                if lines.is_empty() {
                    continue;
                }
                let t = p.abs_transform();
                let scale = ((t.sx * t.sy - t.kx * t.ky).abs()).sqrt();
                let fill = p.fill().filter(|f| f.opacity().get() > 0.05).map(|f| {
                    (Kind::Fill { even_odd: f.rule() == usvg::FillRule::EvenOdd }, paint_color(f.paint()))
                });
                let stroke = p.stroke().filter(|s| s.opacity().get() > 0.05).map(|s| {
                    (Kind::Stroke { width: s.width().get() * scale }, paint_color(s.paint()))
                });
                let both = match p.paint_order() {
                    usvg::PaintOrder::FillAndStroke => [fill, stroke],
                    usvg::PaintOrder::StrokeAndFill => [stroke, fill],
                };
                for (kind, color) in both.into_iter().flatten() {
                    out.shapes.push((kind, lines.clone(), color));
                }
            }
        }
    }
}

pub fn load(path: &Path) -> Result<Artwork, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let data = std::fs::read(path).map_err(|e| format!("Couldn't open {name}: {e}."))?;
    parse(&name, &data)
}

pub fn parse(name: &str, data: &[u8]) -> Result<Artwork, String> {
    let mut opt = usvg::Options::default();
    // Text in the SVG is drawn with the installed fonts.
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(data, &opt).map_err(|e| format!("{name} isn't an SVG Hoop can read: {e}."))?;
    let mut c = Collect { shapes: Vec::new(), images: 0 };
    walk_group(tree.root(), &mut c);
    if c.shapes.is_empty() {
        return Err(format!(
            "{name} has no shapes to stitch.{}",
            if c.images > 0 { " It only holds pictures; trace them into paths first (Inkscape: Path → Trace Bitmap)." } else { "" }
        ));
    }

    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (_, lines, _) in &c.shapes {
        for p in lines.iter().flat_map(|l| &l.0) {
            b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
        }
    }
    let (bw, bh) = (b.2 - b.0, b.3 - b.1);

    let mut layers: Vec<Layer> = Vec::new();
    let mut shapes = Vec::new();
    for (i, (kind, lines, color)) in c.shapes.into_iter().enumerate() {
        // A filled rectangle under everything that spans the drawing is a
        // background, which usually shouldn't be stitched.
        let mut sb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in lines.iter().flat_map(|l| &l.0) {
            sb = (sb.0.min(p.0), sb.1.min(p.1), sb.2.max(p.0), sb.3.max(p.1));
        }
        let filled: f32 = lines.iter().map(|l| area(&l.0)).sum();
        let background = i == 0
            && matches!(kind, Kind::Fill { .. })
            && sb.2 - sb.0 >= bw * 0.97
            && sb.3 - sb.1 >= bh * 0.97
            && filled >= 0.9 * bw * bh;
        let layer = match layers.iter().position(|l| l.color == color && l.background == background) {
            Some(k) => k,
            None => {
                layers.push(Layer { color, background, shapes: 0 });
                layers.len() - 1
            }
        };
        layers[layer].shapes += 1;
        shapes.push(Shape { kind, lines, layer });
    }
    Ok(Artwork { name: name.to_string(), shapes, layers, bounds: b, skipped_images: c.images })
}

fn area(c: &Contour) -> f32 {
    let mut a = 0.0;
    for i in 0..c.len() {
        let (p, q) = (c[i], c[(i + 1) % c.len()]);
        a += p.0 * q.1 - q.0 * p.1;
    }
    (a / 2.0).abs()
}

fn length(c: &Contour) -> f32 {
    c.windows(2).map(|w| lettering::dist(w[0], w[1])).sum()
}

fn segment_dist(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    lettering::dist(p, (a.0 + dx * t, a.1 + dy * t))
}

/// Whether `p` lies inside a filled shape or within `margin` of its edge.
fn covered(p: Pt, shape: &[Contour], even_odd: bool, margin: f32) -> bool {
    let mut winding = 0;
    for c in shape {
        for k in 0..c.len() {
            let (a, b) = (c[k], c[(k + 1) % c.len()]);
            if segment_dist(p, a, b) < margin {
                return true;
            }
            if (a.1 <= p.1) != (b.1 <= p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                winding += if b.1 > a.1 { 1 } else { -1 };
            }
        }
    }
    if even_odd { winding % 2 != 0 } else { winding != 0 }
}

/// The parts of a line not covered by any of `shapes`, as open lines,
/// short of each shape by `margin`, and the same parts for sewing: there
/// each cut end runs on to the shape's edge (at most 1.5 × `margin` along
/// the line), so the line meets the shape as in the drawing. Pieces
/// shorter than 1 mm are dropped.
fn clip_line(line: &(Contour, bool), shapes: &[(Vec<Contour>, bool)], margin: f32) -> (Vec<(Contour, bool)>, Vec<(Contour, bool)>) {
    let (c, closed) = line;
    let b = extent(std::slice::from_ref(line));
    let near: Vec<&(Vec<Contour>, bool)> = shapes
        .iter()
        .filter(|(cs, _)| {
            let e = extent(&cs.iter().map(|c| (c.clone(), true)).collect::<Vec<_>>());
            e.0 - margin <= b.2 && b.0 <= e.2 + margin && e.1 - margin <= b.3 && b.1 <= e.3 + margin
        })
        .collect();
    if near.is_empty() {
        return (vec![line.clone()], vec![line.clone()]);
    }
    let mut path = c.clone();
    if *closed {
        path.push(c[0]);
    }
    // Test points every 0.2 mm: hidden (within the margin) and inside a shape.
    let test = |p: Pt| {
        let inside = near.iter().any(|(cs, eo)| covered(p, cs, *eo, 0.0));
        (p, inside || near.iter().any(|(cs, eo)| covered(p, cs, *eo, margin)), inside)
    };
    let mut pts: Vec<(Pt, bool, bool)> = Vec::new();
    for w in path.windows(2) {
        let n = (lettering::dist(w[0], w[1]) / 2.0).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = k as f32 / n as f32;
            pts.push(test((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t)));
        }
    }
    if let Some(&l) = path.last() {
        pts.push(test(l));
    }
    if pts.iter().all(|p| !p.1) {
        return (vec![line.clone()], vec![line.clone()]);
    }
    // A closed line that is cut starts at a cut, so its pieces don't break
    // where the loop happened to begin.
    if *closed {
        let first = pts.iter().position(|p| p.1).unwrap();
        pts.pop();
        pts.rotate_left(first);
    }
    let n = pts.len();
    let step = |k: usize, back: bool| -> Option<usize> {
        match (back, *closed) {
            (true, _) if k > 0 => Some(k - 1),
            (true, true) => Some(n - 1),
            (false, _) if k + 1 < n => Some(k + 1),
            (false, true) => Some(0),
            _ => None,
        }
    };
    // Hidden points from a cut end on toward the shape, stopping inside it.
    let reach = |from: usize, back: bool| -> Contour {
        let (mut out, mut k, mut went) = (Vec::new(), from, 0.0);
        while let Some(j) = step(k, back) {
            if !pts[j].1 || pts[j].2 {
                break;
            }
            went += lettering::dist(pts[k].0, pts[j].0);
            if went > 1.5 * margin {
                break;
            }
            out.push(pts[j].0);
            k = j;
        }
        out
    };
    let (mut kept, mut sewn) = (Vec::new(), Vec::new());
    let mut k = 0;
    while k < n {
        if pts[k].1 {
            k += 1;
            continue;
        }
        let start = k;
        while k < n && !pts[k].1 {
            k += 1;
        }
        let piece: Contour = pts[start..k].iter().map(|p| p.0).collect();
        if length(&piece) < 10.0 {
            continue;
        }
        let mut full: Contour = reach(start, true).into_iter().rev().collect();
        full.extend_from_slice(&piece);
        full.extend(reach(k - 1, false));
        kept.push((crate::trace::simplify(&piece, 0.5), false));
        sewn.push((crate::trace::simplify(&full, 0.5), false));
    }
    (kept, sewn)
}

/// Stitch order that saves thread changes without breaking the drawing's
/// layers. Shapes come in drawing order (bottom first). A shape may move
/// ahead to join its thread only past shapes it doesn't touch: anything it
/// overlaps that lies below it must be sewn first, or it would bury it.
fn layered_order(threads: &[i32], touch: impl Fn(usize, usize) -> bool) -> Vec<usize> {
    let n = threads.len();
    // above[i]: shapes lying on i, which must wait for it.
    let mut above: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut below = vec![0usize; n];
    for j in 0..n {
        for i in 0..j {
            if touch(i, j) {
                below[j] += 1;
                above[i].push(j);
            }
        }
    }
    let mut state = Sewing { below, ready: Vec::new(), current: None };
    state.ready = (0..n).filter(|&j| state.below[j] == 0).collect();
    // Two levels of lookahead find the fewest changes on every design tried;
    // big traced pictures get one to stay quick.
    let depth = if n <= 300 { 2 } else { 1 };
    let mut order = Vec::with_capacity(n);
    state.run(threads, &above, &mut order, depth);
    order
}

/// Where `layered_order` is: shapes still waiting on others, shapes free to
/// sew (sorted), and the thread on the machine.
#[derive(Clone)]
struct Sewing {
    below: Vec<usize>,
    ready: Vec<usize>,
    current: Option<i32>,
}

impl Sewing {
    /// Sews one shape: the lowest ready one in the current thread, else the
    /// lowest ready one. Returns whether the thread changed.
    fn step(&mut self, threads: &[i32], above: &[Vec<usize>], order: &mut Vec<usize>) -> bool {
        let k = self.ready.iter().position(|&j| Some(threads[j]) == self.current).unwrap_or(0);
        let i = self.ready.remove(k);
        let changed = self.current != Some(threads[i]);
        self.current = Some(threads[i]);
        order.push(i);
        for &j in &above[i] {
            self.below[j] -= 1;
            if self.below[j] == 0 {
                let at = self.ready.partition_point(|&r| r < j);
                self.ready.insert(at, j);
            }
        }
        changed
    }

    /// Sews everything left; returns the thread changes. Whenever the
    /// current thread runs out, it tries each thread that could come next,
    /// sewing the rest with one level less of lookahead, and takes the one
    /// with the fewest changes (ties go to the lowest shape's thread).
    fn run(&mut self, threads: &[i32], above: &[Vec<usize>], order: &mut Vec<usize>, depth: usize) -> usize {
        let mut changes = 0;
        while !self.ready.is_empty() {
            if depth > 0 && !self.ready.iter().any(|&j| Some(threads[j]) == self.current) {
                let mut options: Vec<i32> = Vec::new();
                for &j in &self.ready {
                    if !options.contains(&threads[j]) {
                        options.push(threads[j]);
                    }
                }
                let mut best = (usize::MAX, options[0]);
                for &t in &options {
                    let mut trial = self.clone();
                    trial.current = Some(t);
                    let n = trial.run(threads, above, &mut Vec::new(), depth - 1);
                    if n < best.0 {
                        best = (n, t);
                    }
                }
                changes += self.current.is_some() as usize;
                self.current = Some(best.1);
            }
            changes += self.step(threads, above, order) as usize;
        }
        changes
    }
}

/// Cells of a 0.5 mm grid a shape covers, for telling which shapes overlap.
/// Fills cover their inside and outline, lines their stitched width.
/// Units are 0.1 mm.
struct Coverage {
    bounds: (i32, i32, i32, i32),
    cells: HashSet<(i32, i32)>,
}

impl Coverage {
    const CELL: f32 = 5.0;

    fn new(lines: &[(Contour, bool)], fill: Option<bool>, width: f32) -> Self {
        let cell = |v: f32| (v / Self::CELL).floor() as i32;
        let mut cells = HashSet::new();
        let r = cell(width / 2.0 + Self::CELL / 2.0).max(0);
        for (c, closed) in lines {
            let n = if *closed || fill.is_some() { c.len() } else { c.len().saturating_sub(1) };
            for i in 0..n {
                let (p, q) = (c[i], c[(i + 1) % c.len()]);
                let steps = (lettering::dist(p, q) / (Self::CELL / 2.0)).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = k as f32 / steps as f32;
                    let (x, y) = (cell(p.0 + (q.0 - p.0) * t), cell(p.1 + (q.1 - p.1) * t));
                    for dy in -r..=r {
                        for dx in -r..=r {
                            cells.insert((x + dx, y + dy));
                        }
                    }
                }
            }
        }
        if let Some(even_odd) = fill {
            let contours: Vec<Contour> = lines.iter().map(|l| l.0.clone()).collect();
            let b = extent(lines);
            for row in cell(b.1)..=cell(b.3) {
                let y = (row as f32 + 0.5) * Self::CELL;
                for (x0, x1) in lettering::row_spans(&contours, y, even_odd) {
                    for col in cell(x0)..=cell(x1) {
                        cells.insert((col, row));
                    }
                }
            }
        }
        let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for &(x, y) in &cells {
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
        }
        Coverage { bounds, cells }
    }

    fn overlaps(&self, o: &Coverage) -> bool {
        let (p, q) = (self.bounds, o.bounds);
        if p.0 > q.2 || q.0 > p.2 || p.1 > q.3 || q.1 > p.3 {
            return false;
        }
        let (small, big) = if self.cells.len() < o.cells.len() { (self, o) } else { (o, self) };
        small.cells.iter().any(|c| big.cells.contains(c))
    }
}

pub fn generate(art: &Artwork, s: &ArtSettings) -> Result<Pattern, String> {
    let (scale, (cx, cy)) = frame(art, s);
    let to_mm = |p: Pt| ((p.0 - cx) * scale, (p.1 - cy) * scale);
    let enabled = |layer: usize| s.layers.get(layer).is_none_or(|l| l.enabled);

    // Shapes in stitch space with tiny specks dropped. Detail size is the
    // side of a square, so 1 mm drops anything smaller than 1 mm².
    let min_area = (s.min_detail_mm * 10.0).powi(2);
    struct Item {
        doc: usize,
        layer: usize,
        look: Look,
        kind: Kind,
        lines: Vec<(Contour, bool)>,
        /// Lines as sewn, when cutting made them differ from `lines`.
        sew: Option<Vec<(Contour, bool)>>,
    }
    let mut items: Vec<Item> = Vec::new();
    for (doc, e) in s.shapes.iter().enumerate() {
        let sh = e.base(art);
        if e.hidden || !enabled(e.layer) {
            continue;
        }
        let lines: Vec<(Contour, bool)> = e.lines(art).iter().map(|(c, closed)| (c.iter().map(|&p| to_mm(p)).collect(), *closed)).collect();
        let keep = match sh.kind {
            Kind::Fill { .. } => lines.iter().map(|l| area(&l.0)).sum::<f32>() >= min_area,
            Kind::Stroke { .. } => lines.iter().map(|l| length(&l.0)).sum::<f32>() >= 10.0,
        };
        if keep {
            let look = e.look.unwrap_or_else(|| s.layers.get(e.layer).map_or(Look::Default, |l| l.look));
            items.push(Item { doc, layer: e.layer, look, kind: sh.kind.clone(), lines, sew: None });
        }
    }
    if items.is_empty() {
        return Err(if s.shapes.iter().all(|e| e.hidden) {
            "No shapes yet. Add one from Shapes, or open an SVG or a picture.".into()
        } else {
            "Every color is turned off or too small to stitch.".into()
        });
    }

    // Stitch order: by color in order of first appearance, or the drawing's
    // own order, where each color change costs a thread change.
    let thread_of = |layer: usize| s.layers.get(layer).map_or_else(|| formats::nearest_jef(art.layers[layer].color), |l| l.thread);
    // The fill and edge each shape gets after its thread's look is applied.
    let styled = |look: Look| -> Settings {
        let mut style = s.style.clone();
        match look {
            Look::Default => {}
            Look::Fill(k) => {
                if k != style.fill {
                    lettering::style(&mut style, k, s.style.fill_angle, s.style.edge);
                }
            }
            Look::Outline | Look::Applique => {
                style.fill = FillKind::None;
                if style.edge == EdgeKind::None {
                    style.edge = EdgeKind::Triple;
                }
            }
        }
        style
    };
    // Shapes stacked on top of a fill, cut out of it when knockout is on.
    let masks = |it: &Item| matches!(it.kind, Kind::Fill { .. }) && (styled(it.look).fill != FillKind::None || it.look == Look::Applique);
    // Lines hidden under fills drawn above them are cut away, a little past
    // the fill's edge, so they neither show through nor have to be sewn
    // before that fill (which would cost extra thread changes).
    if s.knockout {
        let stroke_w = |width: f32| (width * scale).max(s.stroke_min_mm * 10.0).min(70.0);
        for i in 0..items.len() {
            let Kind::Stroke { width } = items[i].kind else { continue };
            let above: Vec<(Vec<Contour>, bool)> = items
                .iter()
                .filter(|o| o.doc > items[i].doc && masks(o))
                .map(|o| (o.lines.iter().map(|l| l.0.clone()).collect(), matches!(o.kind, Kind::Fill { even_odd: true })))
                .collect();
            if !above.is_empty() {
                // Past the half width, a gap of a cell and a half keeps the line
                // clear of the fill on the overlap grid used for ordering.
                let margin = stroke_w(width) / 2.0 + 1.5 * Coverage::CELL;
                let (mut kept, mut sewn) = (Vec::new(), Vec::new());
                for l in &items[i].lines {
                    let (k, s) = clip_line(l, &above, margin);
                    kept.extend(k);
                    sewn.extend(s);
                }
                items[i].lines = kept;
                items[i].sew = Some(sewn);
            }
        }
        items.retain(|it| !it.lines.is_empty());
    }
    let order: Vec<usize> = if s.group_colors {
        let threads: Vec<i32> = items.iter().map(|it| thread_of(it.layer)).collect();
        let cover: Vec<Coverage> = items
            .iter()
            .map(|it| match it.kind {
                Kind::Fill { even_odd } => Coverage::new(&it.lines, Some(even_odd), 0.0),
                Kind::Stroke { width } => Coverage::new(&it.lines, None, (width * scale).max(s.stroke_min_mm * 10.0).min(70.0)),
            })
            .collect();
        // Anything a shape overlaps that lies below it is sewn first, so
        // the top shape's edges lie over it, as in the drawing.
        layered_order(&threads, |a, b| cover[a].overlaps(&cover[b]))
    } else {
        (0..items.len()).collect()
    };

    let mut st = Stitcher::new();
    let mut threads = Vec::new();
    let mut current: Option<i32> = None;
    for &i in &order {
        let it = &items[i];
        let thread = thread_of(it.layer);
        let applique = matches!(it.kind, Kind::Fill { .. }) && it.look == Look::Applique;
        if applique {
            // Two machine stops even within one thread: lay the fabric, then trim it.
            let outlines: Vec<Contour> = it.lines.iter().map(|l| l.0.clone()).collect();
            for (name, passes) in [("Placement line", 1), ("Tack-down (lay the fabric first)", 2)] {
                if !threads.is_empty() {
                    st.color_change();
                }
                let mut t = formats::jef_thread(thread, threads.len());
                t.name = name.to_string();
                t.code = String::new();
                threads.push(t);
                for _ in 0..passes {
                    for c in &outlines {
                        lettering::run_contour(&mut st, c, 20.0, false);
                    }
                }
            }
            st.color_change();
            threads.push(formats::jef_thread(thread, threads.len()));
            current = Some(thread);
            let width = s.style.edge_width_mm.max(2.5) * 10.0;
            for c in &outlines {
                lettering::satin_contour(&mut st, c, width);
            }
            continue;
        }
        if current != Some(thread) {
            if current.is_some() {
                st.color_change();
            }
            threads.push(formats::jef_thread(thread, threads.len()));
            current = Some(thread);
        }
        match it.kind {
            Kind::Fill { even_odd } => {
                let own = styled(it.look);
                let st_ref = &own;
                let contours: Vec<Contour> = it.lines.iter().map(|l| l.0.clone()).collect();
                // Everything drawn above this shape and overlapping it, and of
                // that, what sews densely enough to hide travel under it.
                let above = |dense: bool| -> Vec<lettering::MaskShape> {
                    if !s.knockout {
                        return Vec::new();
                    }
                    items
                        .iter()
                        .filter(|o| o.doc > it.doc && masks(o))
                        .filter(|o| !dense || o.look == Look::Applique || lettering::covers(styled(o.look).fill, styled(o.look).density_mm))
                        .map(|o| (o.lines.iter().map(|l| l.0.clone()).collect(), matches!(o.kind, Kind::Fill { even_odd: true })))
                        .collect()
                };
                let mask = above(false);
                st.region = crate::route::Region::new(&contours, even_odd, &mask, &above(true));
                if st_ref.fill != FillKind::None {
                    if st_ref.underlay {
                        lettering::underlay(&mut st, &contours, even_odd, &mask, st_ref.fill, st_ref.fill_angle, st_ref);
                    }
                    lettering::fill_shape(&mut st, &contours, even_odd, &mask, st_ref.fill, st_ref.fill_angle, st_ref);
                    if st_ref.fill == FillKind::Crosshatch {
                        lettering::fill_shape(&mut st, &contours, even_odd, &mask, st_ref.fill, st_ref.fill_angle + 90.0, st_ref);
                    }
                }
                for c in &contours {
                    match st_ref.edge {
                        EdgeKind::None => {}
                        EdgeKind::Running => lettering::run_contour(&mut st, c, 25.0, false),
                        EdgeKind::Triple => lettering::run_contour(&mut st, c, 25.0, true),
                        EdgeKind::Satin => lettering::satin_contour(&mut st, c, st_ref.edge_width_mm * 10.0),
                    }
                }
                st.region = None;
            }
            Kind::Stroke { width } => {
                let w = (width * scale).max(s.stroke_min_mm * 10.0).min(70.0);
                for (c, closed) in it.sew.as_ref().unwrap_or(&it.lines) {
                    let mut line = c.clone();
                    if *closed {
                        line.push(c[0]);
                    }
                    match s.stroke {
                        EdgeKind::None => {}
                        EdgeKind::Running => lettering::run_path(&mut st, &line, 25.0, false),
                        EdgeKind::Triple => lettering::run_path(&mut st, &line, 25.0, true),
                        EdgeKind::Satin => lettering::satin_path(&mut st, &line, w),
                    }
                }
            }
        }
    }
    st.finish();
    if !st.cmds.contains(&Cmd::Stitch) {
        return Err("Nothing to stitch with these settings. Turn on a fill or a line style.".into());
    }
    Ok(Pattern { format: Format::Svg, points: st.points, cmds: st.cmds, threads })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
        <rect width="100" height="50" fill="#ffffff"/>
        <rect x="10" y="10" width="40" height="30" fill="#ff0000"/>
        <circle cx="45" cy="25" r="10" fill="#0000ff"/>
        <path d="M60 10 L90 40" stroke="#000000" stroke-width="2" fill="none"/>
    </svg>"##;

    #[test]
    fn colors_become_layers_and_background_is_off() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        assert_eq!(art.layers.len(), 4);
        assert!(art.layers[0].background);
        let s = ArtSettings::for_artwork(&art);
        assert!(!s.layers[0].enabled);
        assert_eq!(s.layers[1].thread, formats::nearest_jef([255, 0, 0]));
    }

    #[test]
    fn generates_one_thread_per_color_at_the_chosen_width() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.width_mm = 100.0;
        let p = generate(&art, &s).unwrap();
        assert_eq!(p.blocks().len(), 3);
        let b = p.bounds();
        // Background is off, so the art spans from the red rect to the line's end.
        assert!((b.2 - b.0) / 10.0 > 75.0 && (b.2 - b.0) / 10.0 < 90.0, "{:?}", b);
    }

    #[test]
    fn knockout_leaves_covered_area_unstitched() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.stroke = EdgeKind::None;
        s.style.underlay = false;
        s.width_mm = 100.0;
        // Red stitches well inside the blue circle, which sits on top of the red square.
        let inside = |p: &Pattern| {
            let red = &p.blocks()[0];
            (red.start..red.end).filter(|&i| p.cmds[i] == Cmd::Stitch && lettering::dist(p.points[i], (-50.0, 0.0)) < 70.0).count()
        };
        let with = inside(&generate(&art, &s).unwrap());
        s.knockout = false;
        let without = inside(&generate(&art, &s).unwrap());
        assert!(with * 10 < without, "{with} vs {without}");
    }

    #[test]
    fn lines_under_later_fills_are_cut_away() {
        // A line across a square drawn on top of it: only the parts outside
        // the square, short of its edge by the margin, are left.
        let square = vec![(vec![vec![(100.0, -50.0), (200.0, -50.0), (200.0, 50.0), (100.0, 50.0)]], false)];
        let (pieces, sewn) = clip_line(&(vec![(0.0, 0.0), (300.0, 0.0)], false), &square, 10.0);
        assert_eq!(pieces.len(), 2);
        let ends: Vec<f32> = pieces.iter().flat_map(|(c, _)| [c[0].0, c[c.len() - 1].0]).collect();
        assert!((ends[0] - 0.0).abs() < 1.0 && (ends[1] - 90.0).abs() < 3.0, "{ends:?}");
        assert!((ends[2] - 210.0).abs() < 3.0 && (ends[3] - 300.0).abs() < 1.0, "{ends:?}");
        // As sewn, the pieces run on to the square's edge.
        let ends: Vec<f32> = sewn.iter().flat_map(|(c, _)| [c[0].0, c[c.len() - 1].0]).collect();
        assert!((ends[1] - 100.0).abs() < 3.0 && (ends[2] - 200.0).abs() < 3.0, "{ends:?}");
        // A loop around the square is left whole; a loop inside it is gone.
        let around = (vec![(50.0, -100.0), (250.0, -100.0), (250.0, 100.0), (50.0, 100.0)], true);
        assert_eq!(clip_line(&around, &square, 10.0).0, vec![around.clone()]);
        assert!(clip_line(&(vec![(130.0, -20.0), (170.0, -20.0), (170.0, 20.0), (130.0, 20.0)], true), &square, 10.0).1.is_empty());
    }

    #[test]
    fn outlines_hidden_in_the_drawing_are_not_sewn() {
        // A black-outlined red square under a blue circle: no black stitch
        // may land well inside the circle, and black still sews in one run.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
            <rect x="10" y="10" width="40" height="40" fill="#ff0000" stroke="#000000" stroke-width="1"/>
            <circle cx="50" cy="30" r="12" fill="#0000ff" stroke="#000000" stroke-width="1"/>
        </svg>"##;
        let art = parse("hidden.svg", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.width_mm = 100.0;
        let p = generate(&art, &s).unwrap();
        // The circle's middle in stitch space: design x 50 of 10..62 wide art.
        let (scale, (cx, cy)) = frame(&art, &s);
        let mid = ((50.0 - cx) * scale, (30.0 - cy) * scale);
        let r = 12.0 * scale;
        let black: Vec<_> = p.blocks().into_iter().filter(|b| b.thread.color == [0, 0, 0]).collect();
        assert_eq!(black.len(), 1, "black should sew once");
        let b = &black[0];
        let hidden = (b.start..b.end).filter(|&i| p.cmds[i] == Cmd::Stitch && lettering::dist(p.points[i], mid) < r - 15.0).count();
        assert_eq!(hidden, 0);
    }

    #[test]
    fn grouping_never_sews_a_shape_under_one_below_it() {
        // Black silhouette, white key on it, black detail on the key, and a
        // black mark far away. The detail must wait for the white; the far
        // mark may join the first black run.
        let sq = |x: f32, y: f32, w: f32| vec![(vec![(x, y), (x + w, y), (x + w, y + w), (x, y + w)], true)];
        let cover: Vec<Coverage> = [sq(0.0, 0.0, 100.0), sq(10.0, 10.0, 80.0), sq(40.0, 40.0, 20.0), sq(500.0, 0.0, 20.0)]
            .iter()
            .map(|l| Coverage::new(l, Some(false), 0.0))
            .collect();
        let threads = [1, 2, 1, 1];
        assert_eq!(layered_order(&threads, |a, b| cover[a].overlaps(&cover[b])), vec![0, 3, 1, 2]);
        // A thin line inside a ring's hole doesn't touch the ring.
        let ring = vec![(vec![(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)], true), (vec![(50.0, 50.0), (50.0, 250.0), (250.0, 250.0), (250.0, 50.0)], true)];
        let line = vec![(vec![(100.0, 150.0), (200.0, 150.0)], false)];
        assert!(!Coverage::new(&ring, Some(false), 0.0).overlaps(&Coverage::new(&line, None, 5.0)));
    }

    #[test]
    fn shared_threads_need_no_thread_change_and_reduce_merges() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.layers[2].thread = s.layers[1].thread;
        assert_eq!(generate(&art, &s).unwrap().blocks().len(), 2);
        let mut s = ArtSettings::for_artwork(&art);
        reduce_threads(&art, &mut s, 1);
        assert_eq!(generate(&art, &s).unwrap().blocks().len(), 1);
        assert_eq!(thread_groups(&art, &s).len(), 2); // background stays apart
    }

    #[test]
    fn a_thread_can_be_outline_only() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.stroke = EdgeKind::None;
        let filled = generate(&art, &s).unwrap().blocks()[0].stitches;
        s.layers[1].look = Look::Outline;
        let outline = generate(&art, &s).unwrap().blocks()[0].stitches;
        assert!(outline * 2 < filled, "{outline} vs {filled}");
    }

    #[test]
    fn applique_thread_stops_twice_and_covers_with_satin() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.layers[1].look = Look::Applique;
        let p = generate(&art, &s).unwrap();
        assert_eq!(p.threads[0].name, "Placement line");
        assert!(p.threads[1].name.starts_with("Tack-down"));
        assert_eq!(p.blocks().len(), 5);
    }

    #[test]
    fn moving_hiding_and_reordering_shapes() {
        let art = parse("logo", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        let before = generate(&art, &s).unwrap().bounds();
        // Move the red square 20 user units left: the design grows that way.
        let red = s.shapes.iter().position(|e| art.layers[e.layer].color == [255, 0, 0]).unwrap();
        s.shapes[red].dx = -20.0;
        let moved = generate(&art, &s).unwrap().bounds();
        assert!(moved.0 < before.0 - 50.0, "{moved:?} vs {before:?}");
        // The point under the moved square now hits it; the old spot hits nothing red.
        let (scale, mid) = frame(&art, &s);
        let at = |u: Pt| ((u.0 - mid.0) * scale, (u.1 - mid.1) * scale);
        assert_eq!(shape_at(&art, &s, at((5.0, 25.0)), 1.0), Some(red));
        assert_ne!(shape_at(&art, &s, at((40.0, 15.0)), 1.0), Some(red));
        // Hidden shapes are skipped, both for stitching and picking.
        s.shapes[red].hidden = true;
        assert_eq!(shape_at(&art, &s, at((5.0, 25.0)), 1.0), None);
        let threads = generate(&art, &s).unwrap().threads.len();
        assert_eq!(threads, 2);
        // Baking keeps the outline and resets the move.
        s.shapes[red].hidden = false;
        s.shapes[red].rotate = 90.0;
        let lines = s.shapes[red].lines(&art);
        s.shapes[red].bake(&art);
        assert!(!s.shapes[red].moved());
        assert_eq!(s.shapes[red].lines(&art), lines);
    }

    #[test]
    fn blank_artwork_takes_built_in_shapes() {
        let art = blank();
        let mut s = ArtSettings::for_artwork(&art);
        assert_eq!(s.width_mm, 100.0);
        assert!(generate(&art, &s).unwrap_err().starts_with("No shapes"));
        for (k, name) in BUILT_IN.iter().enumerate() {
            let sh = built_in(name, (189.0, 189.0), 150.0, k % art.layers.len());
            let mut e = ShapeEdit::of(&Artwork { shapes: vec![sh.clone()], ..blank() }, 0);
            e.own = Some(sh);
            e.source = usize::MAX;
            s.shapes.push(e);
        }
        let p = generate(&art, &s).unwrap();
        let b = p.bounds();
        // 150 user units of a 378-unit, 100 mm frame is about 40 mm.
        assert!((b.2 - b.0) > 350.0 && (b.2 - b.0) < 450.0, "{b:?}");
        assert!(p.threads.len() >= 6);
    }

    #[test]
    fn reports_files_without_shapes() {
        let empty = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;
        assert!(parse("e.svg", empty.as_bytes()).is_err());
        assert!(parse("x.svg", b"not svg").is_err());
    }
}

#[cfg(test)]
mod samples {
    use super::*;

    /// Converts the SVGs listed in HOOP_SVGS (colon separated) to JEF in HOOP_OUT:
    /// HOOP_OUT=/tmp/dir HOOP_SVGS=a.svg:b.svg cargo test svg_samples -- --ignored
    #[test]
    #[ignore]
    fn svg_samples() {
        let (Some(out), Some(list)) = (std::env::var_os("HOOP_OUT"), std::env::var("HOOP_SVGS").ok()) else { return };
        for path in list.split(':') {
            let art = load(Path::new(path)).unwrap();
            let s = ArtSettings::for_artwork(&art);
            let t = std::time::Instant::now();
            let p = generate(&art, &s).unwrap();
            eprintln!("{path}: {} layers, {} threads, {} stitches, {:?}", art.layers.len(), p.threads.len(), p.count(Cmd::Stitch), t.elapsed());
            let threads: Vec<i32> = p.threads.iter().map(|t| formats::nearest_jef(t.color)).collect();
            let b = p.bounds();
            let pts: Vec<Pt> = p.points.iter().map(|&(x, y)| (x - (b.0 + b.2) / 2.0, y - (b.1 + b.3) / 2.0)).collect();
            let stem = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
            std::fs::write(Path::new(&out).join(format!("svg_{stem}.jef")), formats::write_jef(&pts, &p.cmds, &threads)).unwrap();
        }
    }
}

/// One-click looks for artwork, changing only how shapes are stitched.
pub const PRESETS: [(&str, &str, fn(&mut ArtSettings)); 4] = [
    ("Solid", "Dense tatami, hidden parts cut away, satin lines", |s| {
        lettering::style(&mut s.style, FillKind::Tatami, 45.0, EdgeKind::None);
        s.knockout = true;
        s.stroke = EdgeKind::Satin;
    }),
    ("Patch", "Dense fill with a satin border on every shape", |s| {
        lettering::style(&mut s.style, FillKind::Tatami, 45.0, EdgeKind::Satin);
        s.style.edge_width_mm = 1.4;
        s.knockout = true;
        s.stroke = EdgeKind::Satin;
    }),
    ("Light", "Open fill with a running outline, soft on thin fabric", |s| {
        lettering::style(&mut s.style, FillKind::Open, 45.0, EdgeKind::Running);
        s.stroke = EdgeKind::Running;
    }),
    ("Line art", "Outlines only, no fills", |s| {
        lettering::style(&mut s.style, FillKind::None, 45.0, EdgeKind::Triple);
        s.stroke = EdgeKind::Triple;
    }),
];


