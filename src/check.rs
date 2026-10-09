//! Checks that a design will sew well: thread density, needle hits piling
//! up in one spot, stitch lengths, and shapes too thin or too wide for the
//! way they are stitched. Works on the stitches themselves, so it covers
//! opened files too, plus the artwork and lettering settings when known.

use crate::artwork::{self, ArtSettings, Artwork, Kind, Look};
use crate::formats::{Cmd, Pattern};
use crate::lettering::{self, EdgeKind, FillKind, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Problem,
    Warning,
    Note,
}

/// A one-click way out of an issue. The app applies it to the current
/// lettering or artwork settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fix {
    /// Set the fill row spacing, in mm.
    Rows(f32),
    /// Turn on cutting away parts hidden under other shapes.
    Knockout,
    /// Stitch one artwork shape another way.
    ShapeLook(usize, Look),
    /// Lettering height, in mm.
    Height(f32),
    /// Lettering fill.
    LetterFill(FillKind),
    /// Width of satin edges, in mm.
    EdgeWidth(f32),
}

impl Fix {
    pub fn label(&self) -> String {
        match self {
            Fix::Rows(mm) => format!("Rows {mm:.2} mm"),
            Fix::Knockout => "Cut hidden parts".into(),
            Fix::ShapeLook(_, Look::Outline) => "Stitch as outline".into(),
            Fix::ShapeLook(_, Look::Fill(k)) => format!("Use {k:?}"),
            Fix::ShapeLook(_, _) => "Change".into(),
            Fix::Height(mm) => format!("Make {mm:.0} mm tall"),
            Fix::LetterFill(k) => format!("Use {k:?}"),
            Fix::EdgeWidth(mm) => format!("Edge {mm:.1} mm"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub level: Level,
    pub title: String,
    pub detail: String,
    /// Where to look, in design coordinates (0.1 mm).
    pub at: Option<(f32, f32)>,
    /// The artwork shape it is about.
    pub shape: Option<usize>,
    pub fix: Option<Fix>,
}

/// Thread laid per area, in mm of thread per mm². One layer of tatami at
/// 0.45 mm rows is about 2.2; satin at 0.4 mm about 2.5.
pub const DENSE: f32 = 6.5;
pub const TOO_DENSE: f32 = 9.0;
/// Needle hits per mm² before the fabric gets perforated.
const HITS: f32 = 25.0;

pub struct Report {
    pub issues: Vec<Issue>,
    /// Grid of 1 mm cells over the design: top-left corner and size.
    pub origin: (f32, f32),
    pub w: usize,
    pub h: usize,
    /// Thread per cell, smoothed over neighbours, in mm per mm².
    pub coverage: Vec<f32>,
    pub stitches: usize,
}

impl Report {
    pub fn count(&self, level: Level) -> usize {
        self.issues.iter().filter(|i| i.level == level).count()
    }
}

const CELL: f32 = 10.0;

/// Connected groups of cells over a limit, largest first, as (cells, middle).
fn clusters(grid: &[f32], w: usize, h: usize, limit: f32, origin: (f32, f32)) -> Vec<(usize, (f32, f32))> {
    let mut seen = vec![false; grid.len()];
    let mut out = Vec::new();
    for start in 0..grid.len() {
        if seen[start] || grid[start] <= limit {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut n, mut sx, mut sy) = (0usize, 0.0f32, 0.0f32);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            n += 1;
            sx += x as f32;
            sy += y as f32;
            let mut push = |j: usize| {
                if !seen[j] && grid[j] > limit {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 { push(i - 1); }
            if x + 1 < w { push(i + 1); }
            if y > 0 { push(i - w); }
            if y + 1 < h { push(i + w); }
        }
        let mid = (origin.0 + (sx / n as f32 + 0.5) * CELL, origin.1 + (sy / n as f32 + 0.5) * CELL);
        out.push((n, mid));
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.0));
    out
}

fn blur(grid: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0; grid.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = (0.0, 0.0);
            for (dx, dy) in [(0i32, 0i32), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                    // The cell itself counts double.
                    let k = if dx == 0 && dy == 0 { 2.0 } else { 1.0 };
                    sum += grid[ny as usize * w + nx as usize] * k;
                    n += k;
                }
            }
            out[y * w + x] = sum / n;
        }
    }
    out
}

/// Checks the stitches of a design.
pub fn stitches(p: &Pattern) -> Report {
    let b = p.bounds();
    let origin = (b.0 - CELL, b.1 - CELL);
    let w = (((b.2 - b.0) / CELL).ceil() as usize + 2).clamp(1, 2000);
    let h = (((b.3 - b.1) / CELL).ceil() as usize + 2).clamp(1, 2000);
    let mut thread = vec![0.0f32; w * h];
    let mut hits = vec![0.0f32; w * h];
    let cell_of = |q: (f32, f32)| -> Option<usize> {
        let (x, y) = (((q.0 - origin.0) / CELL).floor(), ((q.1 - origin.1) / CELL).floor());
        (x >= 0.0 && y >= 0.0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize)
    };
    let (mut stitches, mut short, mut long) = (0usize, Vec::new(), Vec::new());
    for i in 1..p.cmds.len() {
        if p.cmds[i] != Cmd::Stitch || matches!(p.cmds[i - 1], Cmd::ColorChange) {
            continue;
        }
        stitches += 1;
        let (a, q) = (p.points[i - 1], p.points[i]);
        let len = lettering::dist(a, q);
        if let Some(c) = cell_of(q) {
            hits[c] += 1.0;
        }
        if len > 0.0 && len < lettering::MIN_STITCH {
            short.push(q);
        } else if len > 80.0 {
            long.push(q);
        }
        // Spread the thread over the cells it crosses, a quarter mm at a time.
        let steps = (len / 2.5).ceil().max(1.0) as usize;
        for k in 0..steps {
            let t = (k as f32 + 0.5) / steps as f32;
            if let Some(c) = cell_of((a.0 + (q.0 - a.0) * t, a.1 + (q.1 - a.1) * t)) {
                thread[c] += len / 10.0 / steps as f32;
            }
        }
    }
    let coverage = blur(&thread, w, h);
    let mut issues = Vec::new();

    let dense = clusters(&coverage, w, h, DENSE, origin);
    // Single hot cells happen at every satin corner and tie-off; only
    // patches of a few mm² matter.
    let too: Vec<(usize, (f32, f32))> = clusters(&coverage, w, h, TOO_DENSE, origin).into_iter().filter(|c| c.0 >= 3).collect();
    if let Some(&(n, at)) = too.first() {
        issues.push(Issue {
            level: Level::Problem,
            title: "Far too much thread in one place".into(),
            detail: format!(
                "{} spot{} ({} mm² in all) carry about four layers of stitching or more. The needle can break and the fabric pucker. Turn on the density view (H) to see them.",
                too.len(),
                if too.len() == 1 { "" } else { "s" },
                too.iter().map(|c| c.0).sum::<usize>()
            ),
            at: Some(at),
            shape: None,
            fix: None,
        });
        let _ = n;
    } else if let Some(&(_, at)) = dense.iter().find(|c| c.0 >= 4) {
        let area: usize = dense.iter().map(|c| c.0).sum();
        issues.push(Issue {
            level: Level::Warning,
            title: "Dense areas".into(),
            detail: format!("About {area} mm² carry two layers of stitching or more, often fills under fills. It sews, but stiffly."),
            at: Some(at),
            shape: None,
            fix: None,
        });
    }
    let hit_blur = blur(&hits, w, h);
    let piles: Vec<(usize, (f32, f32))> = clusters(&hit_blur, w, h, HITS, origin).into_iter().filter(|c| c.0 >= 2).collect();
    if let Some(&(_, at)) = piles.first() {
        issues.push(Issue {
            level: Level::Warning,
            title: "Needle hits pile up".into(),
            detail: format!(
                "In {} spot{} the needle goes in over {HITS:.0} times per mm², usually tight curves or points in satin. It can cut the fabric.",
                piles.len(),
                if piles.len() == 1 { "" } else { "s" }
            ),
            at: Some(at),
            shape: None,
            fix: None,
        });
    }
    if short.len() * 50 > stitches.max(1) {
        issues.push(Issue {
            level: Level::Warning,
            title: format!("{} very short stitches", short.len()),
            detail: "Stitches under 0.3 mm land in their own holes and can break the thread.".into(),
            at: short.get(short.len() / 2).copied(),
            shape: None,
            fix: None,
        });
    }
    if !long.is_empty() {
        issues.push(Issue {
            level: Level::Warning,
            title: format!("{} stitches longer than 8 mm", long.len()),
            detail: "Long loose stitches snag on wear and washing. Use tatami there or a narrower satin.".into(),
            at: long.first().copied(),
            shape: None,
            fix: None,
        });
    }
    let jumps = p.count(Cmd::Jump);
    if jumps > 60 {
        issues.push(Issue {
            level: Level::Note,
            title: format!("{jumps} jumps"),
            detail: "Every jump is a thread to trim. Fewer, larger shapes sew faster.".into(),
            at: None,
            shape: None,
            fix: None,
        });
    }
    Report { issues, origin, w, h, coverage, stitches }
}

/// Checks artwork shapes against how they are stitched.
pub fn artwork(report: &mut Report, art: &Artwork, s: &ArtSettings) {
    let (scale, mid) = artwork::frame(art, s);
    let to_mm = |p: (f32, f32)| ((p.0 - mid.0) * scale, (p.1 - mid.1) * scale);
    let mut thin = 0;
    for (i, e) in s.shapes.iter().enumerate() {
        if e.hidden || !s.layers.get(e.layer).is_none_or(|l| l.enabled) {
            continue;
        }
        let look = e.look.unwrap_or_else(|| s.layers.get(e.layer).map_or(Look::Default, |l| l.look));
        let lines: Vec<lettering::Contour> = e.lines(art).iter().map(|(c, _)| c.iter().map(|&p| to_mm(p)).collect()).collect();
        let b = artwork::extent(&e.lines(art).iter().map(|(c, k)| (c.iter().map(|&p| to_mm(p)).collect(), *k)).collect::<Vec<_>>());
        let at = Some(((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0));
        match e.base(art).kind {
            Kind::Fill { even_odd } => {
                let fill = match look {
                    Look::Default => s.style.fill,
                    Look::Fill(k) => k,
                    Look::Outline | Look::Applique => FillKind::None,
                };
                if fill == FillKind::None {
                    continue;
                }
                let area = lettering::shape_area(&lines, even_odd);
                if area < (s.min_detail_mm * 10.0).powi(2) {
                    continue;
                }
                let t = lettering::thickness(&lines, even_odd) / 10.0;
                if t < 0.9 {
                    thin += 1;
                    if thin <= 5 {
                        report.issues.push(Issue {
                            level: Level::Problem,
                            title: format!("{} is too thin to fill", e.name),
                            detail: format!("About {t:.1} mm wide. Fill rows that short won't show; stitch it as an outline or make it bigger."),
                            at,
                            shape: Some(i),
                            fix: Some(Fix::ShapeLook(i, Look::Outline)),
                        });
                    }
                } else if t < 2.5 && matches!(fill, FillKind::Tatami | FillKind::Wave | FillKind::Gradient | FillKind::Contour | FillKind::Crosshatch) {
                    report.issues.push(Issue {
                        level: Level::Warning,
                        title: format!("{} is narrow for {fill:?}", e.name),
                        detail: format!("About {t:.1} mm wide. Satin gives narrow parts a clean, shiny column."),
                        at,
                        shape: Some(i),
                        fix: Some(Fix::ShapeLook(i, Look::Fill(FillKind::Satin))),
                    });
                } else if fill == FillKind::Satin && t > 8.0 {
                    report.issues.push(Issue {
                        level: Level::Warning,
                        title: format!("{} is wide for satin", e.name),
                        detail: format!("About {t:.0} mm across. Satin over 7 mm is split into long, loose stitches; tatami holds better."),
                        at,
                        shape: Some(i),
                        fix: Some(Fix::ShapeLook(i, Look::Fill(FillKind::Tatami))),
                    });
                }
            }
            Kind::Stroke { width } => {
                let w = width * scale / 10.0;
                if s.stroke == EdgeKind::Satin && w > 6.0 {
                    report.issues.push(Issue {
                        level: Level::Warning,
                        title: format!("{} is a wide satin line", e.name),
                        detail: format!("{w:.1} mm wide; satin lines are capped at 7 mm, and wide satin snags."),
                        at,
                        shape: Some(i),
                        fix: None,
                    });
                }
            }
        }
    }
    if thin > 5 {
        report.issues.push(Issue {
            level: Level::Problem,
            title: format!("{} more shapes too thin to fill", thin - 5),
            detail: "Raise Detail in Style → Clean-up to leave out slivers, or stitch them as outlines.".into(),
            at: None,
            shape: None,
            fix: None,
        });
    }
    let has_dense = report.issues.iter().any(|i| i.title.contains("thread in one place") || i.title == "Dense areas");
    if has_dense && !s.knockout {
        for i in &mut report.issues {
            if i.fix.is_none() && (i.title.contains("thread in one place") || i.title == "Dense areas") {
                i.fix = Some(Fix::Knockout);
                i.detail.push_str(" Cutting away hidden parts helps.");
            }
        }
    }
    sort(report);
}

/// Checks lettering settings.
pub fn lettering(report: &mut Report, s: &Settings) {
    if s.height_mm < 5.0 {
        report.issues.push(Issue {
            level: Level::Problem,
            title: "Letters under 5 mm".into(),
            detail: "Small letters lose their shape in thread. 6 mm is about the smallest that reads well.".into(),
            at: None,
            shape: None,
            fix: Some(Fix::Height(6.0)),
        });
    } else if s.height_mm < 10.0 && matches!(s.fill, FillKind::Tatami | FillKind::Wave | FillKind::Gradient | FillKind::Crosshatch | FillKind::Contour) {
        report.issues.push(Issue {
            level: Level::Warning,
            title: "Small letters with a row fill".into(),
            detail: "Below 10 mm the letter strokes are narrow; satin keeps them crisp.".into(),
            at: None,
            shape: None,
            fix: Some(Fix::LetterFill(FillKind::Satin)),
        });
    }
    if s.edge == EdgeKind::Satin && s.edge_width_mm > 5.0 {
        report.issues.push(Issue {
            level: Level::Warning,
            title: "Wide satin edge".into(),
            detail: "Satin edges over 5 mm get loose and snag.".into(),
            at: None,
            shape: None,
            fix: Some(Fix::EdgeWidth(3.0)),
        });
    }
    let fill_rows = matches!(s.fill, FillKind::Tatami | FillKind::Satin | FillKind::Wave | FillKind::Contour);
    if fill_rows && s.density_mm < 0.3 {
        report.issues.push(Issue {
            level: Level::Warning,
            title: "Fill rows very close".into(),
            detail: "Rows under 0.3 mm pile thread on thread. 0.4–0.45 mm covers well.".into(),
            at: None,
            shape: None,
            fix: Some(Fix::Rows(0.4)),
        });
    }
    sort(report);
}

fn sort(report: &mut Report) {
    report.issues.sort_by_key(|i| i.level);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::Format;

    fn pattern(points: Vec<(f32, f32)>) -> Pattern {
        let mut cmds = vec![Cmd::Stitch; points.len()];
        cmds[0] = Cmd::Jump;
        Pattern { format: Format::Svg, points, cmds, threads: Vec::new() }
    }

    /// Rows `gap` apart across a 20 × 20 mm square, `layers` times over.
    fn rows(gap: f32, layers: usize) -> Pattern {
        let mut pts = vec![(0.0, 0.0)];
        for _ in 0..layers {
            let mut y = 0.0;
            let mut right = true;
            while y <= 200.0 {
                for x in (0..=200).step_by(35) {
                    let x = if right { x as f32 } else { 200.0 - x as f32 };
                    pts.push((x, y));
                }
                right = !right;
                y += gap;
            }
        }
        pattern(pts)
    }

    #[test]
    fn one_layer_of_fill_is_fine_and_three_are_not() {
        let one = stitches(&rows(4.5, 1));
        assert!(one.issues.iter().all(|i| i.level != Level::Problem), "{:?}", one.issues);
        let mid = one.coverage[one.w * (one.h / 2) + one.w / 2];
        assert!((1.8..3.0).contains(&mid), "one layer of 0.45 mm rows: {mid}");
        let three = stitches(&rows(4.5, 4));
        assert_eq!(three.issues[0].level, Level::Problem);
    }

    #[test]
    fn short_and_long_stitches_are_found() {
        let mut pts = vec![(0.0, 0.0)];
        for k in 0..40 {
            pts.push((k as f32 * 1.0, 0.0));
        }
        pts.push((200.0, 0.0));
        let r = stitches(&pattern(pts));
        assert!(r.issues.iter().any(|i| i.title.contains("very short")));
        assert!(r.issues.iter().any(|i| i.title.contains("longer than 8 mm")));
    }

    #[test]
    fn thin_artwork_shapes_get_an_outline_fix() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <rect x="10" y="10" width="80" height="40" fill="#ff0000"/>
            <rect x="10" y="70" width="80" height="0.5" fill="#0000ff"/>
            <rect x="10" y="80" width="80" height="2" fill="#00ff00"/>
        </svg>"##;
        let art = artwork::parse("t", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.width_mm = 100.0;
        s.min_detail_mm = 0.0;
        let p = artwork::generate(&art, &s).unwrap();
        let mut r = stitches(&p);
        artwork(&mut r, &art, &s);
        let thin = r.issues.iter().find(|i| i.title.contains("too thin")).expect("thin sliver found");
        assert_eq!(thin.fix, Some(Fix::ShapeLook(1, Look::Outline)));
        let narrow = r.issues.iter().find(|i| i.title.contains("narrow")).expect("2 mm strip found");
        assert_eq!(narrow.fix, Some(Fix::ShapeLook(2, Look::Fill(FillKind::Satin))));
        assert!(!r.issues.iter().any(|i| i.shape == Some(0)), "the big square is fine");
        // Applying the fixes clears them.
        s.shapes[1].look = Some(Look::Outline);
        s.shapes[2].look = Some(Look::Fill(FillKind::Satin));
        let mut r = stitches(&artwork::generate(&art, &s).unwrap());
        artwork(&mut r, &art, &s);
        assert!(!r.issues.iter().any(|i| i.shape.is_some()), "{:?}", r.issues);
    }

    /// Lengths of sewn stitches, leaving out the first after a color change.
    fn lengths(p: &Pattern) -> Vec<f32> {
        (1..p.cmds.len())
            .filter(|&i| p.cmds[i] == Cmd::Stitch && p.cmds[i - 1] != Cmd::ColorChange)
            .map(|i| lettering::dist(p.points[i - 1], p.points[i]))
            .collect()
    }

    #[test]
    fn hoop_makes_no_tiny_or_overlong_stitches() {
        let fonts = lettering::bundled_fonts();
        for fill in [FillKind::Tatami, FillKind::Satin, FillKind::Contour, FillKind::Wave] {
            let mut s = Settings { text: "Bremen".into(), height_mm: 30.0, ..Default::default() };
            lettering::style(&mut s, fill, 45.0, EdgeKind::Satin);
            let p = lettering::generate(&fonts[0], &s).unwrap();
            let l = lengths(&p);
            assert!(l.iter().all(|&d| d == 0.0 || d >= lettering::MIN_STITCH - 1e-3), "{fill:?}: short stitch");
            assert!(l.iter().all(|&d| d <= 80.0), "{fill:?}: longest {}", l.iter().cloned().fold(0.0, f32::max));
            let r = stitches(&p);
            assert!(r.issues.iter().all(|i| i.level != Level::Problem), "{fill:?}: {:?}", r.issues);
        }
    }

    #[test]
    fn wide_fills_get_a_sparse_underlay() {
        let square: Vec<lettering::Contour> = vec![vec![(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)]];
        let strip: Vec<lettering::Contour> = vec![vec![(0.0, 0.0), (300.0, 0.0), (300.0, 20.0), (0.0, 20.0)]];
        let count = |g: &[lettering::Contour]| {
            let mut st = lettering::Stitcher::new();
            lettering::underlay(&mut st, g, false, &[], FillKind::Tatami, 45.0, &Settings::default());
            st.points.len()
        };
        // The 30 mm square gets rows across; the 2 mm strip only its outline.
        // An outline alone is 48 stitches of 2.5 mm.
        assert!(count(&square) > 100);
        assert!((count(&strip) as f32) < 40.0);
        assert!((lettering::thickness(&strip, false) - 19.0).abs() < 2.0);
    }

    #[test]
    fn small_tatami_letters_are_flagged() {
        let mut r = stitches(&rows(4.5, 1));
        lettering(&mut r, &Settings { height_mm: 8.0, ..Default::default() });
        assert!(r.issues.iter().any(|i| i.fix == Some(Fix::LetterFill(FillKind::Satin))));
    }
}

#[cfg(test)]
mod samples {
    use super::*;

    /// Prints the report for real files: HOOP_FILES=a.jef:b.svg cargo test report_samples -- --ignored --nocapture
    #[test]
    #[ignore]
    fn report_samples() {
        let files = std::env::var("HOOP_FILES").unwrap_or_default();
        for f in files.split(':').filter(|f| !f.is_empty()) {
            let data = std::fs::read(f).unwrap();
            let r = if f.ends_with(".svg") {
                let art = artwork::parse(f, &data).unwrap();
                let s = ArtSettings::for_artwork(&art);
                let mut r = stitches(&artwork::generate(&art, &s).unwrap());
                artwork(&mut r, &art, &s);
                r
            } else {
                stitches(&crate::formats::read(f, &data).unwrap())
            };
            let peak = r.coverage.iter().cloned().fold(0.0f32, f32::max);
            println!("{f}: {} stitches, peak {peak:.1} mm/mm²", r.stitches);
            for i in &r.issues {
                println!("  {:?} {} — {}", i.level, i.title, i.detail);
            }
        }
    }
}
