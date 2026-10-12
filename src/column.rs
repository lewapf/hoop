//! Satin columns that turn with the shape, like the column tools of
//! digitizing programs. The shape's center lines come from thinning it on
//! a fine grid; short spurs from corners are pruned; each remaining
//! branch is smoothed and sewn as satin whose stitches stand square to the
//! branch, reaching the outline found by casting rays both ways.
//!
//! Each branch gets a center-run underlay (and a zigzag one when wide),
//! its open ends run on to the tip of the shape, stitches on the inside of
//! tight curves are shortened so needle holes don't pile up, and parts a
//! branch shares with one sewn before it are left out.

use crate::lettering::{self, Contour, MaskShape, Pt, Settings, Stitcher, dist, mask_spans, row_spans, subtract};

/// Distance from each grid cell to the outside of a shape.
pub struct Field {
    pub w: usize,
    pub h: usize,
    pub x0: f32,
    pub y0: f32,
    pub cell: f32,
    /// Distance to the outline in world units; 0 outside.
    pub d: Vec<f32>,
}

impl Field {
    pub fn new(glyph: &[Contour], even_odd: bool, mask: &[MaskShape], cell: f32) -> Option<Field> {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in glyph.iter().flatten() {
            b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
        }
        if b.2 <= b.0 || b.3 <= b.1 {
            return None;
        }
        let (x0, y0) = (b.0 - 3.0 * cell, b.1 - 3.0 * cell);
        let w = ((b.2 - b.0) / cell) as usize + 7;
        let h = ((b.3 - b.1) / cell) as usize + 7;
        let mut f = vec![0.0f32; w * h];
        for j in 0..h {
            let y = y0 + (j as f32 + 0.5) * cell;
            let mut spans = row_spans(glyph, y, even_odd);
            if !mask.is_empty() {
                spans = subtract(spans, &mask_spans(mask, y));
            }
            for (a, bb) in spans {
                let i0 = ((a - x0) / cell - 0.5).ceil().max(0.0) as usize;
                let i1 = ((bb - x0) / cell - 0.5).floor();
                if i1 < 0.0 {
                    continue;
                }
                for i in i0..=(i1 as usize).min(w - 1) {
                    f[j * w + i] = 1e12;
                }
            }
        }
        let mut buf = vec![0.0f32; w.max(h)];
        let mut out = vec![0.0f32; w.max(h)];
        for i in 0..w {
            for j in 0..h {
                buf[j] = f[j * w + i];
            }
            lettering::edt_1d(&buf[..h], &mut out[..h]);
            for j in 0..h {
                f[j * w + i] = out[j];
            }
        }
        for j in 0..h {
            buf[..w].copy_from_slice(&f[j * w..j * w + w]);
            lettering::edt_1d(&buf[..w], &mut out[..w]);
            for i in 0..w {
                // Cell centers next to the outside are about half a cell from the edge.
                f[j * w + i] = if out[i] > 0.0 { (out[i].sqrt() - 0.5).max(0.25) * cell } else { 0.0 };
            }
        }
        Some(Field { w, h, x0, y0, cell, d: f })
    }

    fn center(&self, k: usize) -> Pt {
        (self.x0 + ((k % self.w) as f32 + 0.5) * self.cell, self.y0 + ((k / self.w) as f32 + 0.5) * self.cell)
    }

    fn index(&self, p: Pt) -> Option<usize> {
        let i = ((p.0 - self.x0) / self.cell).floor();
        let j = ((p.1 - self.y0) / self.cell).floor();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.w && (j as usize) < self.h).then(|| j as usize * self.w + i as usize)
    }

    pub fn at(&self, p: Pt) -> f32 {
        self.index(p).map_or(0.0, |k| self.d[k])
    }
}

/// A grid cell size that keeps the grid to about a million cells.
fn cell_for(glyph: &[Contour]) -> f32 {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyph.iter().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let area = ((b.2 - b.0) * (b.3 - b.1)).max(1.0);
    (area / 1.0e6).sqrt().max(1.5)
}

/// Width of the widest part of a shape (the largest circle inside it), in
/// world units, for deciding between satin and fill.
pub fn widest(glyph: &[Contour], even_odd: bool, mask: &[MaskShape]) -> f32 {
    let cell = (cell_for(glyph) * 2.0).max(2.0);
    Field::new(glyph, even_odd, mask, cell).map_or(0.0, |f| 2.0 * f.d.iter().copied().fold(0.0, f32::max))
}

/// Zhang–Suen thinning: peels the shape down to lines one cell wide.
fn thin(m: &mut [bool], w: usize, h: usize) {
    let mut del = Vec::new();
    loop {
        let mut changed = false;
        for step in 0..2 {
            del.clear();
            for j in 1..h - 1 {
                for i in 1..w - 1 {
                    if !m[j * w + i] {
                        continue;
                    }
                    let at = |di: isize, dj: isize| m[(j as isize + dj) as usize * w + (i as isize + di) as usize];
                    let n = [at(0, -1), at(1, -1), at(1, 0), at(1, 1), at(0, 1), at(-1, 1), at(-1, 0), at(-1, -1)];
                    let b = n.iter().filter(|x| **x).count();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    if (0..8).filter(|&k| !n[k] && n[(k + 1) % 8]).count() != 1 {
                        continue;
                    }
                    let (p2, p4, p6, p8) = (n[0], n[2], n[4], n[6]);
                    let keep = if step == 0 { (p2 && p4 && p6) || (p4 && p6 && p8) } else { (p2 && p4 && p8) || (p2 && p6 && p8) };
                    if !keep {
                        del.push(j * w + i);
                    }
                }
            }
            changed |= !del.is_empty();
            for &k in &del {
                m[k] = false;
            }
        }
        if !changed {
            break;
        }
    }
}

const RING: [(isize, isize); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

#[derive(Clone, Debug)]
struct Edge {
    /// End nodes; None for a loop with no node on it.
    a: Option<usize>,
    b: Option<usize>,
    pts: Vec<Pt>,
    closed: bool,
}

fn length(pts: &[Pt]) -> f32 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// The center lines of a shape as a graph of branches between nodes.
fn skeleton(f: &Field) -> (Vec<Pt>, Vec<Edge>) {
    let (w, h) = (f.w, f.h);
    let mut m: Vec<bool> = f.d.iter().map(|&d| d > 0.0).collect();
    thin(&mut m, w, h);
    let nbrs = |k: usize| -> Vec<usize> {
        let (i, j) = ((k % w) as isize, (k / w) as isize);
        RING.iter()
            .filter_map(|&(di, dj)| {
                let (x, y) = (i + di, j + dj);
                (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize)
            })
            .filter(|&q| m[q])
            .collect()
    };
    let crossings = |k: usize| -> usize {
        let (i, j) = ((k % w) as isize, (k / w) as isize);
        let on: Vec<bool> = RING.iter().map(|&(di, dj)| m[(j + dj) as usize * w + (i + di) as usize]).collect();
        (0..8).filter(|&t| !on[t] && on[(t + 1) % 8]).count()
    };
    // Node pixels: line ends and branch points, grouped into clusters.
    let mut node_of: Vec<Option<usize>> = vec![None; w * h];
    let mut nodes: Vec<Vec<usize>> = Vec::new();
    for k in 0..w * h {
        if !m[k] || node_of[k].is_some() {
            continue;
        }
        let c = crossings(k);
        if c == 2 && nbrs(k).len() >= 2 {
            continue;
        }
        if nbrs(k).is_empty() {
            continue;
        }
        // Flood the cluster of adjacent node pixels.
        let id = nodes.len();
        let mut cluster = vec![k];
        node_of[k] = Some(id);
        let mut s = 0;
        while s < cluster.len() {
            for q in nbrs(cluster[s]) {
                if node_of[q].is_none() && (crossings(q) != 2 || nbrs(q).len() < 2) {
                    node_of[q] = Some(id);
                    cluster.push(q);
                }
            }
            s += 1;
        }
        nodes.push(cluster);
    }
    let node_pos: Vec<Pt> = nodes
        .iter()
        .map(|c| {
            let n = c.len() as f32;
            let s = c.iter().map(|&k| f.center(k)).fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            (s.0 / n, s.1 / n)
        })
        .collect();
    let mut seen = vec![false; w * h];
    let mut edges = Vec::new();
    // Walks from `start` along unvisited line pixels, preferring side
    // neighbors over diagonal ones so no pixel is skipped.
    let walk = |start: usize, from: usize, seen: &mut Vec<bool>, origin: Option<usize>| -> (Vec<usize>, Option<usize>) {
        let mut path = vec![start];
        seen[start] = true;
        let (mut prev, mut cur) = (from, start);
        loop {
            let mut cand: Vec<usize> = nbrs(cur).into_iter().filter(|&q| q != prev).collect();
            // A node ends the branch, unless it is where the walk began.
            if let Some(&q) = cand.iter().find(|&&q| node_of[q].is_some() && (node_of[q] != origin || path.len() > 2)) {
                return (path, node_of[q]);
            }
            cand.retain(|&q| !seen[q] && node_of[q].is_none());
            let side = |q: &usize| ((q % w) as isize - (cur % w) as isize).abs() + ((q / w) as isize - (cur / w) as isize).abs();
            cand.sort_by_key(side);
            let Some(&next) = cand.first() else { return (path, None) };
            seen[next] = true;
            path.push(next);
            prev = cur;
            cur = next;
        }
    };
    for (id, cluster) in nodes.iter().enumerate() {
        for &k in cluster {
            for q in nbrs(k) {
                if seen[q] || node_of[q].is_some() {
                    continue;
                }
                let (path, end) = walk(q, k, &mut seen, Some(id));
                let mut pts = vec![node_pos[id]];
                pts.extend(path.iter().map(|&p| f.center(p)));
                if let Some(e) = end {
                    pts.push(node_pos[e]);
                }
                edges.push(Edge { a: Some(id), b: end, pts, closed: false });
            }
        }
        // Two node clusters touching directly.
        for &k in cluster {
            for q in nbrs(k) {
                if let Some(o) = node_of[q] {
                    if o > id && !edges.iter().any(|e| (e.a == Some(id) && e.b == Some(o)) || (e.a == Some(o) && e.b == Some(id))) {
                        edges.push(Edge { a: Some(id), b: Some(o), pts: vec![node_pos[id], node_pos[o]], closed: false });
                    }
                }
            }
        }
    }
    // Loops with no node on them, like the ring of an O.
    for k in 0..w * h {
        if !m[k] || seen[k] || node_of[k].is_some() {
            continue;
        }
        let first = nbrs(k).into_iter().next().unwrap_or(k);
        let (path, _) = walk(k, first, &mut seen, None);
        if path.len() > 3 {
            let pts: Vec<Pt> = path.iter().map(|&p| f.center(p)).collect();
            edges.push(Edge { a: None, b: None, pts, closed: true });
        }
    }
    // Single cells, left by small round blobs like the dot of an i.
    for k in 0..w * h {
        if m[k] && nbrs(k).is_empty() {
            let c = f.center(k);
            edges.push(Edge { a: None, b: None, pts: vec![(c.0 - 1.0, c.1), (c.0 + 1.0, c.1)], closed: false });
        }
    }
    (node_pos, edges)
}

fn degree(edges: &[Edge], n: usize) -> usize {
    edges.iter().map(|e| (e.a == Some(n)) as usize + (e.b == Some(n)) as usize).sum()
}

/// Removes short spurs that thinning leaves in corners, then joins
/// branches that meet at nodes with only two branches.
fn prune(f: &Field, nodes: &[Pt], mut edges: Vec<Edge>) -> Vec<Edge> {
    loop {
        let mut cut = None;
        for (i, e) in edges.iter().enumerate() {
            if e.closed {
                continue;
            }
            let ends = [(e.a, e.b), (e.b, e.a)];
            for (tip, base) in ends {
                let tip_free = tip.is_none_or(|t| degree(&edges, t) == 1);
                let Some(base) = base else { continue };
                if tip_free && degree(&edges, base) >= 3 {
                    let r = f.at(nodes[base]);
                    if length(&e.pts) < 1.3 * r + 8.0 {
                        cut = Some(i);
                    }
                }
            }
            if cut.is_some() {
                break;
            }
        }
        match cut {
            Some(i) => {
                edges.remove(i);
            }
            None => break,
        }
    }
    // Join through nodes of degree two.
    loop {
        let mut joined = false;
        for n in 0..nodes.len() {
            let at: Vec<usize> = (0..edges.len()).filter(|&i| edges[i].a == Some(n) || edges[i].b == Some(n)).collect();
            if at.len() == 1 && edges[at[0]].a == Some(n) && edges[at[0]].b == Some(n) {
                // A loop through a single node.
                edges[at[0]].closed = true;
                edges[at[0]].a = None;
                edges[at[0]].b = None;
                edges[at[0]].pts.pop();
                joined = true;
                break;
            }
            if at.len() != 2 {
                continue;
            }
            let (i, j) = (at[0], at[1]);
            let mut e1 = edges[i].clone();
            let mut e2 = edges[j].clone();
            if e1.a == Some(n) {
                e1.pts.reverse();
                std::mem::swap(&mut e1.a, &mut e1.b);
            }
            if e2.b == Some(n) {
                e2.pts.reverse();
                std::mem::swap(&mut e2.a, &mut e2.b);
            }
            let mut pts = e1.pts;
            pts.extend(e2.pts.into_iter().skip(1));
            let merged = if e1.a == e2.b && e1.a.is_some() {
                pts.pop();
                Edge { a: None, b: None, pts, closed: true }
            } else {
                Edge { a: e1.a, b: e2.b, pts, closed: false }
            };
            edges.remove(j.max(i));
            edges.remove(j.min(i));
            edges.push(merged);
            joined = true;
            break;
        }
        if !joined {
            break;
        }
    }
    edges
}

/// Even spacing along a line; for closed lines the last point comes back
/// to the start.
fn resample(pts: &[Pt], step: f32, closed: bool) -> Vec<Pt> {
    let mut path = pts.to_vec();
    if closed && !pts.is_empty() {
        path.push(pts[0]);
    }
    if path.len() < 2 {
        return path;
    }
    let total = length(&path);
    let n = (total / step).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(n + 1);
    let (mut seg, mut along) = (0usize, 0.0f32);
    for k in 0..=n {
        let target = total * k as f32 / n as f32;
        while seg + 1 < path.len() - 1 && along + dist(path[seg], path[seg + 1]) < target {
            along += dist(path[seg], path[seg + 1]);
            seg += 1;
        }
        let l = dist(path[seg], path[seg + 1]).max(1e-6);
        let t = ((target - along) / l).clamp(0.0, 1.0);
        let (a, b) = (path[seg], path[seg + 1]);
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
    if closed {
        out.pop();
    }
    out
}

/// Moving average that irons out the grid's stair steps; open lines keep
/// their ends.
fn smooth(pts: &[Pt], radius: usize, closed: bool) -> Vec<Pt> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if !closed && (i == 0 || i == n - 1) {
            out.push(pts[i]);
            continue;
        }
        let r = if closed { radius } else { radius.min(i).min(n - 1 - i) };
        let (mut sx, mut sy) = (0.0, 0.0);
        for k in 0..=2 * r {
            let idx = (i + n + k - r) % n;
            sx += pts[idx].0;
            sy += pts[idx].1;
        }
        let c = (2 * r + 1) as f32;
        out.push((sx / c, sy / c));
    }
    out
}

/// Where a ray from `p` along `dir` (unit) first meets an outline.
fn ray(p: Pt, dir: Pt, walls: &[(Pt, Pt)]) -> Option<f32> {
    let mut best: Option<f32> = None;
    for &(a, b) in walls {
        let e = (b.0 - a.0, b.1 - a.1);
        let den = dir.0 * e.1 - dir.1 * e.0;
        if den.abs() < 1e-9 {
            continue;
        }
        let w = (a.0 - p.0, a.1 - p.1);
        let t = (w.0 * e.1 - w.1 * e.0) / den;
        let u = (w.0 * dir.1 - w.1 * dir.0) / den;
        if t > 0.05 && (0.0..=1.0).contains(&u) && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    }
    best
}

/// One stitch row across the column: center, unit normal, and the reach
/// to each side.
#[derive(Clone, Copy, Debug)]
struct Row {
    c: Pt,
    n: Pt,
    left: f32,
    right: f32,
}

impl Row {
    fn side(&self, s: f32, extra: f32) -> Pt {
        let r = if s > 0.0 { self.left + extra } else { self.right + extra };
        (self.c.0 + self.n.0 * s * r, self.c.1 + self.n.1 * s * r)
    }
}

/// Rows along a smoothed branch every `step`, reaching to the outline.
fn rows(f: &Field, line: &[Pt], closed: bool, step: f32, walls: &[(Pt, Pt)], free: (bool, bool)) -> Vec<Row> {
    let mut line = line.to_vec();
    // Free ends run on toward the tip of the shape.
    if !closed && line.len() >= 2 {
        for front in [true, false] {
            if (front && !free.0) || (!front && !free.1) {
                continue;
            }
            let (e, back) = if front { (line[0], line[line.len().min(6) - 1]) } else { (line[line.len() - 1], line[line.len().saturating_sub(6)]) };
            let d = dist(e, back).max(1e-3);
            let u = ((e.0 - back.0) / d, (e.1 - back.1) / d);
            let r = f.at(e);
            let reach = ray(e, u, walls).unwrap_or(r).min(2.5 * r + 5.0) - 2.0;
            if reach > 1.0 {
                let tip = (e.0 + u.0 * reach, e.1 + u.1 * reach);
                if front {
                    line.insert(0, tip);
                } else {
                    line.push(tip);
                }
            }
        }
    }
    // Rows are tried four times as often as needed and kept once the
    // center or either edge has moved a full step, so the outside of a
    // curve gets as many stitches as a straight part.
    let pts = resample(&line, step / 4.0, closed);
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let look = ((10.0 / (step / 4.0)).round() as usize).max(1);
    let mut all = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = if closed {
            (pts[(i + n - look % n) % n], pts[(i + look) % n])
        } else {
            (pts[i.saturating_sub(look)], pts[(i + look).min(n - 1)])
        };
        let d = dist(a, b).max(1e-3);
        let t = ((b.0 - a.0) / d, (b.1 - a.1) / d);
        let nrm = (-t.1, t.0);
        let c = pts[i];
        let r = f.at(c).max(1.0);
        // A center line a little off center still reaches both edges, as
        // long as the whole row is about as wide as the column. Longer rows
        // run down another branch at a junction, so they are cut short.
        let (hl, hr) = (ray(c, nrm, walls).unwrap_or(r), ray(c, (-nrm.0, -nrm.1), walls).unwrap_or(r));
        let cap = 1.2 * r + 2.0;
        let (left, right) = if hl + hr <= 2.4 * r + 4.0 { (hl, hr) } else { (hl.min(cap), hr.min(cap)) };
        all.push(Row { c, n: nrm, left, right });
    }
    let mut out: Vec<Row> = vec![all[0]];
    for (i, row) in all.iter().enumerate().skip(1) {
        let last = out[out.len() - 1];
        let moved = dist(row.c, last.c).max(dist(row.side(1.0, 0.0), last.side(1.0, 0.0))).max(dist(row.side(-1.0, 0.0), last.side(-1.0, 0.0)));
        if moved >= step * 0.9 || (!closed && i == n - 1) {
            out.push(*row);
        }
    }
    out
}

/// Cuts a center line where it turns sharply, like the corner of an L or
/// the points of an M. Each piece then runs square into the corner and the
/// next one starts where it stopped, instead of stitches fanning round it.
/// Returns the pieces, whether each is a closed loop, and which ends are free.
fn split_corners(f: &Field, line: &[Pt], closed: bool, free: (bool, bool)) -> Vec<(Vec<Pt>, bool, (bool, bool))> {
    let n = line.len();
    if n < 5 {
        return vec![(line.to_vec(), closed, free)];
    }
    // Turning angle at each point, measured over about the column's width.
    let turn: Vec<f32> = (0..n)
        .map(|i| {
            let reach = ((f.at(line[i]).max(10.0) * 2.0) / 5.0).round().max(2.0) as usize;
            let (a, b) = if closed {
                (line[(i + n - reach % n) % n], line[(i + reach) % n])
            } else {
                if i < reach || i + reach >= n {
                    return 0.0;
                }
                (line[i - reach], line[i + reach])
            };
            let c = line[i];
            let (u, v) = ((c.0 - a.0, c.1 - a.1), (b.0 - c.0, b.1 - c.1));
            let (lu, lv) = ((u.0 * u.0 + u.1 * u.1).sqrt(), (v.0 * v.0 + v.1 * v.1).sqrt());
            if lu < 1e-3 || lv < 1e-3 {
                return 0.0;
            }
            ((u.0 * v.0 + u.1 * v.1) / (lu * lv)).clamp(-1.0, 1.0).acos().to_degrees()
        })
        .collect();
    // Corners: local peaks over 60°, at least a column width apart.
    let mut cuts: Vec<usize> = Vec::new();
    for i in 0..n {
        if turn[i] < 60.0 {
            continue;
        }
        let w = 3;
        let peak = (1..=w).all(|k| {
            let (p, q) = if closed { ((i + n - k) % n, (i + k) % n) } else { (i.saturating_sub(k), (i + k).min(n - 1)) };
            turn[i] >= turn[p] && turn[i] >= turn[q]
        });
        if peak && cuts.last().is_none_or(|&l| i - l > 4) {
            cuts.push(i);
        }
    }
    if cuts.is_empty() {
        return vec![(line.to_vec(), closed, free)];
    }
    let mut out = Vec::new();
    // Each piece stops short of the corner by the column's half width, where
    // the bend begins, so its free end runs straight on into the corner.
    let trim = |piece: Vec<Pt>, front: bool, back: bool| -> Vec<Pt> {
        let mut p = piece;
        let (first, last) = (p[0], p[p.len() - 1]);
        let (rf, rb) = (f.at(first).max(10.0), f.at(last).max(10.0));
        if front {
            while p.len() > 3 && dist(p[1], first) < rf {
                p.remove(0);
            }
        }
        if back {
            while p.len() > 3 && dist(p[p.len() - 2], last) < rb {
                p.pop();
            }
        }
        p
    };
    if closed {
        for k in 0..cuts.len() {
            let (a, b) = (cuts[k], cuts[(k + 1) % cuts.len()]);
            let mut piece = Vec::new();
            let mut i = a;
            loop {
                piece.push(line[i]);
                if i == b && piece.len() > 1 {
                    break;
                }
                i = (i + 1) % n;
            }
            out.push((trim(piece, true, true), false, (true, true)));
        }
    } else {
        let mut start = 0;
        for (k, &c) in cuts.iter().enumerate() {
            out.push((trim(line[start..=c].to_vec(), k > 0, true), false, (if k == 0 { free.0 } else { true }, true)));
            start = c;
        }
        out.push((trim(line[start..].to_vec(), true, false), false, (true, free.1)));
    }
    out
}

/// Fills a shape with satin columns that follow its center lines.
pub fn column_fill(st: &mut Stitcher, glyph: &[Contour], even_odd: bool, mask: &[MaskShape], s: &Settings) {
    let cell = cell_for(glyph);
    let Some(f) = Field::new(glyph, even_odd, mask, cell) else { return };
    let (nodes, edges) = skeleton(&f);
    let edges = prune(&f, &nodes, edges);
    let mut walls: Vec<(Pt, Pt)> = Vec::new();
    for c in glyph.iter().chain(mask.iter().flat_map(|(cs, _)| cs.iter())) {
        for i in 0..c.len() {
            walls.push((c[i], c[(i + 1) % c.len()]));
        }
    }
    let step = (s.density_mm * 10.0).clamp(2.5, 10.0);
    let pull = s.pull_mm * 10.0;
    // Satin wider than this is split by a needle point in the middle.
    let split = 70.0;
    let mut branches: Vec<(Vec<Row>, bool)> = Vec::new();
    for e in &edges {
        let line = smooth(&smooth(&resample(&e.pts, 5.0, e.closed), 3, e.closed), 3, e.closed);
        let free = (e.a.is_none_or(|n| degree(&edges, n) <= 1), e.b.is_none_or(|n| degree(&edges, n) <= 1));
        for (piece, closed, ends) in split_corners(&f, &line, e.closed, free) {
            let r = rows(&f, &piece, closed, step, &walls, ends);
            if r.len() >= 2 {
                branches.push((r, closed));
            }
        }
    }
    let mut covered = vec![false; f.w * f.h];
    let is_covered = |covered: &Vec<bool>, p: Pt| f.index(p).is_some_and(|k| covered[k]);
    while !branches.is_empty() {
        // The branch with an end nearest the needle.
        let here = st.last();
        let (bi, flip, rot) = {
            let mut best = (0, false, 0, f32::MAX);
            for (i, (r, closed)) in branches.iter().enumerate() {
                let Some(h) = here else { break };
                if *closed {
                    for (k, row) in r.iter().enumerate() {
                        let d = dist(row.c, h);
                        if d < best.3 {
                            best = (i, false, k, d);
                        }
                    }
                } else {
                    for (flip, row) in [(false, r[0]), (true, r[r.len() - 1])] {
                        let d = dist(row.c, h);
                        if d < best.3 {
                            best = (i, flip, 0, d);
                        }
                    }
                }
            }
            (best.0, best.1, best.2)
        };
        let (mut r, closed) = branches.swap_remove(bi);
        if closed {
            r.rotate_left(rot);
            let first = r[0];
            r.push(first);
        }
        if flip {
            r.reverse();
            for row in r.iter_mut() {
                row.n = (-row.n.0, -row.n.1);
                std::mem::swap(&mut row.left, &mut row.right);
            }
        }
        // Leave out the ends another branch already covered.
        while r.len() >= 2 && is_covered(&covered, r[0].c) {
            r.remove(0);
        }
        while r.len() >= 2 && is_covered(&covered, r[r.len() - 1].c) {
            r.pop();
        }
        if r.len() < 2 {
            continue;
        }
        let wide = match s.underlay_kind {
            lettering::Underlay::Edge => false,
            lettering::Underlay::Zigzag | lettering::Underlay::Double | lettering::Underlay::Tatami => true,
            lettering::Underlay::Auto => r.iter().map(|w| w.left + w.right).sum::<f32>() / r.len() as f32 > 45.0,
        };
        let centers: Vec<Pt> = r.iter().map(|w| w.c).collect();
        if s.underlay {
            // Center run there, then (when wide) a loose zigzag back.
            let run = resample(&centers, 20.0, false);
            st.travel(run[0], run.get(1).copied().unwrap_or(run[0]));
            for p in &run[1..] {
                st.stitch(*p);
            }
            if wide {
                let k = ((20.0 / step).round() as usize).max(1);
                let mut side = 1.0;
                for row in r.iter().rev().step_by(k) {
                    let inset = Row { left: (row.left - 6.0).max(row.left * 0.5), right: (row.right - 6.0).max(row.right * 0.5), ..*row };
                    st.stitch(inset.side(side, 0.0));
                    side = -side;
                }
                r.reverse();
                for row in r.iter_mut() {
                    row.n = (-row.n.0, -row.n.1);
                    std::mem::swap(&mut row.left, &mut row.right);
                }
            } else {
                r.reverse();
                for row in r.iter_mut() {
                    row.n = (-row.n.0, -row.n.1);
                    std::mem::swap(&mut row.left, &mut row.right);
                }
            }
        }
        let first = r[0].side(1.0, pull);
        st.travel(first, r[1].side(-1.0, pull));
        let mut last_side: [Option<Pt>; 2] = [None, None];
        let mut prev = first;
        for (i, row) in r.iter().enumerate() {
            let s_ = if i % 2 == 0 { 1.0 } else { -1.0 };
            let mut p = row.side(s_, pull);
            // Inside a tight curve the needle would keep landing in the same
            // spot: shorten every other stitch there.
            let slot = (i % 2) as usize;
            if let Some(q) = last_side[slot] {
                if dist(p, q) < step * 0.7 && i % 4 >= 2 {
                    p = (row.c.0 + (p.0 - row.c.0) * 0.6, row.c.1 + (p.1 - row.c.1) * 0.6);
                }
            }
            last_side[slot] = Some(p);
            let span = dist(prev, p);
            if span > split && i > 0 {
                let pieces = (span / split).ceil() as usize;
                // Stagger the extra needle points so they don't line up.
                let shift = if i % 4 < 2 { 0.35 } else { 0.65 };
                for k in 0..pieces - 1 {
                    let t = (k as f32 + shift) / pieces as f32;
                    st.stitch((prev.0 + (p.0 - prev.0) * t, prev.1 + (p.1 - prev.1) * t));
                }
            }
            st.stitch(p);
            // What this row covers, from edge to edge.
            mark(&f, &mut covered, row.side(1.0, 0.0), row.side(-1.0, 0.0));
            prev = p;
        }
    }
    // Whatever no column reached, where branches meet at odd angles, is
    // filled with tatami so nothing is left bare.
    let field: Vec<f32> = (0..f.w * f.h).map(|k| if f.d[k] > 0.0 && !covered[k] { 1.0 } else { 0.0 }).collect();
    let center = |i: usize, j: usize| f.center(j * f.w + i);
    let rings: Vec<Contour> = lettering::iso_lines(&field, f.w, f.h, 0.5, &center)
        .into_iter()
        .filter(|r| lettering::shape_area(std::slice::from_ref(r), true) > 300.0)
        .collect();
    if !rings.is_empty() && lettering::thickness(&rings, true) > 6.0 {
        let t = Settings { pull_mm: 0.0, ..s.clone() };
        lettering::fill_shape(st, &rings, true, &[], lettering::FillKind::Tatami, s.fill_angle, &t);
    }
}

/// Marks the cells a stitch crosses, and their neighbors, as covered.
fn mark(f: &Field, covered: &mut [bool], a: Pt, b: Pt) {
    let n = (dist(a, b) / (f.cell * 0.5)).ceil().max(1.0) as usize;
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        if let Some(c) = f.index(p) {
            let (i, j) = (c % f.w, c / f.w);
            for dj in 0..3 {
                for di in 0..3 {
                    let (x, y) = ((i + di).wrapping_sub(1), (j + dj).wrapping_sub(1));
                    if x < f.w && y < f.h {
                        covered[y * f.w + x] = true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::Cmd;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Contour {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    fn ring(r_out: f32, r_in: f32) -> Vec<Contour> {
        let circle = |r: f32, rev: bool| -> Contour {
            let mut c: Contour = (0..96).map(|k| {
                let a = k as f32 / 96.0 * std::f32::consts::TAU;
                (r * a.cos(), r * a.sin())
            }).collect();
            if rev {
                c.reverse();
            }
            c
        };
        vec![circle(r_out, false), circle(r_in, true)]
    }

    fn sew(glyph: &[Contour]) -> Stitcher {
        let mut st = Stitcher::new();
        let s = Settings { density_mm: 0.4, pull_mm: 0.0, ..Settings::default() };
        column_fill(&mut st, glyph, false, &[], &s);
        st
    }

    fn stitched(st: &Stitcher) -> Vec<Pt> {
        st.points.iter().zip(&st.cmds).filter(|(_, c)| **c == Cmd::Stitch).map(|(p, _)| *p).collect()
    }

    #[test]
    fn a_bar_is_one_column_across_its_width() {
        // 40 × 5 mm bar: one branch, stitches straight across, reaching both
        // long edges and close to both ends.
        let g = vec![rect(0.0, 0.0, 400.0, 50.0)];
        let f = Field::new(&g, false, &[], cell_for(&g)).unwrap();
        let (nodes, edges) = skeleton(&f);
        let edges = prune(&f, &nodes, edges);
        assert_eq!(edges.len(), 1, "{edges:?}");
        let st = sew(&g);
        let pts = stitched(&st);
        let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
        assert!(ys.iter().any(|&y| y < 2.0) && ys.iter().any(|&y| y > 48.0));
        let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
        let (lo, hi) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
        assert!(lo < 8.0 && hi > 392.0, "{lo} {hi}");
        // Everything stays inside the bar.
        assert!(pts.iter().all(|p| p.0 > -1.0 && p.0 < 401.0 && p.1 > -1.0 && p.1 < 51.0));
        // A satin stitch every 0.4 mm along the bar, plus the underlay.
        assert!(pts.len() > 100 && pts.len() < 200, "{}", pts.len());
    }

    #[test]
    fn a_ring_is_one_closed_column_that_turns() {
        let g = ring(200.0, 150.0);
        let f = Field::new(&g, false, &[], cell_for(&g)).unwrap();
        let (nodes, edges) = skeleton(&f);
        let edges = prune(&f, &nodes, edges);
        assert_eq!(edges.len(), 1);
        assert!(edges[0].closed);
        let st = sew(&g);
        // Stitches reach out near both circles all the way round.
        let pts = stitched(&st);
        let radii: Vec<f32> = pts.iter().map(|p| (p.0 * p.0 + p.1 * p.1).sqrt()).collect();
        assert!(radii.iter().all(|&r| r > 145.0 && r < 205.0), "{:?}", radii.iter().fold((f32::MAX, f32::MIN), |a, &r| (a.0.min(r), a.1.max(r))));
        for q in 0..8 {
            let a0 = q as f32 / 8.0 * std::f32::consts::TAU - std::f32::consts::PI;
            let near = pts.iter().filter(|p| {
                let d = (p.1.atan2(p.0) - a0).rem_euclid(std::f32::consts::TAU);
                d < 0.2 || d > std::f32::consts::TAU - 0.2
            }).count();
            assert!(near > 12, "octant {q}: {near}");
        }
        // Each satin stitch runs roughly toward the center (square to the ring).
        let mut square = 0;
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = dist(a, b);
            if d > 30.0 {
                let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                let m = (mid.0 * mid.0 + mid.1 * mid.1).sqrt();
                let cos = ((b.0 - a.0) * mid.0 + (b.1 - a.1) * mid.1).abs() / (d * m);
                if cos > 0.85 {
                    square += 1;
                }
            }
        }
        assert!(square > 200, "{square}");
    }

    #[test]
    fn a_t_shape_does_not_sew_the_crossing_twice() {
        // A bar with a stem below it.
        let g = vec![vec![(0.0, 0.0), (300.0, 0.0), (300.0, 40.0), (170.0, 40.0), (170.0, 300.0), (130.0, 300.0), (130.0, 40.0), (0.0, 40.0)]];
        let st = sew(&g);
        let pts = stitched(&st);
        // Penetrations in the crossing square: about one column's worth, not two or three.
        let inside = pts.iter().filter(|p| p.0 > 135.0 && p.0 < 165.0 && p.1 > 5.0 && p.1 < 35.0).count();
        let one_col = pts.iter().filter(|p| p.0 > 35.0 && p.0 < 65.0 && p.1 > 5.0 && p.1 < 35.0).count();
        assert!(inside as f32 <= one_col as f32 * 1.8 + 6.0, "crossing {inside} vs bar {one_col}");
        // The stem is sewn down to its end.
        assert!(pts.iter().any(|p| p.1 > 290.0));
    }

    #[test]
    fn widest_measures_the_largest_circle() {
        let w = widest(&[rect(0.0, 0.0, 400.0, 50.0)], false, &[]);
        assert!((w - 50.0).abs() < 6.0, "{w}");
    }
}
#[cfg(test)]
pub(crate) mod debug {
    use super::*;
    use resvg::tiny_skia;

    /// Draws outlines in gray and stitches in red into a PNG, `px` pixels per mm.
    pub fn render(path: &str, glyphs: &[Vec<Contour>], points: &[Pt], cmds: &[crate::formats::Cmd], px: f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in glyphs.iter().flatten().flatten().chain(points.iter()) {
            b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
        }
        let k = px / 10.0;
        let (w, h) = (((b.2 - b.0) * k) as u32 + 40, ((b.3 - b.1) * k) as u32 + 40);
        let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
        pm.fill(tiny_skia::Color::WHITE);
        let tr = |p: Pt| ((p.0 - b.0) * k + 20.0, (p.1 - b.1) * k + 20.0);
        for g in glyphs {
            let mut pb = tiny_skia::PathBuilder::new();
            for c in g {
                let q = tr(c[0]);
                pb.move_to(q.0, q.1);
                for &p in &c[1..] {
                    let q = tr(p);
                    pb.line_to(q.0, q.1);
                }
                pb.close();
            }
            if let Some(path) = pb.finish() {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(215, 215, 215, 255);
                pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
            }
        }
        let mut pb = tiny_skia::PathBuilder::new();
        for i in 1..points.len() {
            if cmds[i] == crate::formats::Cmd::Stitch {
                let (a, c) = (tr(points[i - 1]), tr(points[i]));
                pb.move_to(a.0, a.1);
                pb.line_to(c.0, c.1);
            }
        }
        if let Some(path) = pb.finish() {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(190, 20, 40, 200);
            paint.anti_alias = true;
            pm.stroke_path(&path, &paint, &tiny_skia::Stroke { width: 0.6, ..Default::default() }, tiny_skia::Transform::identity(), None);
        }
        pm.save_png(path).unwrap();
    }

    /// HOOP_OUT, T (text), H (height mm), F (font index): renders lettering
    /// sewn as columns.
    #[test]
    #[ignore]
    fn column_letters() {
        let out = std::env::var("HOOP_OUT").unwrap_or_else(|_| "/tmp".into());
        let fonts = crate::lettering::bundled_fonts();
        let font = &fonts[std::env::var("F").ok().and_then(|v| v.parse().ok()).unwrap_or(0)];
        let fill = match std::env::var("FILL").as_deref() {
            Ok("auto") => crate::lettering::FillKind::Auto,
            Ok("stipple") => crate::lettering::FillKind::Stipple,
            Ok("cross") => crate::lettering::FillKind::Cross,
            _ => crate::lettering::FillKind::Column,
        };
        let edge = match std::env::var("EDGE").as_deref() {
            Ok("blanket") => crate::lettering::EdgeKind::Blanket,
            Ok("scallop") => crate::lettering::EdgeKind::Scallop,
            Ok("triple") => crate::lettering::EdgeKind::Triple,
            _ => crate::lettering::EdgeKind::None,
        };
        let shape = std::env::var("SHAPE").ok().and_then(|v| crate::lettering::TextShape::ALL.into_iter().find(|t| format!("{t:?}") == v)).unwrap_or(crate::lettering::TextShape::Straight);
        let s = Settings {
            text: std::env::var("T").unwrap_or("Be".into()).replace("\\n", "\n"),
            height_mm: std::env::var("H").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0),
            fill,
            shape,
            envelope_mm: std::env::var("ENV").ok().and_then(|v| v.parse().ok()).unwrap_or(8.0),
            circle_mm: 70.0,
            density_mm: std::env::var("ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(fill.default_rows_mm()),
            edge_width_mm: 2.5,
            edge,
            ..Settings::default()
        };
        let p = crate::lettering::generate(font, &s).unwrap();
        let g = crate::lettering::outlines(font, &s);
        render(&format!("{out}/letters.png"), &g, &p.points, &p.cmds, std::env::var("PX").ok().and_then(|v| v.parse().ok()).unwrap_or(8.0));
    }
}
