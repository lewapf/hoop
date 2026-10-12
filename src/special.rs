//! Decorative stitch types: stipple (a meandering line that fills a shape
//! without crossing itself, as in quilting), cross-stitch, and the blanket
//! and scallop edges.

use crate::lettering::{self, Contour, MaskShape, Pt, Settings, Stitcher, dist, mask_spans, row_spans, subtract};

/// Cells of a grid whose centers lie inside the shape, at least `margin`
/// from its edge. Returns (origin x, origin y, columns, rows, inside flags).
fn inside_cells(glyph: &[Contour], even_odd: bool, mask: &[MaskShape], cell: f32, margin: f32) -> (f32, f32, usize, usize, Vec<bool>) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyph.iter().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let (x0, y0) = (b.0, b.1);
    let w = ((b.2 - b.0) / cell).ceil().max(1.0) as usize;
    let h = ((b.3 - b.1) / cell).ceil().max(1.0) as usize;
    let mut inside = vec![false; w * h];
    for j in 0..h {
        let cy = y0 + (j as f32 + 0.5) * cell;
        // A cell counts when the rows a margin above and below its center
        // also cover it, so the line keeps clear of the edge.
        let rows: Vec<Vec<(f32, f32)>> = [cy - margin, cy, cy + margin]
            .iter()
            .map(|&y| {
                let mut s = row_spans(glyph, y, even_odd);
                if !mask.is_empty() {
                    s = subtract(s, &mask_spans(mask, y));
                }
                s
            })
            .collect();
        for i in 0..w {
            let cx = x0 + (i as f32 + 0.5) * cell;
            inside[j * w + i] = rows.iter().all(|spans| spans.iter().any(|&(a, bb)| cx - margin >= a && cx + margin <= bb));
        }
    }
    (x0, y0, w, h, inside)
}

/// A small, repeatable random number generator, so the same shape always
/// gets the same stipple.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Corner cutting: rounds a closed line `rounds` times.
fn chaikin(c: &Contour, rounds: usize) -> Contour {
    let mut c = c.clone();
    for _ in 0..rounds {
        let n = c.len();
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            out.push((a.0 * 0.75 + b.0 * 0.25, a.1 * 0.75 + b.1 * 0.25));
            out.push((a.0 * 0.25 + b.0 * 0.75, a.1 * 0.25 + b.1 * 0.75));
        }
        c = out;
    }
    c
}

/// Stipple: a random tree is grown over a grid of cells inside the shape,
/// and the needle runs round the tree. That line visits every cell, keeps
/// about `density_mm` from itself and never crosses; rounding its corners
/// gives the familiar meander.
pub fn stipple_fill(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
    // The fabric shows between the lines, so moves jump instead of
    // running through the shape.
    let region = st.region.take();
    stipple(st, glyph, even_odd, mask, s);
    st.region = region;
}

fn stipple(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
    let spacing = (s.density_mm * 10.0).max(10.0);
    let cell = spacing * 2.0;
    let (x0, y0, w, h, inside) = inside_cells(glyph, even_odd, mask, cell, spacing * 0.5);
    // Random depth-first tree over the inside cells.
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (w as u64 * 31 + h as u64));
    let mut seen = vec![false; w * h];
    // Fine grid at half a cell: tree cells at even positions, links between them.
    let (fw, fh) = (2 * w + 1, 2 * h + 1);
    let mut fine = vec![0.0f32; fw * fh];
    for start in 0..w * h {
        if !inside[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        fine[(2 * (start / w) + 1) * fw + 2 * (start % w) + 1] = 1.0;
        while let Some(&c) = stack.last() {
            let (i, j) = ((c % w) as isize, (c / w) as isize);
            let mut next: Vec<(isize, isize)> = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter(|&(di, dj)| {
                    let (x, y) = (i + di, j + dj);
                    x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && inside[y as usize * w + x as usize] && !seen[y as usize * w + x as usize]
                })
                .collect();
            if next.is_empty() {
                stack.pop();
                continue;
            }
            let (di, dj) = next.swap_remove((rng.next() % next.len() as u64) as usize);
            let (x, y) = ((i + di) as usize, (j + dj) as usize);
            let k = y * w + x;
            seen[k] = true;
            fine[(2 * y + 1) * fw + 2 * x + 1] = 1.0;
            fine[((2 * j + 1) as usize + dj as usize) * fw + (2 * i + 1) as usize + di as usize] = 1.0;
            stack.push(k);
        }
    }
    let half = cell / 2.0;
    let center = |i: usize, j: usize| (x0 + i as f32 * half, y0 + j as f32 * half);
    let rings = lettering::iso_lines(&fine, fw, fh, 0.5, &center);
    let stitch_len = (s.stitch_mm * 10.0).clamp(15.0, 25.0);
    for ring in rings {
        if ring.len() < 4 {
            continue;
        }
        let smooth = chaikin(&ring, 4);
        let path = lettering::densify(&smooth, stitch_len);
        let from = st.last().unwrap_or(path[0]);
        let start = (0..path.len()).min_by(|&a, &b| dist(path[a], from).total_cmp(&dist(path[b], from))).unwrap_or(0);
        let n = path.len();
        st.travel(path[start], path[(start + 1) % n]);
        let mut last = path[start];
        for k in 1..=n {
            let p = path[(start + k) % n];
            if dist(p, last) >= stitch_len * 0.6 || k == n {
                st.stitch(p);
                last = p;
            }
        }
    }
}

/// Cross-stitch: an X in every grid cell inside the shape, each row sewn
/// as two zigzags of opposite phase so no stitch runs along a cell edge.
pub fn cross_fill(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
    let region = st.region.take();
    cross(st, glyph, even_odd, mask, s);
    st.region = region;
}

fn cross(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
    let cell = (s.density_mm * 10.0).max(15.0);
    let (x0, y0, w, h, inside) = inside_cells(glyph, even_odd, mask, cell, cell * 0.45);
    let corner = |i: usize, j: usize| (x0 + i as f32 * cell, y0 + j as f32 * cell);
    for j in 0..h {
        // Runs of neighboring cells in this row.
        let mut i = 0;
        while i < w {
            if !inside[j * w + i] {
                i += 1;
                continue;
            }
            let a = i;
            while i < w && inside[j * w + i] {
                i += 1;
            }
            let b = i; // cells a..b
            // Forward: "/" in even cells, "\" in odd ones; back: the others.
            let fwd: Vec<Pt> = (a..=b).map(|k| corner(k, if (k - a) % 2 == 0 { j + 1 } else { j })).collect();
            let back: Vec<Pt> = (a..=b).rev().map(|k| corner(k, if (k - a) % 2 == 0 { j } else { j + 1 })).collect();
            st.travel(fwd[0], fwd[1]);
            for p in &fwd[1..] {
                st.stitch(*p);
            }
            // Down the end edge to start back; the X covers it.
            for p in &back {
                st.stitch(*p);
            }
        }
    }
}

/// Blanket (E) stitch round an outline: a run along the edge with a short
/// stitch into the shape every `step`, the classic appliqué finish.
pub fn blanket_contour(st: &mut Stitcher, c: &Contour, width: f32) {
    let step = 25.0;
    let pts = lettering::densify(c, 2.0);
    if pts.len() < 3 {
        return;
    }
    // Which side is inside: the outline's turning direction.
    let area: f32 = (0..pts.len()).map(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        a.0 * b.1 - b.0 * a.1
    }).sum();
    let side = if area > 0.0 { 1.0 } else { -1.0 };
    let along = walk_closed(&pts, step);
    st.travel(along[0], along[1]);
    for k in 0..along.len() {
        let p = along[k];
        let q = along[(k + 1) % along.len()];
        let d = dist(p, q).max(0.01);
        let n = (-(q.1 - p.1) / d * side, (q.0 - p.0) / d * side);
        st.stitch(p);
        st.stitch((p.0 + n.0 * width, p.1 + n.1 * width));
        st.stitch(p);
    }
    st.stitch(along[0]);
}

/// Scallops: half circles along the outline, bulging outward.
pub fn scallop_contour(st: &mut Stitcher, c: &Contour, width: f32, triple: bool) {
    let pts = lettering::densify(c, 2.0);
    if pts.len() < 3 {
        return;
    }
    let area: f32 = (0..pts.len()).map(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        a.0 * b.1 - b.0 * a.1
    }).sum();
    let side = if area > 0.0 { -1.0 } else { 1.0 };
    let span = (width * 2.0).max(30.0);
    let along = walk_closed(&pts, span / 6.0);
    let mut line: Vec<Pt> = Vec::with_capacity(along.len());
    for k in 0..along.len() {
        let p = along[k];
        let q = along[(k + 1) % along.len()];
        let d = dist(p, q).max(0.01);
        let n = (-(q.1 - p.1) / d * side, (q.0 - p.0) / d * side);
        // Six samples per scallop: a sine hump.
        let t = (k % 6) as f32 / 6.0;
        let lift = (t * std::f32::consts::PI).sin() * width;
        line.push((p.0 + n.0 * lift, p.1 + n.1 * lift));
    }
    st.travel(line[0], line[1]);
    for w in line.windows(2) {
        if triple {
            st.stitch(w[1]);
            st.stitch(w[0]);
        }
        st.stitch(w[1]);
    }
    if triple {
        st.stitch(line[0]);
        st.stitch(line[line.len() - 1]);
    }
    st.stitch(line[0]);
}

/// Points every `step` round a closed line.
fn walk_closed(c: &Contour, step: f32) -> Vec<Pt> {
    let n = c.len();
    let total: f32 = (0..n).map(|i| dist(c[i], c[(i + 1) % n])).sum();
    let count = (total / step).round().max(3.0) as usize;
    let step = total / count as f32;
    let mut out = vec![c[0]];
    let mut carry = 0.0;
    for i in 0..n {
        let (a, b) = (c[i], c[(i + 1) % n]);
        let len = dist(a, b);
        let mut t = step - carry;
        while t < len - 1e-3 && out.len() < count {
            out.push((a.0 + (b.0 - a.0) * t / len, a.1 + (b.1 - a.1) * t / len));
            t += step;
        }
        carry = len - (t - step);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::Cmd;

    fn square(n: f32) -> Vec<Contour> {
        vec![vec![(0.0, 0.0), (n, 0.0), (n, n), (0.0, n)]]
    }

    fn stitched(st: &Stitcher) -> Vec<Pt> {
        st.points.iter().zip(&st.cmds).filter(|(_, c)| **c == Cmd::Stitch).map(|(p, _)| *p).collect()
    }

    fn crosses(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
        let o = |p: Pt, q: Pt, r: Pt| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
        let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
        d1 * d2 < -1e-3 && d3 * d4 < -1e-3
    }

    #[test]
    fn stipple_covers_the_shape_without_crossing_itself() {
        let mut st = Stitcher::new();
        let s = Settings { density_mm: 2.5, ..Settings::default() };
        stipple_fill(&mut st, &square(400.0), false, &[], &s);
        let pts = stitched(&st);
        assert!(pts.len() > 200, "{}", pts.len());
        assert!(pts.iter().all(|p| p.0 > 0.0 && p.0 < 400.0 && p.1 > 0.0 && p.1 < 400.0));
        // Every 5 mm square of the inside gets the line.
        for gy in 1..7 {
            for gx in 1..7 {
                let c = (gx as f32 * 50.0 + 25.0, gy as f32 * 50.0 + 25.0);
                assert!(pts.iter().any(|p| (p.0 - c.0).abs() < 25.0 && (p.1 - c.1).abs() < 25.0), "{c:?}");
            }
        }
        let segs: Vec<(Pt, Pt)> = pts.windows(2).map(|w| (w[0], w[1])).collect();
        let mut bad = 0;
        for i in 0..segs.len() {
            for j in i + 2..segs.len() {
                if crosses(segs[i].0, segs[i].1, segs[j].0, segs[j].1) {
                    bad += 1;
                }
            }
        }
        assert_eq!(bad, 0);
    }

    #[test]
    fn cross_stitch_puts_an_x_in_each_cell() {
        let mut st = Stitcher::new();
        let s = Settings { density_mm: 2.5, ..Settings::default() };
        cross_fill(&mut st, &square(100.0), false, &[], &s);
        let pts = stitched(&st);
        // 4 × 4 cells; each X is two diagonals of 25 √2.
        let diag = pts.windows(2).filter(|w| (dist(w[0], w[1]) - 25.0 * 2f32.sqrt()).abs() < 0.5).count();
        assert!(diag >= 32, "{diag}");
        // No stitch runs along a cell edge except the turn at a row's end.
        let edges = pts.windows(2).filter(|w| (dist(w[0], w[1]) - 25.0).abs() < 0.5).count();
        assert!(edges <= 4, "{edges}");
    }

    #[test]
    fn blanket_prongs_point_inward() {
        let mut st = Stitcher::new();
        blanket_contour(&mut st, &square(200.0)[0], 20.0);
        let pts = stitched(&st);
        assert!(pts.iter().any(|p| p.0 > 15.0 && p.0 < 185.0 && p.1 > 15.0 && p.1 < 185.0));
        assert!(pts.iter().all(|p| p.0 > -1.0 && p.0 < 201.0 && p.1 > -1.0 && p.1 < 201.0));
    }
}
