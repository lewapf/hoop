//! Small test stitch-outs, so a design's edges and small parts can be tried on
//! scrap fabric before sewing the whole thing: a piece of a design cut to an
//! area of the hoop, and a calibration sheet of satin widths, narrow fills,
//! fill densities, pull compensation and small lettering.

use crate::formats::{self, Cmd, Format, Pattern, Thread};
use crate::lettering::{self, Contour, EdgeKind, FillKind, FontSource, Pt, Settings, Stitcher};

/// A rectangle as min x, min y, max x, max y in 0.1 mm.
pub type Area = (f32, f32, f32, f32);

fn inside(a: Area, p: Pt) -> bool {
    p.0 >= a.0 && p.0 <= a.2 && p.1 >= a.1 && p.1 <= a.3
}

/// Copies one thread's stitches into `st`, moved by `shift`. Gaps become
/// travels, so jumps get tie stitches.
fn paste(st: &mut Stitcher, points: &[Pt], cmds: &[Cmd], shift: Pt) {
    for i in 1..cmds.len().min(points.len()) {
        if cmds[i] != Cmd::Stitch {
            continue;
        }
        let (a, b) = (points[i - 1], points[i]);
        let (a, b) = ((a.0 + shift.0, a.1 + shift.1), (b.0 + shift.0, b.1 + shift.1));
        if st.last().is_none_or(|l| lettering::dist(l, a) > 0.5) {
            st.travel(a, b);
        }
        st.stitch(b);
    }
}

/// Length of tie stitches where the crop cuts a run, in 0.1 mm.
const TIE: f32 = 5.0;

/// Three tiny stitches back and forth from `at` toward `toward`, so a run
/// the crop cuts open can't pull out.
fn tie(points: &mut Vec<Pt>, cmds: &mut Vec<Cmd>, at: Pt, toward: Pt) {
    let d = lettering::dist(at, toward);
    let v = if d > 0.1 { ((toward.0 - at.0) / d * TIE, (toward.1 - at.1) / d * TIE) } else { (TIE, 0.0) };
    for p in [(at.0 + v.0, at.1 + v.1), at, (at.0 + v.0, at.1 + v.1), at] {
        points.push(p);
        cmds.push(Cmd::Stitch);
    }
}

/// Keeps only the stitches with both ends inside `area`. `threads` holds one
/// thread per color change segment; threads with nothing left are dropped.
/// Jumps of the design stay as they are; where the crop cuts a run, the
/// thread is tied off, jumps, and is tied in again. Returns `None` when no
/// stitch is inside.
pub fn crop(points: &[Pt], cmds: &[Cmd], threads: &[Thread], area: Area) -> Option<Pattern> {
    let mut out: Vec<Pt> = Vec::new();
    let mut out_cmds: Vec<Cmd> = Vec::new();
    let mut kept: Vec<Thread> = Vec::new();
    let mut seg = 0;
    let mut started = None;
    // A stitch was left out since the last one kept.
    let mut cut = false;
    for i in 1..cmds.len().min(points.len()) {
        match cmds[i] {
            Cmd::ColorChange => {
                seg += 1;
                cut = false;
            }
            Cmd::Jump | Cmd::Trim => {}
            Cmd::Stitch => {
                let (a, b) = (points[i - 1], points[i]);
                if !inside(area, a) || !inside(area, b) {
                    cut = true;
                    continue;
                }
                let fresh = started != Some(seg);
                if fresh {
                    if let Some(&l) = out.last() {
                        out.push(l);
                        out_cmds.push(Cmd::ColorChange);
                    }
                    kept.push(threads.get(seg).cloned().unwrap_or_else(|| formats::stock_thread(seg)));
                    started = Some(seg);
                }
                let at = out.last().copied();
                if at.is_none_or(|l| lettering::dist(l, a) > 0.5) {
                    if let (true, false, Some(l)) = (cut, fresh, at) {
                        let prev = out[out.len().saturating_sub(2)];
                        tie(&mut out, &mut out_cmds, l, prev);
                    }
                    out.push(a);
                    out_cmds.push(Cmd::Jump);
                    if cut {
                        tie(&mut out, &mut out_cmds, a, b);
                    }
                }
                out.push(b);
                out_cmds.push(Cmd::Stitch);
                cut = false;
            }
        }
    }
    if kept.is_empty() {
        return None;
    }
    Some(Pattern { format: Format::Jef, points: out, cmds: out_cmds, threads: kept })
}

/// The samples of each row of the test sheet, left to right, in mm.
pub const SATIN_WIDTHS: [f32; 5] = [1.5, 2.5, 3.5, 5.0, 7.0];
pub const FILL_WIDTHS: [f32; 5] = [1.0, 2.0, 3.0, 4.0, 6.0];
pub const ROW_SPACINGS: [f32; 4] = [0.35, 0.40, 0.45, 0.55];
pub const PULLS: [f32; 3] = [0.0, 0.2, 0.4];
pub const LETTER_HEIGHTS: [f32; 3] = [5.0, 7.0, 10.0];

/// One row of the sheet: its letter, what it holds, the question to answer
/// after sewing it, and the samples to answer with.
pub struct SheetRow {
    pub letter: &'static str,
    pub holds: &'static str,
    pub question: &'static str,
    pub samples: &'static [f32],
}

pub const SHEET_ROWS: [SheetRow; 5] = [
    SheetRow { letter: "A", holds: "Satin columns", question: "Widest satin that still lies flat and tight", samples: &SATIN_WIDTHS },
    SheetRow { letter: "B", holds: "Narrow fill strips", question: "Narrowest strip that filled cleanly", samples: &FILL_WIDTHS },
    SheetRow { letter: "C", holds: "Fill row spacing", question: "Most open rows that still hide the fabric", samples: &ROW_SPACINGS },
    SheetRow { letter: "D", holds: "Edge over fill, pull compensation", question: "Circle with no gap between fill and edge", samples: &PULLS },
    SheetRow { letter: "E", holds: "Small letters", question: "Smallest letters that read well", samples: &LETTER_HEIGHTS },
];

/// What the test sheet showed works with this machine, thread and fabric,
/// one answer per row of `SHEET_ROWS`, in mm. `None` means not sewn yet;
/// general rules apply instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tuning {
    pub satin_max: Option<f32>,
    pub fill_min: Option<f32>,
    pub rows: Option<f32>,
    pub pull: Option<f32>,
    pub letters_min: Option<f32>,
}

impl Tuning {
    /// The answers in the order of `SHEET_ROWS`.
    pub fn answers(&mut self) -> [&mut Option<f32>; 5] {
        [&mut self.satin_max, &mut self.fill_min, &mut self.rows, &mut self.pull, &mut self.letters_min]
    }

    pub fn is_empty(&self) -> bool {
        *self == Tuning::default()
    }

    /// Puts the row spacing and pull compensation the sheet chose into
    /// stitch settings.
    pub fn apply(&self, s: &mut Settings) {
        // The sheet tests row fills; satin and open styles keep their own spacing.
        if let Some(r) = self.rows {
            if matches!(s.fill, FillKind::Tatami | FillKind::Wave | FillKind::Gradient | FillKind::Contour) {
                s.density_mm = r;
            }
        }
        if let Some(p) = self.pull {
            s.pull_mm = p;
        }
        if let Some(m) = self.satin_max {
            s.column_max_mm = m;
        }
    }
}

/// Thread numbers of the sheet: fills, then edges and lettering.
const SHEET_FILL: i32 = 12;
const SHEET_EDGE: i32 = 1;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Contour {
    vec![(x - w / 2.0, y - h / 2.0), (x + w / 2.0, y - h / 2.0), (x + w / 2.0, y + h / 2.0), (x - w / 2.0, y + h / 2.0)]
}

fn circle(x: f32, y: f32, r: f32) -> Contour {
    (0..72).map(|i| (i as f32 / 72.0 * std::f32::consts::TAU).sin_cos()).map(|(s, c)| (x + r * c, y + r * s)).collect()
}

/// Stitches `text` from `font` centered on `at`.
fn text(st: &mut Stitcher, font: &FontSource, text: &str, height_mm: f32, at: Pt) -> Result<(), String> {
    let s = Settings {
        text: text.into(),
        height_mm,
        fill: FillKind::Satin,
        density_mm: FillKind::Satin.default_rows_mm(),
        edge: EdgeKind::None,
        fill_thread: SHEET_EDGE,
        ..Settings::default()
    };
    let p = lettering::generate(font, &s)?;
    let b = p.bounds();
    let shift = (at.0 - (b.0 + b.2) / 2.0, at.1 - (b.1 + b.3) / 2.0);
    paste(st, &p.points, &p.cmds, shift);
    Ok(())
}

/// The calibration sheet, about 124 × 110 mm so it fits the SQ14b hoop.
/// Two threads: fills first, then satin, edges and lettering.
pub fn sheet(font: &FontSource) -> Result<Pattern, String> {
    // Row centers, top to bottom, and the left edge of the samples.
    let rows = [-500.0, -240.0, 0.0, 230.0, 440.0];
    let col = |k: usize| -380.0 + 240.0 * k as f32;
    let base = Settings::default();
    let mut st = Stitcher::new();

    // Thread 1: fills of rows B, C and D.
    for (k, w) in FILL_WIDTHS.into_iter().enumerate() {
        let shape = [rect(col(k), rows[1], w * 10.0, 200.0)];
        lettering::underlay(&mut st, &shape, false, &[], FillKind::Tatami, base.fill_angle, &base);
        lettering::fill_shape(&mut st, &shape, false, &[], FillKind::Tatami, base.fill_angle, &base);
    }
    for (k, rows_mm) in ROW_SPACINGS.into_iter().enumerate() {
        let s = Settings { density_mm: rows_mm, ..base.clone() };
        let shape = [rect(col(k), rows[2], 150.0, 150.0)];
        lettering::underlay(&mut st, &shape, false, &[], FillKind::Tatami, s.fill_angle, &s);
        lettering::fill_shape(&mut st, &shape, false, &[], FillKind::Tatami, s.fill_angle, &s);
    }
    let discs: Vec<Contour> = (0..PULLS.len()).map(|k| circle(-350.0 + 300.0 * k as f32, rows[3], 100.0)).collect();
    for (disc, pull) in discs.iter().zip(PULLS) {
        let s = Settings { pull_mm: pull, ..base.clone() };
        let shape = std::slice::from_ref(disc);
        lettering::underlay(&mut st, shape, false, &[], FillKind::Tatami, s.fill_angle, &s);
        lettering::fill_shape(&mut st, shape, false, &[], FillKind::Tatami, s.fill_angle, &s);
    }

    // Thread 2: edges over the discs, satin columns, row letters, lettering.
    st.color_change();
    for disc in &discs {
        lettering::satin_contour(&mut st, disc, base.edge_width_mm * 10.0);
    }
    for (k, w) in SATIN_WIDTHS.into_iter().enumerate() {
        let x = col(k);
        lettering::satin_path(&mut st, &[(x, rows[0] - 100.0), (x, rows[0] + 100.0)], w * 10.0);
    }
    for (row, y) in SHEET_ROWS.iter().zip(rows) {
        text(&mut st, font, row.letter, 6.0, (-580.0, y))?;
    }
    for (k, h) in LETTER_HEIGHTS.into_iter().enumerate() {
        text(&mut st, font, "Abg", h, (-350.0 + 320.0 * k as f32, rows[4]))?;
    }
    st.finish();
    let threads = vec![formats::jef_thread(SHEET_FILL, 0), formats::jef_thread(SHEET_EDGE, 1)];
    Ok(Pattern { format: Format::Lettering, points: st.points, cmds: st.cmds, threads })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(from: Pt, to: Pt, n: usize) -> Vec<Pt> {
        (0..=n).map(|i| i as f32 / n as f32).map(|t| (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)).collect()
    }

    #[test]
    fn crop_keeps_only_stitches_inside_and_drops_empty_threads() {
        // Thread 1 runs left to right through the area; thread 2 stays outside.
        let mut points = line((-200.0, 0.0), (200.0, 0.0), 40);
        let mut cmds = vec![Cmd::Stitch; points.len()];
        cmds[0] = Cmd::Jump;
        points.push(*points.last().unwrap());
        cmds.push(Cmd::ColorChange);
        for p in line((300.0, 300.0), (400.0, 300.0), 10) {
            points.push(p);
            cmds.push(Cmd::Stitch);
        }
        let threads = vec![formats::jef_thread(2, 0), formats::jef_thread(9, 1)];
        let piece = crop(&points, &cmds, &threads, (-50.0, -50.0, 50.0, 50.0)).unwrap();
        assert_eq!(piece.threads, vec![threads[0].clone()]);
        assert_eq!(piece.count(Cmd::ColorChange), 0);
        for (p, c) in piece.points.iter().zip(&piece.cmds) {
            if *c == Cmd::Stitch {
                assert!(inside((-50.0, -50.0, 50.0, 50.0), *p), "{p:?} is outside");
            }
        }
        let b = piece.bounds();
        assert!(b.0 <= -40.0 && b.2 >= 40.0, "{b:?}");
    }

    #[test]
    fn crop_jumps_between_separate_parts() {
        let mut points = line((-200.0, 0.0), (200.0, 0.0), 40);
        let mut cmds = vec![Cmd::Stitch; points.len()];
        cmds[0] = Cmd::Jump;
        points.extend(line((200.0, 100.0), (-200.0, 100.0), 40));
        cmds.extend(vec![Cmd::Stitch; 41]);
        // The two runs are 10 mm apart, joined by a stitch outside the left area.
        let piece = crop(&points, &cmds, &[], (-210.0, -10.0, -100.0, 110.0)).unwrap();
        assert_eq!(piece.threads.len(), 1);
        assert!(piece.count(Cmd::Jump) >= 2, "{:?}", piece.cmds);
    }

    #[test]
    fn crop_around_everything_changes_nothing() {
        let p = sheet(&lettering::bundled_fonts()[0]).unwrap();
        let all = crop(&p.points, &p.cmds, &p.threads, (f32::MIN, f32::MIN, f32::MAX, f32::MAX)).unwrap();
        assert_eq!(all.count(Cmd::Stitch), p.count(Cmd::Stitch));
        assert_eq!(all.threads, p.threads);
    }

    #[test]
    fn crop_of_nothing_is_none() {
        let points = line((0.0, 0.0), (100.0, 0.0), 10);
        let cmds = vec![Cmd::Stitch; points.len()];
        assert!(crop(&points, &cmds, &[], (500.0, 500.0, 600.0, 600.0)).is_none());
    }

    #[test]
    fn sheet_fits_the_small_hoop_in_two_threads() {
        let font = &lettering::bundled_fonts()[0];
        let p = sheet(font).unwrap();
        assert_eq!(p.threads.len(), 2);
        assert_eq!(p.count(Cmd::ColorChange), 1);
        let b = p.bounds();
        let (w, h) = (b.2 - b.0, b.3 - b.1);
        assert!(w <= 1300.0 && h <= 1300.0, "{w} × {h}");
        assert!(w > 1000.0 && h > 900.0, "{w} × {h}");
    }

    /// Writes the sheet for a visual check: `HOOP_OUT=dir cargo test sheet_sample -- --ignored`.
    #[test]
    #[ignore]
    fn sheet_sample() {
        let out = std::path::PathBuf::from(std::env::var("HOOP_OUT").unwrap_or_else(|_| "/tmp".into()));
        let p = sheet(&lettering::bundled_fonts()[0]).unwrap();
        let threads = [SHEET_FILL, SHEET_EDGE];
        std::fs::write(out.join("test-sheet.jef"), formats::write_jef(&p.points, &p.cmds, &threads)).unwrap();
    }
}
