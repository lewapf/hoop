//! The stitch engine shared by lettering, artwork and test pieces: the
//! stitcher that turns moves into stitches with ties and travel, running
//! and satin lines, and the geometry the fills use. Fill styles live in
//! `fill`; satin columns, decorative fills and routing in their own files.
//! Units are 0.1 mm with y down.

pub mod column;
pub mod fill;
pub mod route;
pub mod special;

use crate::formats::Cmd;
use crate::stitch::route::Region;

pub use fill::*;

pub type Pt = (f32, f32);

pub type Contour = Vec<Pt>;
/// A polyline and whether it closes on itself.
pub type Line = (Contour, bool);

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
pub(crate) const TIE: f32 = 5.0;

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
pub(crate) fn walk(c: &Contour, start: usize, step: f32) -> Vec<Pt> {
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

pub(crate) fn nearest_index(c: &Contour, to: Option<Pt>) -> usize {
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
pub(crate) fn walk_open(c: &[Pt], step: f32) -> Vec<Pt> {
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
    if let Some(l) = st.last()
        && dist(l, c[c.len() - 1]) < dist(l, c[0])
    {
        c.reverse();
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
pub(crate) fn satin_rows(line: &[Pt], closed: bool, width: f32) -> Vec<(Pt, Pt)> {
    let mut path = line.to_vec();
    if closed && !path.is_empty() {
        path.push(path[0]);
    }
    let fine = walk_open(&path, 1.0);
    let n = fine.len();
    if n < 2 {
        return Vec::new();
    }
    let mut half = width / 2.0;
    // Round a small closed outline the inner edge would fold over itself
    // and the stitches fan out like a star; keep the satin to about the
    // outline's own radius.
    let loops = closed || (line.len() > 2 && dist(line[0], line[line.len() - 1]) < 1.0);
    if loops {
        let area = (0..line.len())
            .map(|i| {
                let (a, b) = (line[i], line[(i + 1) % line.len()]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<f32>()
            .abs()
            / 2.0;
        let radius = (area / std::f32::consts::PI).sqrt();
        half = half.min((radius * 0.45).max(4.0));
    }
    let reach = (half.max(3.0)).round() as usize;
    let mut out: Vec<(Pt, Pt)> = Vec::new();
    let mut last: Option<(Pt, Pt, Pt)> = None;
    // Satin spacing counts needle points on one side (0.4 mm peak to
    // peak), and stitches alternate sides, so each row moves half that.
    let step = 2.0;
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
            Some((lc, ll, lr)) => {
                // Along the line a row every step. Where only the outside
                // moves (the fan round a corner) rows come a little wider
                // apart there, so the inside isn't buried in stitches.
                let center = dist(c, lc);
                let edge = dist(l, ll).max(dist(r, lr));
                center >= step || edge >= step * (1.0 + 2.5 * (1.0 - center / step)) || i == n - 1
            }
        };
        if keep {
            out.push((l, r));
            last = Some((c, l, r));
        }
    }
    // Where one side crowds (the inside of a corner or a tight curve), the
    // needle would keep landing in nearly the same spot. Each crowded
    // stitch is shortened, alternately more and less, so the holes spread
    // over the column instead of piling up on its edge. Rows alternate
    // sides, so only the side a row is sewn to matters.
    let mut last: [Option<Pt>; 2] = [None, None];
    let mut deep = false;
    for (k, row) in out.iter_mut().enumerate() {
        let side = k % 2;
        let (l, r) = *row;
        let c = ((l.0 + r.0) / 2.0, (l.1 + r.1) / 2.0);
        let p = if side == 0 { l } else { r };
        let mut q = p;
        if last[side].is_some_and(|prev| dist(p, prev) < step * 0.75) {
            deep = !deep;
            let f = if deep { 0.45 } else { 0.7 };
            q = (c.0 + (p.0 - c.0) * f, c.1 + (p.1 - c.1) * f);
            if side == 0 {
                row.0 = q;
            } else {
                row.1 = q;
            }
        }
        last[side] = Some(q);
    }
    out
}

/// A satin column along an open line, for strokes.
pub fn satin_path(st: &mut Stitcher, c: &[Pt], width: f32) {
    if c.len() < 2 {
        return;
    }
    let mut c = c.to_vec();
    if let Some(l) = st.last()
        && dist(l, c[c.len() - 1]) < dist(l, c[0])
    {
        c.reverse();
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

/// Area of a shape left after cutting out the mask shapes on top of it.
pub fn masked_area(glyph: &[Contour], even_odd: bool, mask: &[MaskShape]) -> f32 {
    let (mut y0, mut y1) = (f32::MAX, f32::MIN);
    for p in glyph.iter().flatten() {
        (y0, y1) = (y0.min(p.1), y1.max(p.1));
    }
    let step = ((y1 - y0) / 400.0).max(1.0);
    let mut area = 0.0;
    let mut y = y0 + step / 2.0;
    while y < y1 {
        let spans = subtract(row_spans(glyph, y, even_odd), &mask_spans(mask, y));
        area += spans.iter().map(|(a, b)| b - a).sum::<f32>() * step;
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
