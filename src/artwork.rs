//! Turns SVG artwork into stitches. usvg resolves the file into plain paths
//! with absolute transforms; filled paths become fill regions, stroked paths
//! become running or satin lines, and every color becomes one thread.

use std::path::Path;

use crate::formats::{self, Cmd, Format, Pattern};
use crate::lettering::{self, Contour, EdgeKind, FillKind, Pt, Settings, Stitcher};

#[derive(Clone, Debug)]
pub enum Kind {
    Fill { even_odd: bool },
    Stroke { width: f32 },
}

/// One painted path. Coordinates are SVG user units, y down.
#[derive(Clone, Debug)]
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

pub fn generate(art: &Artwork, s: &ArtSettings) -> Result<Pattern, String> {
    let scale = s.width_mm * 10.0 / (art.bounds.2 - art.bounds.0).max(1e-3);
    let (cx, cy) = ((art.bounds.0 + art.bounds.2) / 2.0, (art.bounds.1 + art.bounds.3) / 2.0);
    let to_mm = |p: Pt| ((p.0 - cx) * scale, (p.1 - cy) * scale);
    let enabled = |layer: usize| s.layers.get(layer).is_none_or(|l| l.enabled);

    // Shapes in stitch space with tiny specks dropped. Detail size is the
    // side of a square, so 1 mm drops anything smaller than 1 mm².
    let min_area = (s.min_detail_mm * 10.0).powi(2);
    struct Item {
        doc: usize,
        layer: usize,
        kind: Kind,
        lines: Vec<(Contour, bool)>,
    }
    let mut items: Vec<Item> = Vec::new();
    for (doc, sh) in art.shapes.iter().enumerate() {
        if !enabled(sh.layer) {
            continue;
        }
        let lines: Vec<(Contour, bool)> = sh.lines.iter().map(|(c, closed)| (c.iter().map(|&p| to_mm(p)).collect(), *closed)).collect();
        let keep = match sh.kind {
            Kind::Fill { .. } => lines.iter().map(|l| area(&l.0)).sum::<f32>() >= min_area,
            Kind::Stroke { .. } => lines.iter().map(|l| length(&l.0)).sum::<f32>() >= 10.0,
        };
        if keep {
            items.push(Item { doc, layer: sh.layer, kind: sh.kind.clone(), lines });
        }
    }
    if items.is_empty() {
        return Err("Every color is turned off or too small to stitch.".into());
    }

    // Stitch order: by color in order of first appearance, or the drawing's
    // own order, where each color change costs a thread change.
    let thread_of = |layer: usize| s.layers.get(layer).map_or_else(|| formats::nearest_jef(art.layers[layer].color), |l| l.thread);
    let mut order: Vec<usize> = (0..items.len()).collect();
    if s.group_colors {
        // Several SVG colors can share a thread; they are stitched together.
        let mut first: Vec<(i32, usize)> = Vec::new();
        for it in &items {
            let t = thread_of(it.layer);
            if !first.iter().any(|f| f.0 == t) {
                first.push((t, first.len()));
            }
        }
        let rank = |t: i32| first.iter().find(|f| f.0 == t).map_or(usize::MAX, |f| f.1);
        order.sort_by_key(|&i| (rank(thread_of(items[i].layer)), items[i].doc));
    }

    // The fill and edge each shape gets after its thread's look is applied.
    let look_of = |layer: usize| s.layers.get(layer).map_or(Look::Default, |l| l.look);
    let styled = |layer: usize| -> Settings {
        let mut style = s.style.clone();
        match look_of(layer) {
            Look::Default => {}
            Look::Fill(k) => {
                if k != style.fill {
                    lettering::style(&mut style, k, s.style.fill_angle, s.style.edge);
                }
            }
            Look::Outline => {
                style.fill = FillKind::None;
                if style.edge == EdgeKind::None {
                    style.edge = EdgeKind::Triple;
                }
            }
        }
        style
    };
    let mut st = Stitcher::new();
    let mut threads = Vec::new();
    let mut current: Option<i32> = None;
    for &i in &order {
        let it = &items[i];
        let thread = thread_of(it.layer);
        if current != Some(thread) {
            if current.is_some() {
                st.color_change();
            }
            threads.push(formats::jef_thread(thread, threads.len()));
            current = Some(thread);
        }
        match it.kind {
            Kind::Fill { even_odd } => {
                let own = styled(it.layer);
                let st_ref = &own;
                let contours: Vec<Contour> = it.lines.iter().map(|l| l.0.clone()).collect();
                // Everything drawn above this shape and overlapping it.
                let mask: Vec<Contour> = if s.knockout {
                    items
                        .iter()
                        .filter(|o| o.doc > it.doc && matches!(o.kind, Kind::Fill { .. }) && styled(o.layer).fill != FillKind::None)
                        .flat_map(|o| o.lines.iter().map(|l| l.0.clone()))
                        .collect()
                } else {
                    Vec::new()
                };
                if st_ref.fill != FillKind::None {
                    if st_ref.underlay {
                        for c in &contours {
                            lettering::run_contour(&mut st, c, 25.0, false);
                        }
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
            }
            Kind::Stroke { width } => {
                let w = (width * scale).max(s.stroke_min_mm * 10.0).min(80.0);
                for (c, closed) in &it.lines {
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
            eprintln!("{path}: {} layers, {} stitches, {:?}", art.layers.len(), p.count(Cmd::Stitch), t.elapsed());
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
