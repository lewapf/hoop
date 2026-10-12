//! Travel inside a shape. When the needle has to move between two parts of
//! the same fill, it runs a line of short stitches through the shape instead
//! of jumping, so the machine doesn't trim and tie off for every column.
//! The path prefers ground that the rows still to come will cover: parts
//! not sewn yet, and parts cut out for shapes on top, which sew later.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::stitch::{Contour, MaskShape, Pt, dist, mask_spans, row_spans, subtract};

/// A shape on a grid of square cells.
pub struct Region {
    x0: f32,
    y0: f32,
    cell: f32,
    w: usize,
    h: usize,
    inside: Vec<bool>,
    /// Inside cells cut out for shapes on top; those shapes hide anything
    /// run here, so it never counts as sewn.
    hidden: Vec<bool>,
    sewn: Vec<bool>,
    /// Travel may not cross sewn ground away from its ends, because nothing
    /// will cover it. Off while sewing underlay, which the fill hides.
    pub strict: bool,
}

/// Points of a travel run are at most this far apart (2.5 mm).
const STEP: f32 = 25.0;
/// How far a start or end point may lie outside the grid's shape (1.5 mm);
/// row ends carry pull compensation and sit just past the outline.
const SNAP: f32 = 15.0;
/// Extra cost per unit length of travelling over stitches already sewn.
const SEWN_COST: f32 = 3.0;

impl Region {
    /// `cut` holds the shapes cut out of this one and `cover` those of them
    /// that sew densely enough to hide a travel run; the rest of the cut-out
    /// area stays off limits.
    pub fn new(glyph: &[Contour], even_odd: bool, cut: &[MaskShape], cover: &[MaskShape]) -> Option<Region> {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in glyph.iter().flatten() {
            b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
        }
        if b.2 <= b.0 || b.3 <= b.1 {
            return None;
        }
        // About 0.5 mm cells, coarser for big shapes to bound the work.
        let cell = ((b.2 - b.0).max(b.3 - b.1) / 300.0).max(5.0);
        let (x0, y0) = (b.0 - cell, b.1 - cell);
        let w = ((b.2 - b.0) / cell) as usize + 3;
        let h = ((b.3 - b.1) / cell) as usize + 3;
        // Marks the cells whose centers fall in row `j`'s spans.
        let fill = |grid: &mut [bool], j: usize, spans: &[(f32, f32)], on: bool| {
            for &(a, bb) in spans {
                let i0 = ((a - x0) / cell - 0.5).ceil().max(0.0) as usize;
                let i1 = ((bb - x0) / cell - 0.5).floor();
                if i1 < 0.0 {
                    continue;
                }
                for i in i0..=(i1 as usize).min(w - 1) {
                    grid[j * w + i] = on;
                }
            }
        };
        let mut inside = vec![false; w * h];
        let mut hidden = vec![false; w * h];
        for j in 0..h {
            let y = y0 + (j as f32 + 0.5) * cell;
            let spans = row_spans(glyph, y, even_odd);
            if cut.is_empty() {
                fill(&mut inside, j, &spans, true);
                continue;
            }
            fill(&mut inside, j, &subtract(spans.clone(), &mask_spans(cut, y)), true);
            if !cover.is_empty() {
                // The covered part of this shape: the shape minus what's uncovered.
                let under = subtract(spans.clone(), &subtract(spans, &mask_spans(cover, y)));
                fill(&mut inside, j, &under, true);
                fill(&mut hidden, j, &under, true);
            }
        }
        Some(Region { x0, y0, cell, w, h, sewn: vec![false; w * h], inside, hidden, strict: true })
    }

    fn index(&self, p: Pt) -> Option<usize> {
        let i = ((p.0 - self.x0) / self.cell).floor();
        let j = ((p.1 - self.y0) / self.cell).floor();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.w && (j as usize) < self.h).then(|| j as usize * self.w + i as usize)
    }

    fn center(&self, k: usize) -> Pt {
        (self.x0 + ((k % self.w) as f32 + 0.5) * self.cell, self.y0 + ((k / self.w) as f32 + 0.5) * self.cell)
    }

    /// Forgets what was sewn, at the start of a new pass over the shape.
    pub fn clear_sewn(&mut self) {
        self.sewn.fill(false);
    }

    /// Notes that a stitch from `a` to `b` lies on the fabric.
    pub fn mark(&mut self, a: Pt, b: Pt) {
        let n = (dist(a, b) / (self.cell / 2.0)).ceil().max(1.0) as usize;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            if let Some(i) = self.index((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)) {
                self.sewn[i] = !self.hidden[i];
            }
        }
    }

    /// The inside cell nearest to `p`, if one is close enough.
    fn snap(&self, p: Pt) -> Option<usize> {
        if let Some(k) = self.index(p).filter(|&k| self.inside[k]) {
            return Some(k);
        }
        let r = (SNAP / self.cell).ceil() as isize;
        let (ci, cj) = (((p.0 - self.x0) / self.cell).floor() as isize, ((p.1 - self.y0) / self.cell).floor() as isize);
        let mut best: Option<(f32, usize)> = None;
        for j in (cj - r).max(0)..=(cj + r).min(self.h as isize - 1) {
            for i in (ci - r).max(0)..=(ci + r).min(self.w as isize - 1) {
                let k = j as usize * self.w + i as usize;
                let d = dist(self.center(k), p);
                if self.inside[k] && d <= SNAP && best.is_none_or(|b| d < b.0) {
                    best = Some((d, k));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Cells crossed by the line between two cell centers, or None if it
    /// leaves the shape. Also counts the sewn cells it crosses.
    fn line(&self, a: usize, b: usize) -> Option<usize> {
        let (pa, pb) = (self.center(a), self.center(b));
        let n = (dist(pa, pb) / (self.cell / 3.0)).ceil().max(1.0) as usize;
        let mut sewn = 0;
        let mut last = usize::MAX;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let c = self.index((pa.0 + (pb.0 - pa.0) * t, pa.1 + (pb.1 - pa.1) * t))?;
            if !self.inside[c] {
                return None;
            }
            if c != last && self.sewn[c] {
                sewn += 1;
            }
            last = c;
        }
        Some(sewn)
    }

    /// A path from `from` to `to` that stays inside the shape, as the points
    /// to stitch after `from` (ending at `to`), or None when the two aren't
    /// connected inside or the way round is much longer than a jump.
    pub fn route(&self, from: Pt, to: Pt) -> Option<Vec<Pt>> {
        let (s, t) = (self.snap(from)?, self.snap(to)?);
        let cells = self.search(s, t, from, to)?;
        // Pull the cell chain straight wherever a direct line stays inside
        // and crosses no more sewn ground than the chain did.
        let sewn_before: Vec<usize> = cells
            .iter()
            .scan(0, |n, &c| {
                *n += self.sewn[c] as usize;
                Some(*n)
            })
            .collect();
        let mut corners = vec![cells[0]];
        let mut i = 0;
        while i < cells.len() - 1 {
            let mut k = i + 1;
            while k + 1 < cells.len() && self.line(cells[i], cells[k + 1]).is_some_and(|n| n <= sewn_before[k + 1] - sewn_before[i] + !self.strict as usize) {
                k += 1;
            }
            corners.push(cells[k]);
            i = k;
        }
        let mut line = vec![from];
        line.extend(corners.iter().map(|&c| self.center(c)));
        line.push(to);
        line.dedup_by(|a, b| dist(*a, *b) < 1.0);
        let length: f32 = line.windows(2).map(|w| dist(w[0], w[1])).sum();
        let straight = dist(from, to);
        if length > straight * 2.5 + 200.0 {
            return None;
        }
        // Even steps along the line, every corner kept.
        let mut out = Vec::new();
        for w in line.windows(2) {
            let n = (dist(w[0], w[1]) / STEP).ceil().max(1.0) as usize;
            for k in 1..=n {
                let t = k as f32 / n as f32;
                out.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
            }
        }
        Some(out)
    }

    /// Cheapest chain of inside cells from `s` to `t` (8 neighbours, sewn
    /// cells cost more, or are off limits away from the ends when strict).
    fn search(&self, s: usize, t: usize, from: Pt, to: Pt) -> Option<Vec<usize>> {
        let mut cost = vec![f32::INFINITY; self.w * self.h];
        let mut prev = vec![usize::MAX; self.w * self.h];
        let mut heap = BinaryHeap::new();
        cost[s] = 0.0;
        heap.push(Reverse((0u32, s)));
        let goal = self.center(t);
        while let Some(Reverse((_, k))) = heap.pop() {
            if k == t {
                let mut path = vec![t];
                while *path.last().unwrap() != s {
                    path.push(prev[*path.last().unwrap()]);
                }
                path.reverse();
                return Some(path);
            }
            let (i, j) = ((k % self.w) as isize, (k / self.w) as isize);
            for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (ni, nj) = (i + di, j + dj);
                if ni < 0 || nj < 0 || ni >= self.w as isize || nj >= self.h as isize {
                    continue;
                }
                let n = nj as usize * self.w + ni as usize;
                // Diagonals must not squeeze between two outside cells.
                if !self.inside[n]
                    || (di != 0 && dj != 0 && !(self.inside[j as usize * self.w + ni as usize] && self.inside[nj as usize * self.w + i as usize]))
                {
                    continue;
                }
                if self.strict && self.sewn[n] && dist(self.center(n), from) > SNAP && dist(self.center(n), to) > SNAP {
                    continue;
                }
                let step = if di != 0 && dj != 0 { std::f32::consts::SQRT_2 } else { 1.0 } * self.cell;
                let c = cost[k] + step * if self.sewn[n] { 1.0 + SEWN_COST } else { 1.0 };
                if c < cost[n] {
                    cost[n] = c;
                    prev[n] = k;
                    // A* with straight-line distance, which never overestimates.
                    let f = c + dist(self.center(n), goal);
                    heap.push(Reverse((f.to_bits(), n)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Contour {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    #[test]
    fn routes_around_a_hole_inside_the_shape() {
        // A 40 × 40 mm square with a 20 × 20 mm hole in the middle.
        let shape = vec![rect(0.0, 0.0, 400.0, 400.0), rect(100.0, 100.0, 300.0, 300.0)];
        let r = Region::new(&shape, true, &[], &[]).unwrap();
        let path = r.route((50.0, 200.0), (350.0, 200.0)).unwrap();
        assert_eq!(*path.last().unwrap(), (350.0, 200.0));
        let mut prev = (50.0, 200.0);
        for &p in &path {
            assert!(dist(prev, p) <= STEP + 0.01);
            assert!(!(p.0 > 100.0 && p.0 < 300.0 && p.1 > 100.0 && p.1 < 300.0), "{p:?} is in the hole");
            prev = p;
        }
    }

    #[test]
    fn does_not_route_between_separate_parts() {
        let shape = vec![rect(0.0, 0.0, 100.0, 100.0), rect(200.0, 0.0, 300.0, 100.0)];
        let r = Region::new(&shape, false, &[], &[]).unwrap();
        assert!(r.route((50.0, 50.0), (250.0, 50.0)).is_none());
    }

    #[test]
    fn crosses_under_a_shape_on_top_when_that_is_the_only_way() {
        // The cut-out splits the square in two; the shape on top sews later
        // and covers the run.
        let shape = vec![rect(0.0, 0.0, 400.0, 400.0)];
        let mask: Vec<MaskShape> = vec![(vec![rect(150.0, -10.0, 250.0, 410.0)], false)];
        let r = Region::new(&shape, false, &mask, &mask).unwrap();
        assert!(r.route((50.0, 100.0), (350.0, 100.0)).is_some());
        // A sparse shape on top wouldn't hide the run.
        let r = Region::new(&shape, false, &mask, &[]).unwrap();
        assert!(r.route((50.0, 100.0), (350.0, 100.0)).is_none());
    }

    #[test]
    fn prefers_ground_not_sewn_yet() {
        // Sewn rows across the top; a move along their lower edge should
        // dip below them rather than run over them.
        let shape = vec![rect(0.0, 0.0, 400.0, 400.0)];
        let mut r = Region::new(&shape, false, &[], &[]).unwrap();
        r.strict = false;
        for y in (10..120).step_by(4) {
            r.mark((0.0, y as f32), (400.0, y as f32));
        }
        let path = r.route((20.0, 100.0), (380.0, 100.0)).unwrap();
        let over = path.iter().filter(|p| p.1 < 115.0).count();
        assert!(over * 3 < path.len(), "{over} of {} points over sewn rows", path.len());
    }

    #[test]
    fn never_runs_over_finished_rows_on_top() {
        // Sewn rows split the square; on top, the move must jump rather than
        // leave a visible run across them.
        let shape = vec![rect(0.0, 0.0, 400.0, 400.0)];
        let mut r = Region::new(&shape, false, &[], &[]).unwrap();
        for y in (0..400).step_by(4) {
            r.mark((180.0, y as f32), (220.0, y as f32));
        }
        assert!(r.route((50.0, 200.0), (350.0, 200.0)).is_none());
        r.strict = false;
        assert!(r.route((50.0, 200.0), (350.0, 200.0)).is_some());
    }
}
