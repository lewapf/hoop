//! Turns text into stitches. Glyph outlines come from a font, get laid out in
//! 0.1 mm units (y down), optionally bent onto an arc, and are then filled
//! with tatami or satin rows and finished with a running or satin edge.

mod fonts;
mod layout;
mod patch;

use crate::formats::{self, Cmd, Format, Pattern};
use crate::stitch::route::Region;

pub use crate::stitch::*;
pub use fonts::*;
use layout::*;
use patch::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchShape {
    None,
    Rect,
    Rounded,
    Oval,
    Circle,
    /// A rounded tag with a tab and a satin eyelet for a key ring.
    Fob,
}

/// The line the text sits on, or the envelope it is bent into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextShape {
    Straight,
    /// Along an arc of `arc_deg`.
    Arc,
    /// Badge: the first line round the top of a circle, the second round
    /// the bottom (both upright), more lines straight in the middle.
    Circle,
    /// The top edge bows up.
    Arch,
    /// The bottom edge bows down.
    Valley,
    /// Both edges bow out.
    Bulge,
    /// Both edges bow in.
    Pinch,
    /// The whole text bows up, letters kept upright.
    Bridge,
    Wave,
    /// Letters grow from left to right.
    Grow,
    /// Three initials with the middle one bigger.
    Monogram,
}

impl TextShape {
    pub const ALL: [TextShape; 11] = [
        TextShape::Straight, TextShape::Arc, TextShape::Circle, TextShape::Arch, TextShape::Valley, TextShape::Bulge,
        TextShape::Pinch, TextShape::Bridge, TextShape::Wave, TextShape::Grow, TextShape::Monogram,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TextShape::Straight => n!("Straight"),
            TextShape::Arc => n!("Arc"),
            TextShape::Circle => n!("Circle badge"),
            TextShape::Arch => n!("Arch"),
            TextShape::Valley => n!("Valley"),
            TextShape::Bulge => n!("Bulge"),
            TextShape::Pinch => n!("Pinch"),
            TextShape::Bridge => n!("Bridge"),
            TextShape::Wave => n!("Wave"),
            TextShape::Grow => n!("Grow"),
            TextShape::Monogram => n!("Monogram"),
        }
    }

    /// Whether the shape takes the envelope amount.
    pub fn bends(self) -> bool {
        matches!(self, TextShape::Arch | TextShape::Valley | TextShape::Bulge | TextShape::Pinch | TextShape::Bridge | TextShape::Wave | TextShape::Grow)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    None,
    Running,
    Triple,
    Satin,
    /// E-stitch: a run with short stitches into the shape, for appliqué.
    Blanket,
    /// Half circles along the edge.
    Scallop,
}

/// Sews the edge style round one outline.
pub fn edge_contour(st: &mut Stitcher, c: &Contour, s: &Settings) {
    match s.edge {
        EdgeKind::None => {}
        EdgeKind::Running => run_contour(st, c, 25.0, false),
        EdgeKind::Triple => run_contour(st, c, 25.0, true),
        EdgeKind::Satin => satin_contour(st, c, s.edge_width_mm * 10.0),
        EdgeKind::Blanket => crate::stitch::special::blanket_contour(st, c, s.edge_width_mm * 10.0),
        EdgeKind::Scallop => crate::stitch::special::scallop_contour(st, c, s.edge_width_mm * 10.0, false),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub text: String,
    pub font: usize,
    pub height_mm: f32,
    pub spacing_mm: f32,
    pub line_spacing: f32,
    pub arc_deg: f32,
    pub shape: TextShape,
    /// Inner diameter of the circle badge.
    pub circle_mm: f32,
    /// How far envelopes bend the text.
    pub envelope_mm: f32,
    pub fill: FillKind,
    pub fill_angle: f32,
    pub density_mm: f32,
    pub stitch_mm: f32,
    pub wave_amp_mm: f32,
    pub wave_len_mm: f32,
    pub fade_mm: f32,
    /// Extra length added to both ends of every fill row, because stitches
    /// pull the fabric in and fills come out narrower than drawn.
    pub pull_mm: f32,
    /// Italic slant in degrees.
    pub slant_deg: f32,
    pub align: Align,
    /// A background shape behind the text, for badges and name patches.
    pub patch: PatchShape,
    pub patch_pad_mm: f32,
    pub patch_fill: FillKind,
    pub patch_border_mm: f32,
    pub patch_thread: i32,
    pub patch_border_thread: i32,
    /// Fabric appliqué instead of a stitched background.
    pub patch_applique: bool,
    /// Made in the hoop: a stop for backing fabric under the hoop and a
    /// tack-down before the border, so the piece is finished front and back.
    pub patch_ith: bool,
    pub underlay: bool,
    pub underlay_kind: Underlay,
    /// Widest part, in mm, that the Auto style still sews as a satin column.
    pub column_max_mm: f32,
    pub edge: EdgeKind,
    pub edge_width_mm: f32,
    /// Janome thread numbers (1-based index into the JEF chart).
    pub fill_thread: i32,
    pub edge_thread: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            text: t!("Hello").into(),
            font: 0,
            height_mm: 25.0,
            spacing_mm: 0.5,
            line_spacing: 1.3,
            arc_deg: 0.0,
            shape: TextShape::Straight,
            circle_mm: 60.0,
            envelope_mm: 8.0,
            fill: FillKind::Tatami,
            fill_angle: 45.0,
            density_mm: 0.4,
            stitch_mm: 3.5,
            wave_amp_mm: 2.0,
            wave_len_mm: 12.0,
            fade_mm: 2.5,
            pull_mm: 0.2,
            slant_deg: 0.0,
            align: Align::Center,
            patch: PatchShape::None,
            patch_pad_mm: 5.0,
            patch_fill: FillKind::Tatami,
            patch_border_mm: 3.0,
            patch_thread: 2,
            patch_border_thread: 30,
            patch_applique: false,
            patch_ith: false,
            underlay: true,
            underlay_kind: Underlay::Auto,
            column_max_mm: 7.0,
            edge: EdgeKind::Satin,
            edge_width_mm: 1.6,
            fill_thread: 42,
            edge_thread: 1,
        }
    }
}

/// The laid-out letter outlines, as `generate` stitches them.
#[cfg(test)]
pub(crate) fn outlines(font: &FontSource, s: &Settings) -> Vec<Vec<Contour>> {
    let (bytes, index) = font.bytes().unwrap();
    let face = ttf_parser::Face::parse(&bytes, index).unwrap();
    layout(&face, s)
}

pub fn generate(font: &FontSource, s: &Settings) -> Result<Pattern, String> {
    let (bytes, index) = font.bytes()?;
    let face = ttf_parser::Face::parse(&bytes, index).map_err(|_| format!("{} isn't a font Hoop can read.", font.name))?;
    let glyphs = layout(&face, s);
    if glyphs.is_empty() {
        return Err("Type some text to stitch.".into());
    }
    let mut st = Stitcher::new();
    let edge = |st: &mut Stitcher, g: &[Contour]| {
        for c in g {
            edge_contour(st, c, s);
        }
    };
    let fill = |st: &mut Stitcher, g: &[Contour]| {
        if s.fill == FillKind::None {
            return;
        }
        st.region = Region::new(g, false, &[], &[]);
        if s.underlay {
            underlay(st, g, false, &[], s.fill, s.fill_angle, s);
        }
        fill_glyph(st, g, s.fill, s.fill_angle, s);
        if s.fill == FillKind::Crosshatch {
            fill_glyph(st, g, s.fill, s.fill_angle + 90.0, s);
        }
        st.region = None;
    };
    // Thread changes happen only when the thread really changes, except for
    // appliqué stops, where the machine must halt even in the same thread.
    let mut threads: Vec<crate::formats::Thread> = Vec::new();
    let mut current: Option<i32> = None;
    let mut use_thread = |st: &mut Stitcher, t: i32, stop_as: Option<&str>| {
        if current != Some(t) || stop_as.is_some() {
            if !threads.is_empty() {
                st.color_change();
            }
            let mut thread = formats::jef_thread(t, threads.len());
            if let Some(name) = stop_as {
                thread.name = name.to_string();
                thread.code = String::new();
            }
            threads.push(thread);
            current = if stop_as.is_some() { None } else { Some(t) };
        }
    };

    let patch = patch_outline(&glyphs, s);
    if let Some(outline) = &patch {
        if s.patch_applique {
            // Placement line shows where the fabric goes; the machine stops,
            // the fabric is laid on, the tack-down holds it, and the machine
            // stops again so the extra can be trimmed before the cover stitch.
            use_thread(&mut st, s.patch_thread, Some(t!("Placement line")));
            run_contour(&mut st, outline, 25.0, false);
            use_thread(&mut st, s.patch_thread, Some(t!("Tack-down (lay the fabric first)")));
            run_contour(&mut st, outline, 20.0, false);
            run_contour(&mut st, outline, 20.0, false);
        } else if s.patch_fill != FillKind::None {
            use_thread(&mut st, s.patch_thread, None);
            let mask: Vec<MaskShape> = glyphs.iter().map(|g| (g.clone(), false)).collect();
            let cover = if covers(s.fill, s.density_mm) { &mask[..] } else { &[] };
            st.region = Region::new(std::slice::from_ref(outline), false, &mask, cover);
            run_contour(&mut st, outline, 25.0, false);
            let bg = Settings { density_mm: s.patch_fill.default_rows_mm(), underlay: false, ..s.clone() };
            fill_shape(&mut st, std::slice::from_ref(outline), false, &mask, s.patch_fill, s.fill_angle + 90.0, &bg);
            if s.patch_fill == FillKind::Crosshatch {
                // Lace: the second layer of the lattice.
                fill_shape(&mut st, std::slice::from_ref(outline), false, &mask, s.patch_fill, s.fill_angle, &bg);
            }
            st.region = None;
        }
    }

    let text_fill = s.fill != FillKind::None;
    let text_edge = s.edge != EdgeKind::None;
    if text_fill && text_edge && s.fill_thread != s.edge_thread {
        use_thread(&mut st, s.fill_thread, None);
        for g in &glyphs {
            fill(&mut st, g);
        }
        use_thread(&mut st, s.edge_thread, None);
        for g in &glyphs {
            edge(&mut st, g);
        }
    } else if text_fill || text_edge {
        use_thread(&mut st, if text_fill { s.fill_thread } else { s.edge_thread }, None);
        for g in &glyphs {
            fill(&mut st, g);
            edge(&mut st, g);
        }
    }

    // In the hoop: backing fabric goes under the hoop and is held by a
    // tack-down that the border then covers.
    if let (Some(outline), true) = (&patch, s.patch_ith) {
        use_thread(&mut st, s.patch_border_thread, Some(t!("Back (tape backing fabric under the hoop)")));
        run_contour(&mut st, outline, 20.0, false);
    }
    // The border goes on last so it covers the edge of the background fill.
    if let Some(outline) = &patch {
        // Appliqué needs a border wide enough to cover the trimmed fabric edge.
        let border = if s.patch_applique { s.patch_border_mm.max(2.5) } else { s.patch_border_mm };
        if border > 0.0 {
            use_thread(&mut st, s.patch_border_thread, None);
            satin_contour(&mut st, outline, border * 10.0);
        }
        if let Some(eye) = fob_eyelet(&glyphs, s) {
            // Satin round the key-ring hole; cut it open after sewing.
            satin_contour(&mut st, &eye, 12.0);
        }
    }
    st.finish();
    if !st.cmds.contains(&Cmd::Stitch) {
        return Err("Pick a fill or an edge so there is something to stitch.".into());
    }
    Ok(Pattern { format: Format::Lettering, points: st.points, cmds: st.cmds, threads })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_shapes_drawn_opposite_ways_both_cut() {
        // Two overlapping squares, one clockwise and one not: together their
        // winding cancels where they overlap, but both must still be cut.
        let a: Contour = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let b: Contour = vec![(50.0, 0.0), (50.0, 100.0), (150.0, 100.0), (150.0, 0.0)];
        assert_eq!(row_spans(&[a.clone(), b.clone()], 50.0, false), vec![(0.0, 50.0), (100.0, 150.0)]);
        assert_eq!(mask_spans(&[(vec![a], false), (vec![b], false)], 50.0), vec![(0.0, 150.0)]);
    }

    #[test]
    fn letters_come_out_at_the_requested_height() {
        let font = &bundled_fonts()[1]; // Anton
        let s = Settings { text: "HI".into(), height_mm: 30.0, edge: EdgeKind::None, ..Default::default() };
        let p = generate(font, &s).unwrap();
        let b = p.bounds();
        assert!(((b.3 - b.1) / 10.0 - 30.0).abs() < 1.0, "height {}", (b.3 - b.1) / 10.0);
        assert_eq!(p.blocks().len(), 1);
    }

    #[test]
    fn separate_edge_thread_adds_a_color_change() {
        let font = &bundled_fonts()[0];
        let s = Settings { text: "O".into(), ..Default::default() };
        let p = generate(font, &s).unwrap();
        assert_eq!(p.blocks().len(), 2);
        assert_eq!(p.threads[0].name, "Cardinal Red");
        assert_eq!(p.threads[1].name, "Black");
    }

    #[test]
    fn every_fill_style_stitches_inside_the_letters() {
        let font = &bundled_fonts()[1];
        for fill in [FillKind::Tatami, FillKind::Satin, FillKind::Wave, FillKind::Zigzag, FillKind::Crosshatch, FillKind::Gradient, FillKind::Open, FillKind::Contour] {
            let s = Settings { text: "HO".into(), fill, density_mm: fill.default_rows_mm(), edge: EdgeKind::None, underlay: false, ..Default::default() };
            let p = generate(font, &s).unwrap();
            let solid = generate(font, &Settings { fill: FillKind::Tatami, ..s.clone() }).unwrap().bounds();
            let b = p.bounds();
            assert!(p.count(Cmd::Stitch) > 100, "{fill:?}");
            // Stitches stay within the letters (allowing for rounding at the edge).
            assert!(b.0 >= solid.0 - 5.0 && b.2 <= solid.2 + 5.0 && b.1 >= solid.1 - 5.0 && b.3 <= solid.3 + 5.0, "{fill:?} {b:?} {solid:?}");
        }
    }

    #[test]
    fn a_new_thread_never_stitches_where_the_last_one_stopped() {
        let mut st = Stitcher::new();
        st.travel((0.0, 0.0), (100.0, 0.0));
        st.stitch((100.0, 0.0));
        st.color_change();
        let end = st.points.len();
        st.travel((300.0, 0.0), (400.0, 0.0));
        st.stitch((400.0, 0.0));
        // After the change: one jump to the new start, then stitches near it.
        assert_eq!(st.cmds[end], Cmd::Jump);
        assert!(st.points[end..].iter().all(|p| p.0 >= 300.0), "{:?}", &st.points[end..]);
        // The first thread was tied off at its end before the change.
        assert!(st.points[..end].iter().filter(|p| p.0 > 90.0).count() >= 3);
    }

    #[test]
    fn patch_adds_background_and_border_threads_around_the_text() {
        let font = &bundled_fonts()[1];
        let s = Settings { text: "AB".into(), patch: PatchShape::Rounded, ..Default::default() };
        let p = generate(font, &s).unwrap();
        // background, text fill, text edge, border
        assert_eq!(p.blocks().len(), 4);
        let plain = generate(font, &Settings { patch: PatchShape::None, ..s.clone() }).unwrap().bounds();
        let b = p.bounds();
        assert!(b.2 - b.0 > plain.2 - plain.0 + 2.0 * 50.0, "patch is wider than text plus padding");
        // A border in the same thread as the edge doesn't add a change.
        let same = Settings { patch_border_thread: s.edge_thread, patch_fill: FillKind::None, ..s.clone() };
        assert_eq!(generate(font, &same).unwrap().blocks().len(), 2);
    }

    #[test]
    fn applique_adds_two_stops_before_the_text() {
        let font = &bundled_fonts()[1];
        let s = Settings { text: "AB".into(), patch: PatchShape::Oval, patch_applique: true, patch_thread: 1, edge_thread: 1, fill_thread: 1, patch_border_thread: 1, ..Default::default() };
        let p = generate(font, &s).unwrap();
        let names: Vec<&str> = p.threads.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names[0], "Placement line");
        assert!(names[1].starts_with("Tack-down"));
        // The rest is one thread, so no further stops.
        assert_eq!(p.blocks().len(), 3, "{names:?}");
    }

    #[test]
    fn empty_text_is_reported() {
        let s = Settings { text: "  ".into(), ..Default::default() };
        assert!(generate(&bundled_fonts()[0], &s).is_err());
    }

    #[test]
    fn distance_transform_is_exact_on_a_line() {
        let f = [0.0, 1e12, 1e12, 1e12, 0.0, 1e12];
        let mut out = [0.0; 6];
        edt_1d(&f, &mut out);
        assert_eq!(out, [0.0, 1.0, 4.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn walk_spaces_points_evenly() {
        let square = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let pts = walk(&square, 0, 25.0);
        assert_eq!(pts.len(), 17);
        assert!(pts.windows(2).all(|w| (dist(w[0], w[1]) - 25.0).abs() < 0.01));
    }
}

#[cfg(test)]
mod samples {
    use super::*;

    /// Writes sample lettering as JEF files for a visual check:
    /// HOOP_OUT=/tmp/dir cargo test samples -- --ignored
    #[test]
    #[ignore]
    fn write_samples() {
        let Some(out) = std::env::var_os("HOOP_OUT") else { return };
        let fonts = bundled_fonts();
        let cases: Vec<(String, usize, Settings)> = vec![
            ("contour".into(), 0, Settings { text: "GO".into(), height_mm: 45.0, fill: FillKind::Contour, density_mm: 0.5, edge: EdgeKind::None, ..Default::default() }),
            ("column".into(), std::env::var("F").ok().and_then(|v| v.parse().ok()).unwrap_or(0), Settings { text: std::env::var("HOOP_TEXT").unwrap_or("Bremen".into()), height_mm: std::env::var("HOOP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0), fill: FillKind::Column, density_mm: 0.4, edge: EdgeKind::None, ..Default::default() }),
        ];
        let only = std::env::var("HOOP_CASE").ok();
        let cases: Vec<_> = cases.into_iter().filter(|c| only.as_ref().is_none_or(|o| *o == c.0)).collect();
        for (name, font, s) in cases {
            let p = generate(&fonts[font], &s).unwrap();
            let threads: Vec<i32> = p.threads.iter().map(|t| formats::nearest_jef(t.color)).collect();
            let b = p.bounds();
            let pts: Vec<Pt> = p.points.iter().map(|&(x, y)| (x - (b.0 + b.2) / 2.0, y - (b.1 + b.3) / 2.0)).collect();
            std::fs::write(std::path::Path::new(&out).join(format!("{name}.jef")), formats::write_jef(&pts, &p.cmds, &threads)).unwrap();
        }
    }
}

/// One-click looks for lettering. They change only the stitch style, never
/// the text, font, size or threads.
pub const PRESETS: [(&str, &str, fn(&mut Settings)); 10] = [
    (n!("Classic"), n!("Tatami fill with a satin border"), |s| {
        style(s, FillKind::Tatami, 45.0, EdgeKind::Satin);
        s.edge_width_mm = 1.6;
    }),
    (n!("Satin"), n!("Satin columns that follow each stroke, as digitizers sew lettering. Best for small text"), |s| style(s, FillKind::Column, 90.0, EdgeKind::None)),
    (n!("Outline"), n!("Only a triple-run outline"), |s| style(s, FillKind::None, 45.0, EdgeKind::Triple)),
    (n!("Bold"), n!("Tatami with a wide satin border"), |s| {
        style(s, FillKind::Tatami, 45.0, EdgeKind::Satin);
        s.edge_width_mm = 2.8;
    }),
    (n!("Vintage"), n!("Open crosshatch inside a thin border"), |s| {
        style(s, FillKind::Crosshatch, 45.0, EdgeKind::Satin);
        s.edge_width_mm = 1.2;
    }),
    (n!("Wavy"), n!("Wave fill with a satin border"), |s| {
        style(s, FillKind::Wave, 0.0, EdgeKind::Satin);
        s.edge_width_mm = 1.4;
    }),
    (n!("Shaded"), n!("Gradient fill with a running outline"), |s| {
        style(s, FillKind::Gradient, 90.0, EdgeKind::Running);
        s.fade_mm = 3.0;
    }),
    (n!("Key fob"), n!("In the hoop: an appliqué tag with a key-ring eyelet, finished on both sides"), |s| {
        style(s, FillKind::Column, 90.0, EdgeKind::None);
        s.height_mm = 10.0;
        s.patch = PatchShape::Fob;
        s.patch_pad_mm = 4.0;
        s.patch_border_mm = 3.0;
        s.patch_applique = true;
        s.patch_fill = FillKind::None;
        s.patch_ith = true;
    }),
    (n!("Coaster"), n!("In the hoop: a round coaster, appliqué front and backed fabric"), |s| {
        style(s, FillKind::Column, 90.0, EdgeKind::None);
        s.patch = PatchShape::Circle;
        s.patch_pad_mm = 12.0;
        s.patch_border_mm = 3.5;
        s.patch_applique = true;
        s.patch_fill = FillKind::None;
        s.patch_ith = true;
    }),
    (n!("Lace"), n!("Free-standing lace: a lattice that holds together once the water-soluble stabilizer is rinsed away"), |s| {
        style(s, FillKind::Column, 90.0, EdgeKind::None);
        s.patch = PatchShape::Circle;
        s.patch_pad_mm = 8.0;
        s.patch_border_mm = 3.0;
        s.patch_applique = false;
        s.patch_fill = FillKind::Crosshatch;
        s.patch_ith = false;
    }),
];

pub fn style(s: &mut Settings, fill: FillKind, angle: f32, edge: EdgeKind) {
    s.fill = fill;
    s.fill_angle = angle;
    s.edge = edge;
    s.density_mm = fill.default_rows_mm();
    s.underlay = !matches!(fill, FillKind::Open | FillKind::Crosshatch);
}
