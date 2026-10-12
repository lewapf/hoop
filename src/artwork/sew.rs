//! Turning artwork into stitches: knockout of hidden parts, cutting lines
//! under fills, stitch order that keeps layers and saves thread changes,
//! angle-line fill parts, and the generator itself.

use super::*;

/// Splits a fill by its stitch angle lines. Each part is what lies nearer
/// one line's middle than any other's; it is returned as the other parts
/// (to mask out) and the line's angle. No lines give one part at `default`.
pub(super) fn angle_parts(lines: &[(Pt, Pt)], contours: &[Contour], default: f32) -> Vec<(Vec<lettering::MaskShape>, f32)> {
    let angle = |&(a, b): &(Pt, Pt)| (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
    if lines.len() <= 1 {
        return vec![(Vec::new(), lines.first().map_or(default, angle))];
    }
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in contours.iter().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let pad = 100.0;
    let bx: Contour = vec![(b.0 - pad, b.1 - pad), (b.2 + pad, b.1 - pad), (b.2 + pad, b.3 + pad), (b.0 - pad, b.3 + pad)];
    let mids: Vec<Pt> = lines.iter().map(|(a, b)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)).collect();
    // Voronoi cell of each middle: the box cut by the half planes nearer it.
    let cells: Vec<Contour> = (0..mids.len())
        .map(|i| {
            let mut cell = bx.clone();
            for j in 0..mids.len() {
                if i != j {
                    cell = clip_half(&cell, mids[i], mids[j]);
                }
            }
            cell
        })
        .collect();
    (0..lines.len())
        .map(|i| {
            let others = (0..cells.len()).filter(|&j| j != i && cells[j].len() >= 3).map(|j| (vec![cells[j].clone()], false)).collect();
            (others, angle(&lines[i]))
        })
        .collect()
}

/// The part of a convex polygon nearer `a` than `b` (Sutherland–Hodgman).
pub(super) fn clip_half(poly: &Contour, a: Pt, b: Pt) -> Contour {
    let m = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let n = (b.0 - a.0, b.1 - a.1);
    let side = |p: Pt| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1; // < 0 is a's side
    let mut out = Vec::new();
    for k in 0..poly.len() {
        let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp <= 0.0 {
            out.push(p);
        }
        if (sp < 0.0) != (sq < 0.0) && (sp - sq).abs() > 1e-9 {
            let t = sp / (sp - sq);
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

/// Whether `p` lies inside a filled shape or within `margin` of its edge.
pub(super) fn covered(p: Pt, shape: &[Contour], even_odd: bool, margin: f32) -> bool {
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
pub(super) fn clip_line(line: &Line, shapes: &[MaskShape], margin: f32) -> (Vec<Line>, Vec<Line>) {
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
pub(super) fn layered_order(threads: &[i32], touch: impl Fn(usize, usize) -> bool) -> Vec<usize> {
    let n = threads.len();
    // above[i]: shapes lying on i, which must wait for it.
    let mut above: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut below = vec![0usize; n];
    for (j, below_j) in below.iter_mut().enumerate() {
        for (i, above_i) in above.iter_mut().enumerate().take(j) {
            if touch(i, j) {
                *below_j += 1;
                above_i.push(j);
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
pub(super) struct Sewing {
    below: Vec<usize>,
    ready: Vec<usize>,
    current: Option<i32>,
}

impl Sewing {
    /// Sews one shape: the lowest ready one in the current thread, else the
    /// lowest ready one. Returns whether the thread changed.
    pub(super) fn step(&mut self, threads: &[i32], above: &[Vec<usize>], order: &mut Vec<usize>) -> bool {
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
    pub(super) fn run(&mut self, threads: &[i32], above: &[Vec<usize>], order: &mut Vec<usize>, depth: usize) -> usize {
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
pub(super) struct Coverage {
    bounds: (i32, i32, i32, i32),
    cells: HashSet<(i32, i32)>,
}

impl Coverage {
    pub(super) const CELL: f32 = 5.0;

    pub(super) fn new(lines: &[(Contour, bool)], fill: Option<bool>, width: f32) -> Self {
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

    pub(super) fn overlaps(&self, o: &Coverage) -> bool {
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
        /// Stitch angle lines and entry point, in stitch units.
        angles: Vec<(Pt, Pt)>,
        entry: Option<Pt>,
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
            let angles = e.angles.iter().map(|&(a, b)| (to_mm(e.place(art, a)), to_mm(e.place(art, b)))).collect();
            let entry = e.entry.map(|p| to_mm(e.place(art, p)));
            items.push(Item { doc, layer: e.layer, look, kind: sh.kind.clone(), lines, sew: None, angles, entry });
        }
    }
    if items.is_empty() {
        return Err(if s.shapes.iter().all(|e| e.hidden) {
            t!("No shapes yet. Add one from Shapes, or open an SVG or a picture.").into()
        } else {
            t!("Every color is turned off or too small to stitch.").into()
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
            for (name, passes) in [(t!("Placement line"), 1), (t!("Tack-down (lay the fabric first)"), 2)] {
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
                st.region = crate::stitch::route::Region::new(&contours, even_odd, &mask, &above(true));
                if st_ref.fill != FillKind::None {
                    if let Some(p) = it.entry {
                        st.travel(p, p);
                    }
                    // The parts the angle lines split the shape into, each
                    // with its mask and angle; one part when there are none.
                    let parts = angle_parts(&it.angles, &contours, st_ref.fill_angle);
                    let first = parts[0].1;
                    if st_ref.underlay {
                        lettering::underlay(&mut st, &contours, even_odd, &mask, st_ref.fill, first, st_ref);
                    }
                    for (cut, angle) in &parts {
                        let mut m = mask.clone();
                        m.extend(cut.iter().cloned());
                        lettering::fill_shape(&mut st, &contours, even_odd, &m, st_ref.fill, *angle, st_ref);
                        if st_ref.fill == FillKind::Crosshatch {
                            lettering::fill_shape(&mut st, &contours, even_odd, &m, st_ref.fill, *angle + 90.0, st_ref);
                        }
                    }
                }
                for c in &contours {
                    lettering::edge_contour(&mut st, c, st_ref);
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
                        EdgeKind::Running | EdgeKind::Blanket | EdgeKind::Scallop => lettering::run_path(&mut st, &line, 25.0, false),
                        EdgeKind::Triple => lettering::run_path(&mut st, &line, 25.0, true),
                        EdgeKind::Satin => lettering::satin_path(&mut st, &line, w),
                    }
                }
            }
        }
    }
    st.finish();
    if !st.cmds.contains(&Cmd::Stitch) {
        return Err(t!("Nothing to stitch with these settings. Turn on a fill or a line style.").into());
    }
    Ok(Pattern { format: Format::Svg, points: st.points, cmds: st.cmds, threads })
}
