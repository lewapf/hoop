//! Reading SVG files: usvg resolves them into plain paths with absolute
//! transforms; fills and strokes become shapes, each color a layer.

use super::*;

pub(super) fn flatten(path: &usvg::tiny_skia_path::Path, t: usvg::Transform) -> Vec<(Contour, bool)> {
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

pub(super) fn paint_color(paint: &usvg::Paint) -> [u8; 3] {
    let rgb = |c: usvg::Color| [c.red, c.green, c.blue];
    match paint {
        usvg::Paint::Color(c) => rgb(*c),
        // Gradients become one thread in their average color.
        usvg::Paint::LinearGradient(g) => average(g.stops().iter().map(|s| rgb(s.color()))),
        usvg::Paint::RadialGradient(g) => average(g.stops().iter().map(|s| rgb(s.color()))),
        usvg::Paint::Pattern(_) => [128, 128, 128],
    }
}

pub(super) fn average(colors: impl Iterator<Item = [u8; 3]>) -> [u8; 3] {
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

pub(super) struct Collect {
    shapes: Vec<(Kind, Vec<(Contour, bool)>, [u8; 3])>,
    images: usize,
}

pub(super) fn walk_group(g: &usvg::Group, out: &mut Collect) {
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
                let fill = p
                    .fill()
                    .filter(|f| f.opacity().get() > 0.05)
                    .map(|f| (Kind::Fill { even_odd: f.rule() == usvg::FillRule::EvenOdd }, paint_color(f.paint())));
                let stroke = p.stroke().filter(|s| s.opacity().get() > 0.05).map(|s| (Kind::Stroke { width: s.width().get() * scale }, paint_color(s.paint())));
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
    let data = std::fs::read(path).map_err(|e| t!("Couldn't open {name}: {e}.", name = name, e = e))?;
    parse(&name, &data)
}

pub fn parse(name: &str, data: &[u8]) -> Result<Artwork, String> {
    let mut opt = usvg::Options::default();
    // Text in the SVG is drawn with the installed fonts.
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(data, &opt).map_err(|e| t!("{name} isn't an SVG Hoop can read: {e}.", name = name, e = e))?;
    let mut c = Collect { shapes: Vec::new(), images: 0 };
    walk_group(tree.root(), &mut c);
    if c.shapes.is_empty() {
        return Err(t!(
            "{name} has no shapes to stitch.{v0}",
            name = name,
            v0 = if c.images > 0 { " It only holds pictures; trace them into paths first (Inkscape: Path → Trace Bitmap)." } else { "" }
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
    let count = c.shapes.len();
    for (i, (kind, lines, color)) in c.shapes.into_iter().enumerate() {
        // A filled rectangle under everything that spans the drawing is a
        // background, which usually shouldn't be stitched.
        let mut sb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in lines.iter().flat_map(|l| &l.0) {
            sb = (sb.0.min(p.0), sb.1.min(p.1), sb.2.max(p.0), sb.3.max(p.1));
        }
        let filled: f32 = lines.iter().map(|l| area(&l.0)).sum();
        // A drawing that is one shape is that shape, not a background.
        let background =
            i == 0 && count > 1 && matches!(kind, Kind::Fill { .. }) && sb.2 - sb.0 >= bw * 0.97 && sb.3 - sb.1 >= bh * 0.97 && filled >= 0.9 * bw * bh;
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
