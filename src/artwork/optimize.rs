//! Tidying the Objects list: shapes named by their thread, compound shapes
//! broken into their pieces, touching lines of one color joined, and
//! shapes too small to stitch or exact duplicates dropped.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Optimize {
    pub rename: bool,
    /// Fills made of separate pieces become one shape per piece.
    pub split: bool,
    /// Lines made of several loops or strokes become one shape each.
    pub split_lines: bool,
    pub join: bool,
    /// Shapes of one thread, kind and look that follow each other in the
    /// stack (or only have non-touching shapes between them) become one.
    pub merge: bool,
    pub drop_small: bool,
}

impl Default for Optimize {
    fn default() -> Self {
        Optimize { rename: true, split: false, split_lines: false, join: true, merge: true, drop_small: true }
    }
}

/// What an optimize run did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Optimized {
    pub before: usize,
    pub after: usize,
    pub split: usize,
    pub joined: usize,
    pub merged: usize,
    pub dropped: usize,
}

/// A shape with the given outline, keeping everything else of `e`.
fn with_lines(e: &ShapeEdit, kind: Kind, lines: Vec<Line>) -> ShapeEdit {
    ShapeEdit {
        own: Some(Shape { kind, lines, layer: e.layer }),
        dx: 0.0,
        dy: 0.0,
        scale: 1.0,
        rotate: 0.0,
        // Angle lines and the entry point belong to the old outline.
        angles: Vec::new(),
        entry: None,
        made: None,
        ..e.clone()
    }
}

fn signed_area(c: &Contour) -> f32 {
    (0..c.len())
        .map(|i| {
            let (a, b) = (c[i], c[(i + 1) % c.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f32>()
        / 2.0
}

/// The pieces of a fill: each outer outline with the holes directly in it.
fn pieces(lines: &[Line], even_odd: bool) -> Vec<Vec<Line>> {
    let n = lines.len();
    if n <= 1 {
        return vec![lines.to_vec()];
    }
    // How many other outlines each one lies inside: even is an outer
    // outline, odd a hole (nested islands count again as outer).
    let probe = |c: &Contour| c[0];
    let contains = |a: usize, b: usize| a != b && crate::stitch::inside(probe(&lines[b].0), std::slice::from_ref(&lines[a].0), true);
    let depth: Vec<usize> = (0..n).map(|b| (0..n).filter(|&a| contains(a, b)).count()).collect();
    // Under the nonzero rule a same-direction inner outline is not a hole;
    // then the shape is left whole rather than guessed at.
    if !even_odd {
        for b in 0..n {
            if depth[b] % 2 == 1 {
                let parent = (0..n)
                    .filter(|&a| contains(a, b) && depth[a] == depth[b] - 1)
                    .min_by(|&x, &y| signed_area(&lines[x].0).abs().total_cmp(&signed_area(&lines[y].0).abs()));
                if let Some(a) = parent
                    && signed_area(&lines[a].0).signum() == signed_area(&lines[b].0).signum()
                {
                    return vec![lines.to_vec()];
                }
            }
        }
    }
    let mut out: Vec<Vec<Line>> = Vec::new();
    let mut index: Vec<Option<usize>> = vec![None; n];
    for a in 0..n {
        if depth[a].is_multiple_of(2) {
            index[a] = Some(out.len());
            out.push(vec![lines[a].clone()]);
        }
    }
    for b in 0..n {
        if depth[b] % 2 == 1 {
            // The smallest outer outline around this hole.
            let parent = (0..n)
                .filter(|&a| depth[a].is_multiple_of(2) && contains(a, b))
                .min_by(|&x, &y| signed_area(&lines[x].0).abs().total_cmp(&signed_area(&lines[y].0).abs()));
            match parent.and_then(|a| index[a]) {
                Some(k) => out[k].push(lines[b].clone()),
                None => out.push(vec![lines[b].clone()]),
            }
        }
    }
    out
}

/// Joins open lines whose ends meet (within `tol`) into longer lines.
fn join_lines(mut lines: Vec<Contour>, tol: f32) -> (Vec<Contour>, usize) {
    let mut joined = 0;
    loop {
        let mut merged = false;
        'outer: for i in 0..lines.len() {
            for j in 0..lines.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (&lines[i], &lines[j]);
                let (a0, a1, b0, b1) = (a[0], a[a.len() - 1], b[0], b[b.len() - 1]);
                let near = |p: Pt, q: Pt| lettering::dist(p, q) <= tol;
                let mut b = b.clone();
                let mut a = a.clone();
                let ok = if near(a1, b0) {
                    true
                } else if near(a1, b1) {
                    b.reverse();
                    true
                } else if near(a0, b1) {
                    std::mem::swap(&mut a, &mut b);
                    true
                } else if near(a0, b0) {
                    a.reverse();
                    true
                } else {
                    false
                };
                if ok {
                    a.extend(b.into_iter().skip(1));
                    let (lo, hi) = (i.min(j), i.max(j));
                    lines.remove(hi);
                    lines[lo] = a;
                    joined += 1;
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return (lines, joined);
        }
    }
}

/// Bounding box of a shape as placed, in user units.
fn bbox(art: &Artwork, e: &ShapeEdit) -> (f32, f32, f32, f32) {
    extent(&e.lines(art))
}

fn touch(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// Whether two shapes can become one without changing how they sew.
fn mergeable(art: &Artwork, a: &ShapeEdit, b: &ShapeEdit) -> bool {
    let same_kind = match (&a.base(art).kind, &b.base(art).kind) {
        (Kind::Fill { even_odd: x }, Kind::Fill { even_odd: y }) => x == y,
        (Kind::Stroke { width: x }, Kind::Stroke { width: y }) => (x - y).abs() <= x.max(*y) * 0.05,
        _ => false,
    };
    same_kind
        && a.layer == b.layer
        && a.group == b.group
        && a.look == b.look
        && a.style == b.style
        && !a.hidden
        && !b.hidden
        && a.angles.is_empty()
        && b.angles.is_empty()
        && a.entry.is_none()
        && b.entry.is_none()
}

/// Where a shape really lies, on the 0.5 mm grid stitch ordering uses.
fn coverage(art: &Artwork, s: &ArtSettings, e: &ShapeEdit) -> Coverage {
    let (scale, (cx, cy)) = frame(art, s);
    let lines: Vec<Line> = e.lines(art).into_iter().map(|(c, k)| (c.into_iter().map(|p| ((p.0 - cx) * scale, (p.1 - cy) * scale)).collect(), k)).collect();
    match e.base(art).kind {
        Kind::Fill { even_odd } => Coverage::new(&lines, Some(even_odd), 0.0),
        Kind::Stroke { width } => Coverage::new(&lines, None, (width * scale).max(s.stroke_min_mm * 10.0).min(70.0)),
    }
}

/// Merges each shape into an earlier one it can join, as long as nothing
/// it really overlaps lies between them in the stack, so layering stays
/// as drawn.
fn merge_shapes(art: &Artwork, s: &ArtSettings, shapes: Vec<ShapeEdit>, merged: &mut usize) -> Vec<ShapeEdit> {
    let mut out: Vec<ShapeEdit> = Vec::new();
    let mut covers: Vec<Coverage> = Vec::new();
    for e in shapes {
        let eb = bbox(art, &e);
        let ec = coverage(art, s, &e);
        let mut target = None;
        for k in (0..out.len()).rev() {
            if mergeable(art, &out[k], &e) {
                target = Some(k);
                break;
            }
            if touch(bbox(art, &out[k]), eb) && covers[k].overlaps(&ec) {
                break;
            }
        }
        match target {
            Some(k) => {
                let mut lines = out[k].lines(art);
                lines.extend(e.lines(art));
                let kind = out[k].base(art).kind.clone();
                out[k] = with_lines(&out[k], kind, lines);
                covers[k] = coverage(art, s, &out[k]);
                *merged += 1;
            }
            None => {
                out.push(e);
                covers.push(ec);
            }
        }
    }
    out
}

/// Tidies the shapes of `s` as `opts` asks.
pub fn optimize(art: &Artwork, s: &mut ArtSettings, opts: Optimize) -> Optimized {
    let before = s.shapes.len();
    let mut report = Optimized { before, ..Default::default() };
    let (scale, _) = frame(art, s);
    let mut shapes: Vec<ShapeEdit> = Vec::new();
    for e in &s.shapes {
        let lines = e.lines(art);
        let kind = e.base(art).kind.clone();
        if (opts.split || opts.split_lines) && lines.len() > 1 {
            match kind {
                Kind::Fill { even_odd } if opts.split => {
                    let parts = pieces(&lines, even_odd);
                    if parts.len() > 1 {
                        report.split += parts.len() - 1;
                        shapes.extend(parts.into_iter().map(|p| with_lines(e, kind.clone(), p)));
                        continue;
                    }
                }
                Kind::Stroke { .. } if opts.split_lines => {
                    report.split += lines.len() - 1;
                    shapes.extend(lines.into_iter().map(|l| with_lines(e, kind.clone(), vec![l])));
                    continue;
                }
                _ => {}
            }
        }
        shapes.push(e.clone());
    }
    if opts.drop_small {
        let min_area = (s.min_detail_mm * 10.0).powi(2).max(1.0) / (scale * scale);
        let n = shapes.len();
        shapes.retain(|e| {
            let lines = e.lines(art);
            match e.base(art).kind {
                Kind::Fill { .. } => lines.iter().map(|l| signed_area(&l.0).abs()).sum::<f32>() >= min_area,
                Kind::Stroke { .. } => lines.iter().map(|l| length(&l.0)).sum::<f32>() * scale >= 10.0,
            }
        });
        // Exact duplicates: same kind, color and outline as an earlier one.
        let mut seen: Vec<(usize, Vec<Line>)> = Vec::new();
        shapes.retain(|e| {
            let key = (e.layer, e.lines(art));
            let dup = seen.iter().any(|(l, ls)| {
                *l == key.0
                    && ls.len() == key.1.len()
                    && ls
                        .iter()
                        .zip(&key.1)
                        .all(|(a, b)| a.1 == b.1 && a.0.len() == b.0.len() && a.0.iter().zip(&b.0).all(|(p, q)| lettering::dist(*p, *q) < 0.01))
            });
            if !dup {
                seen.push(key);
            }
            !dup
        });
        report.dropped = n - shapes.len();
    }
    if opts.merge {
        shapes = merge_shapes(art, s, shapes, &mut report.merged);
    }
    if opts.join {
        // Lines of one color and width that follow each other in the
        // drawing, and touch end to end, become one line.
        let tol = 5.0 / scale;
        let mut out: Vec<ShapeEdit> = Vec::new();
        for e in shapes {
            let open_line =
                |x: &ShapeEdit| matches!(x.base(art).kind, Kind::Stroke { .. }) && x.lines(art).iter().all(|l| !l.1) && !x.hidden && x.style.is_none();
            if let Some(last) = out.last_mut() {
                let same = match (&last.base(art).kind, &e.base(art).kind) {
                    (Kind::Stroke { width: a }, Kind::Stroke { width: b }) => (a - b).abs() <= a.max(*b) * 0.05,
                    _ => false,
                };
                if same && open_line(last) && open_line(&e) && last.layer == e.layer && last.look == e.look {
                    let mut all: Vec<Contour> = last.lines(art).into_iter().map(|l| l.0).collect();
                    all.extend(e.lines(art).into_iter().map(|l| l.0));
                    let (lines, n) = join_lines(all, tol);
                    if n > 0 {
                        let kind = last.base(art).kind.clone();
                        report.joined += n;
                        // Pieces that didn't touch stay separate lines of one object.
                        *last = with_lines(last, kind, lines.into_iter().map(|c| (c, false)).collect());
                        continue;
                    }
                }
            }
            out.push(e);
        }
        shapes = out;
    }
    if opts.rename {
        let mut counts: std::collections::HashMap<(i32, bool), usize> = std::collections::HashMap::new();
        for e in shapes.iter_mut() {
            let thread = s.layers.get(e.layer).map_or_else(|| formats::nearest_jef(art.layers[e.layer].color), |l| l.thread);
            let fill = matches!(e.base(art).kind, Kind::Fill { .. });
            let n = counts.entry((thread, fill)).or_insert(0);
            *n += 1;
            let color = formats::jef_thread(thread, 0).name;
            e.name = if fill { t!("{color} fill {n}", color = color, n = *n) } else { t!("{color} line {n}", color = color, n = *n) };
        }
    }
    report.after = shapes.len();
    s.shapes = shapes;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HOOP_SVG=file: what optimizing does to a real drawing.
    #[test]
    #[ignore]
    fn optimize_file() {
        let art = load(std::path::Path::new(&std::env::var("HOOP_SVG").unwrap())).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        let p0 = generate(&art, &s).unwrap();
        let r = optimize(&art, &mut s, Optimize::default());
        let p1 = generate(&art, &s).unwrap();
        eprintln!("{r:?}; stitches {} → {}, jumps {} → {}", p0.count(Cmd::Stitch), p1.count(Cmd::Stitch), p0.count(Cmd::Jump), p1.count(Cmd::Jump));
        for e in s.shapes.iter().take(12) {
            eprintln!("  {}", e.name);
        }
    }

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
        <path d="M10 10 h10 v10 h-10 z M40 10 h10 v10 h-10 z M60 5 h30 v30 h-30 z M70 15 h10 v10 h-10 z" fill="#00aa00" fill-rule="evenodd"/>
        <path d="M5 40 L30 40" stroke="#000000" stroke-width="1" fill="none"/>
        <path d="M30 40 L60 45" stroke="#000000" stroke-width="1" fill="none"/>
        <circle cx="95" cy="45" r="0.2" fill="#ff0000"/>
    </svg>"##;

    #[test]
    fn merging_never_adds_objects_and_keeps_the_stitches() {
        let art = parse("t", SVG.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.min_detail_mm = 1.0;
        let before = generate(&art, &s).unwrap().count(Cmd::Stitch);
        let r = optimize(&art, &mut s, Optimize::default());
        assert!(r.after < r.before, "{r:?}");
        let after = generate(&art, &s).unwrap().count(Cmd::Stitch);
        assert!((after as f32 - before as f32).abs() < before as f32 * 0.1, "{before} → {after}");
    }

    #[test]
    fn breaks_apart_joins_lines_and_drops_specks() {
        let art = parse("t", SVG.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.min_detail_mm = 1.0;
        assert_eq!(s.shapes.len(), 4);
        let before = generate(&art, &s).unwrap().count(Cmd::Stitch);
        let r = optimize(&art, &mut s, Optimize { split: true, merge: false, ..Optimize::default() });
        // Green: two squares and a ring (square with its hole) = 3 pieces;
        // the two black lines join into one; the red speck goes.
        assert_eq!(r.split, 2, "{r:?}");
        assert_eq!(r.joined, 1, "{r:?}");
        assert_eq!(r.dropped, 1, "{r:?}");
        assert_eq!(s.shapes.len(), 4);
        let ring = s.shapes.iter().find(|e| e.lines(&art).len() == 2).expect("the ring keeps its hole");
        assert!(matches!(ring.base(&art).kind, Kind::Fill { .. }));
        assert!(s.shapes.iter().any(|e| e.name.contains("line 1")), "{:?}", s.shapes.iter().map(|e| &e.name).collect::<Vec<_>>());
        // Still stitches about the same (the speck was too small anyway).
        let after = generate(&art, &s).unwrap().count(Cmd::Stitch);
        assert!((after as f32 - before as f32).abs() < before as f32 * 0.1, "{before} → {after}");
    }
}
