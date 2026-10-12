//! Multi-hooping: a design bigger than the hoop is split into parts that
//! each fit, sewn one after the other with the fabric re-hooped between.
//!
//! The design is divided into a grid of cells. Each run of stitches (from
//! one jump or thread change to the next) goes whole into the part whose
//! cell holds its middle, as long as it fits that part's hoop; only runs
//! too big for any hoop are cut at the cell edges, with tie stitches.
//! Every part starts with small registration crosses on the edges it
//! shares with its neighbors, sewn in their own thread, so the next
//! hooping can be lined up with the last.

use crate::formats::{self, Cmd, Pattern, Thread};
use crate::lettering::Pt;

/// A cell: min x, min y, max x, max y.
pub type Rect = (f32, f32, f32, f32);

pub struct Part {
    /// The part of the design this hooping covers.
    pub cell: Rect,
    /// Where the hoop's center goes on the design.
    pub center: Pt,
    /// The stitches, in design coordinates.
    pub pattern: Pattern,
}

/// Grid of cells over `bounds` so that each cell, plus some room for runs
/// that stick out of it, fits a `hoop` (width, height).
pub fn plan(bounds: Rect, hoop: (f32, f32)) -> Vec<Rect> {
    let (w, h) = ((bounds.2 - bounds.0).max(1.0), (bounds.3 - bounds.1).max(1.0));
    // A cell may use 80 % of the hoop, leaving room for runs to reach over.
    let cols = (w / (hoop.0 * 0.8)).ceil().max(1.0) as usize;
    let rows = (h / (hoop.1 * 0.8)).ceil().max(1.0) as usize;
    let (cw, ch) = (w / cols as f32, h / rows as f32);
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let x0 = bounds.0 + c as f32 * cw;
            let y0 = bounds.1 + r as f32 * ch;
            out.push((x0, y0, x0 + cw, y0 + ch));
        }
    }
    out
}

fn center(r: Rect) -> Pt {
    ((r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0)
}

fn inside(r: Rect, p: Pt) -> bool {
    p.0 >= r.0 && p.0 <= r.2 && p.1 >= r.1 && p.1 <= r.3
}

/// One run of stitches in one thread: its points (the first is where the
/// needle starts) and thread number.
struct Run {
    pts: Vec<Pt>,
    thread: usize,
}

fn runs(points: &[Pt], cmds: &[Cmd]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut thread = 0;
    let mut open = false;
    for i in 0..cmds.len().min(points.len()) {
        match cmds[i] {
            Cmd::ColorChange => {
                thread += 1;
                open = false;
            }
            Cmd::Jump | Cmd::Trim => open = false,
            Cmd::Stitch => {
                if !open {
                    let from = if i > 0 { points[i - 1] } else { points[i] };
                    out.push(Run { pts: vec![from], thread });
                    open = true;
                }
                out.last_mut().unwrap().pts.push(points[i]);
            }
        }
    }
    out
}

/// A small cross of running stitches, 6 mm across.
fn cross(at: Pt) -> Vec<Pt> {
    let a = 30.0;
    let mut v = vec![at];
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        for k in 1..=2 {
            v.push((at.0 + dx * a * k as f32 / 2.0, at.1 + dy * a * k as f32 / 2.0));
        }
        v.push(at);
    }
    v
}

/// Registration marks on a cell's edges that it shares with others.
fn marks(cell: Rect, all: &[Rect]) -> Vec<Pt> {
    let mut out = Vec::new();
    let near = |a: f32, b: f32| (a - b).abs() < 1.0;
    for o in all {
        if *o == cell {
            continue;
        }
        let overlap_y = (cell.1.max(o.1), cell.3.min(o.3));
        let overlap_x = (cell.0.max(o.0), cell.2.min(o.2));
        let x_edge = if near(cell.2, o.0) { Some(cell.2) } else if near(cell.0, o.2) { Some(cell.0) } else { None };
        let y_edge = if near(cell.3, o.1) { Some(cell.3) } else if near(cell.1, o.3) { Some(cell.1) } else { None };
        if let Some(x) = x_edge.filter(|_| overlap_y.1 - overlap_y.0 > 1.0) {
            for t in [0.25, 0.75] {
                out.push((x, overlap_y.0 + (overlap_y.1 - overlap_y.0) * t));
            }
        }
        if let Some(y) = y_edge.filter(|_| overlap_x.1 - overlap_x.0 > 1.0) {
            for t in [0.25, 0.75] {
                out.push((overlap_x.0 + (overlap_x.1 - overlap_x.0) * t, y));
            }
        }
    }
    out
}

/// Splits stitches (in design coordinates) into hoopings for `hoop`
/// (width, height, 0.1 mm). Parts with no stitches are left out.
pub fn split(points: &[Pt], cmds: &[Cmd], threads: &[Thread], hoop: (f32, f32)) -> Vec<Part> {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (&p, _) in points.iter().zip(cmds).filter(|(_, c)| **c == Cmd::Stitch) {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let cells = plan(b, hoop);
    let all = runs(points, cmds);
    // Pieces per cell: (original run index, points, thread).
    let mut pieces: Vec<Vec<(usize, Vec<Pt>, usize)>> = vec![Vec::new(); cells.len()];
    let fits = |pts: &[Pt], cell: Rect| {
        let c = center(cell);
        pts.iter().all(|p| (p.0 - c.0).abs() <= hoop.0 / 2.0 - 20.0 && (p.1 - c.1).abs() <= hoop.1 / 2.0 - 20.0)
    };
    for (k, run) in all.iter().enumerate() {
        let mut rb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &run.pts {
            rb = (rb.0.min(p.0), rb.1.min(p.1), rb.2.max(p.0), rb.3.max(p.1));
        }
        let mid = center(rb);
        let home = cells.iter().position(|c| inside(*c, mid)).unwrap_or_else(|| {
            (0..cells.len()).min_by(|&a, &b| {
                let (ca, cb) = (center(cells[a]), center(cells[b]));
                (ca.0 - mid.0).hypot(ca.1 - mid.1).total_cmp(&(cb.0 - mid.0).hypot(cb.1 - mid.1))
            }).unwrap()
        });
        if fits(&run.pts, cells[home]) {
            pieces[home].push((k, run.pts.clone(), run.thread));
            continue;
        }
        // Too big for one hooping: cut it at the cell edges.
        for (ci, cell) in cells.iter().enumerate() {
            let mut cur: Vec<Pt> = Vec::new();
            for w in run.pts.windows(2) {
                if inside(*cell, w[0]) && inside(*cell, w[1]) {
                    if cur.is_empty() {
                        cur.push(w[0]);
                    }
                    cur.push(w[1]);
                } else if cur.len() > 1 {
                    pieces[ci].push((k, std::mem::take(&mut cur), run.thread));
                } else {
                    cur.clear();
                }
            }
            if cur.len() > 1 {
                pieces[ci].push((k, cur, run.thread));
            }
        }
    }
    let reg = Thread { color: [0x8a, 0x8a, 0x8a], name: t!("Registration marks (remove after)").into(), code: String::new() };
    let mut parts = Vec::new();
    for (ci, cell) in cells.iter().enumerate() {
        if pieces[ci].is_empty() {
            continue;
        }
        let mut st = crate::lettering::Stitcher::new();
        let mut ths: Vec<Thread> = Vec::new();
        let marks = marks(*cell, &cells);
        if !marks.is_empty() {
            ths.push(reg.clone());
            for m in &marks {
                let c = cross(*m);
                st.travel(c[0], c[1]);
                for p in &c[1..] {
                    st.stitch(*p);
                }
            }
        }
        let mut current: Option<usize> = None;
        for (_, pts, thread) in &pieces[ci] {
            if current != Some(*thread) {
                if !ths.is_empty() {
                    st.color_change();
                }
                ths.push(threads.get(*thread).cloned().unwrap_or_else(|| formats::stock_thread(*thread)));
                current = Some(*thread);
            }
            st.travel(pts[0], pts.get(1).copied().unwrap_or(pts[0]));
            for p in &pts[1..] {
                st.stitch(*p);
            }
        }
        st.finish();
        parts.push(Part { cell: *cell, center: center(*cell), pattern: Pattern { format: formats::Format::Jef, points: st.points, cmds: st.cmds, threads: ths } });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows of 2 mm stitches over a w × h area, in one thread.
    fn field(w: f32, h: f32) -> (Vec<Pt>, Vec<Cmd>) {
        let mut pts = Vec::new();
        let mut cmds = Vec::new();
        let mut y = 0.0;
        while y <= h {
            // Each row its own run, so it can go to one part.
            let mut x = 0.0;
            cmds.push(Cmd::Jump);
            pts.push((x, y));
            while x < w {
                x = (x + 20.0).min(w);
                pts.push((x, y));
                cmds.push(Cmd::Stitch);
            }
            y += 100.0;
        }
        (pts, cmds)
    }

    #[test]
    fn a_wide_design_is_split_into_hoopings_that_fit() {
        // 300 × 100 mm into a 140 × 140 mm hoop: three parts side by side.
        let (pts, cmds) = field(3000.0, 1000.0);
        let threads = vec![formats::jef_thread(1, 0)];
        let parts = split(&pts, &cmds, &threads, (1400.0, 1400.0));
        assert_eq!(parts.len(), 3);
        for p in &parts {
            let b = p.pattern.bounds();
            assert!(b.2 - p.center.0 <= 700.0 && p.center.0 - b.0 <= 700.0, "{b:?} {:?}", p.center);
            // Registration marks come first, in their own thread.
            assert!(p.pattern.threads[0].name.contains("Registration"));
        }
        // Rows are 300 mm long, so they are cut; together the parts still
        // cover the whole length.
        let total: f32 = parts.iter().map(|p| p.pattern.bounds().2 - p.pattern.bounds().0).sum();
        assert!(total >= 2990.0, "{total}");
    }

    #[test]
    fn runs_that_fit_stay_whole() {
        // Two 50 mm squares far apart: each goes whole into one part.
        let mut pts = Vec::new();
        let mut cmds = Vec::new();
        for x0 in [0.0f32, 2200.0] {
            cmds.push(Cmd::Jump);
            pts.push((x0, 0.0));
            for k in 1..=40 {
                pts.push((x0 + (k % 2) as f32 * 500.0, k as f32 * 12.0));
                cmds.push(Cmd::Stitch);
            }
        }
        let parts = split(&pts, &cmds, &[formats::jef_thread(1, 0)], (1400.0, 1400.0));
        assert_eq!(parts.len(), 2);
        for p in &parts {
            let n = p.pattern.cmds.iter().filter(|c| **c == Cmd::Stitch).count();
            assert!(n >= 40, "{n}");
        }
    }
}
