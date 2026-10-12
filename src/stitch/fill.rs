//! Fill styles: rows (tatami, satin, wave, zigzag, gradient, open,
//! crosshatch), contour rings, and the dispatch to columns, stipple and
//! cross-stitch, plus the underlay sewn first.

use super::*;
use crate::lettering::Settings;

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
    matches!(kind, FillKind::Tatami | FillKind::Satin | FillKind::Wave | FillKind::Zigzag | FillKind::Contour | FillKind::Column | FillKind::Auto)
        && rows_mm <= 0.8
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
    if crate::stitch::column::widest(glyph, even_odd, mask) <= s.column_max_mm * 10.0 { FillKind::Column } else { FillKind::Tatami }
}

pub fn fill_shape(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], kind: FillKind, angle: f32, s: &Settings) {
    if let Some(r) = &mut st.region {
        r.clear_sewn();
    }
    let kind = resolve(kind, glyph, even_odd, mask, s);
    match kind {
        FillKind::Column => return crate::stitch::column::column_fill(st, glyph, even_odd, mask, s),
        FillKind::Stipple => return crate::stitch::special::stipple_fill(st, glyph, even_odd, mask, s),
        FillKind::Cross => return crate::stitch::special::cross_fill(st, glyph, even_odd, mask, s),
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
    // Each row: its height and its segments (start, end, sewn yet).
    type Row = (f32, Vec<(f32, f32, bool)>);
    let mut rows: Vec<Row> = Vec::new();
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

/// Rings that follow the outline inward, one row spacing apart. The shape is
/// rasterized, every inside cell gets its distance to the edge, and the rings
/// are the lines of equal distance, traced with marching squares.
pub(crate) fn contour_fill(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
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
