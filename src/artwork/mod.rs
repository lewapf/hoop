//! Turns SVG artwork into stitches. usvg resolves the file into plain paths
//! with absolute transforms; filled paths become fill regions, stroked paths
//! become running or satin lines, and every color becomes one thread.

use std::collections::HashSet;
use std::path::Path;

mod builtin;
mod optimize;
mod sew;
mod svg;
mod threads;

use crate::formats::{self, Cmd, Format, Pattern};
use crate::lettering::{self, Contour, EdgeKind, FillKind, Line, MaskShape, Pt, Settings, Stitcher};

pub use builtin::*;
pub use optimize::*;
pub use sew::*;
pub use svg::*;
pub use threads::*;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum Kind {
    Fill { even_odd: bool },
    Stroke { width: f32 },
}

/// One painted path. Coordinates are SVG user units, y down.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    /// Polylines with whether each one is closed.
    pub lines: Vec<(Contour, bool)>,
    pub layer: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Layer {
    pub color: [u8; 3],
    pub background: bool,
    pub shapes: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Artwork {
    pub name: String,
    pub shapes: Vec<Shape>,
    pub layers: Vec<Layer>,
    /// Bounds of everything in user units: min x, min y, max x, max y.
    pub bounds: (f32, f32, f32, f32),
    pub skipped_images: usize,
}

impl Artwork {
    pub fn aspect(&self) -> f32 {
        (self.bounds.3 - self.bounds.1) / (self.bounds.2 - self.bounds.0).max(1e-3)
    }
}

/// Per-color choices in the order of `Artwork::layers`.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct LayerChoice {
    pub thread: i32,
    pub enabled: bool,
    pub look: Look,
}

/// How one thread's shapes are stitched, overriding the main fill style.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Default,
    Fill(FillKind),
    Outline,
    /// Sewn on as a piece of fabric: placement line, stop, tack-down, stop, satin cover.
    Applique,
    /// Not sewn: the fabric shows here. Cuts its area out of every shape
    /// below it, so nothing is sewn "through" it.
    Hole,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ArtSettings {
    pub width_mm: f32,
    /// Fill style, edge and stitch settings shared with lettering. Its text,
    /// font and thread fields are unused here.
    pub style: Settings,
    pub knockout: bool,
    /// Colors switched off in Threads leave the fabric showing, like
    /// holes, instead of letting shapes below be sewn through them.
    pub off_leaves_fabric: bool,
    pub min_detail_mm: f32,
    pub stroke: EdgeKind,
    pub stroke_min_mm: f32,
    pub group_colors: bool,
    pub layers: Vec<LayerChoice>,
    /// The shapes as the user arranged them, in stitch order (first is sewn
    /// first and lies at the bottom).
    pub shapes: Vec<ShapeEdit>,
    /// Named groups of shapes, like folders of layers. A group's shapes sit
    /// next to each other in `shapes` and are sewn as one block.
    pub groups: Vec<Group>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Group {
    pub id: u32,
    pub name: String,
}

impl ArtSettings {
    /// The stacking order that brings each group's shapes together where
    /// its lowest one sits, keeping every other order the same; None when
    /// they already are.
    pub fn gathered(&self) -> Option<Vec<usize>> {
        let mut order = Vec::with_capacity(self.shapes.len());
        let mut done = vec![false; self.shapes.len()];
        for (i, e) in self.shapes.iter().enumerate() {
            if done[i] {
                continue;
            }
            match e.group {
                Some(g) => {
                    for (k, f) in self.shapes.iter().enumerate().skip(i) {
                        if f.group == Some(g) {
                            order.push(k);
                            done[k] = true;
                        }
                    }
                }
                None => {
                    order.push(i);
                    done[i] = true;
                }
            }
        }
        order.iter().enumerate().any(|(a, &b)| a != b).then_some(order)
    }

    /// An id no shape has yet.
    pub fn fresh_id(&self) -> u32 {
        self.shapes.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }

    /// Adds copies of shapes on top with ids of their own. Lines attached
    /// to shapes copied along stay attached to the copies; ends attached
    /// to anything else come loose. Returns the copies' indices.
    pub fn add_copies(&mut self, copies: Vec<ShapeEdit>) -> std::ops::Range<usize> {
        let first = self.shapes.len();
        let mut map = std::collections::HashMap::new();
        let mut copies = copies;
        for (next, c) in (self.fresh_id()..).zip(copies.iter_mut()) {
            map.insert(c.id, next);
            c.id = next;
        }
        for c in &mut copies {
            if let Some(Made::Line { from, to, .. }) = &mut c.made {
                for end in [from, to] {
                    *end = end.and_then(|an| map.get(&an.shape).map(|&id| Anchor { shape: id, ..an }));
                }
            }
        }
        self.shapes.extend(copies);
        first..self.shapes.len()
    }

    /// Gives copies that share an id (from duplicating or breaking apart)
    /// new ones; the first keeps it. Returns whether any changed.
    pub fn fix_ids(&mut self) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.fresh_id();
        let mut changed = false;
        for e in &mut self.shapes {
            if !seen.insert(e.id) {
                e.id = next;
                next += 1;
                changed = true;
            }
        }
        changed
    }

    /// Moves the attached ends of lines to where their shapes now are.
    /// Ends whose shape is gone stay where they were and come loose.
    /// Returns whether anything moved.
    pub fn follow_lines(&mut self, art: &Artwork) -> bool {
        let mut changed = false;
        for k in 0..self.shapes.len() {
            let Some(Made::Line { a, b, bend, from, to }) = self.shapes[k].made else { continue };
            if from.is_none() && to.is_none() {
                continue;
            }
            let line = &self.shapes[k];
            let end = |p: Pt, anchor: Option<Anchor>| -> (Pt, Option<Anchor>) {
                let Some(an) = anchor else { return (p, None) };
                match self.shapes.iter().find(|e| e.id == an.shape) {
                    Some(t) => (line.unplace(art, t.place(art, an.at)), anchor),
                    None => (p, None),
                }
            };
            let (na, nf) = end(a, from);
            let (nb, nt) = end(b, to);
            let moved = |p: Pt, q: Pt| (p.0 - q.0).abs() > 1e-3 || (p.1 - q.1).abs() > 1e-3;
            if moved(na, a) || moved(nb, b) || nf != from || nt != to {
                let e = &mut self.shapes[k];
                e.made = Some(Made::Line { a: na, b: nb, bend, from: nf, to: nt });
                e.remake();
                changed = true;
            }
        }
        changed
    }

    /// Drops the names of groups with no shapes left.
    pub fn prune_groups(&mut self) {
        let shapes = &self.shapes;
        self.groups.retain(|g| shapes.iter().any(|e| e.group == Some(g.id)));
    }

    /// The shapes of a group, bottom first.
    pub fn members(&self, id: u32) -> Vec<usize> {
        (0..self.shapes.len()).filter(|&i| self.shapes[i].group == Some(id)).collect()
    }
}

/// One shape on the design: a shape of the drawing, or its own geometry once
/// its points were edited or it was added from the built-in shapes, plus
/// how it is moved, scaled and turned. Offsets are SVG user units.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ShapeEdit {
    pub source: usize,
    pub own: Option<Shape>,
    pub name: String,
    pub layer: usize,
    /// How this shape is stitched, overriding its thread's look.
    pub look: Option<Look>,
    pub hidden: bool,
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub rotate: f32,
    /// Stitch angle lines for the fill, in the shape's own coordinates
    /// (before the move). One line sets the angle; several split the shape
    /// into parts, each nearest one line and sewn at its angle.
    pub angles: Vec<(Pt, Pt)>,
    /// Where the needle enters the shape, in the shape's own coordinates.
    pub entry: Option<Pt>,
    /// The shape's own fill and edge style, instead of the drawing's.
    pub style: Option<Settings>,
    /// Placed by hand in the Objects list: sewn exactly where it sits, after
    /// everything below it and before everything above, even when grouping
    /// by thread would move it.
    pub pinned: bool,
    /// Drawn with a shape tool: the numbers it was made from, so it can be
    /// changed later. Cleared once its points are edited by hand.
    pub made: Option<Made>,
    /// The group it belongs to, by `Group::id`.
    pub group: Option<u32>,
    /// Stays the same as the list is rearranged, for lines attached to it.
    /// Unique within a drawing (`ArtSettings::fix_ids` sees to that).
    pub id: u32,
}

impl ShapeEdit {
    pub fn of(art: &Artwork, i: usize) -> Self {
        let sh = &art.shapes[i];
        let kind = match sh.kind {
            Kind::Fill { .. } => t!("Fill"),
            Kind::Stroke { .. } => t!("Line"),
        };
        ShapeEdit {
            source: i,
            own: None,
            name: format!("{kind} {}", i + 1),
            layer: sh.layer,
            look: None,
            hidden: false,
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            rotate: 0.0,
            angles: Vec::new(),
            entry: None,
            style: None,
            pinned: false,
            made: None,
            group: None,
            id: i as u32 + 1,
        }
    }

    /// Redraws a shape made with a shape tool from its numbers, keeping its
    /// middle where it was.
    pub fn remake(&mut self) {
        let (Some(m), Some(own)) = (&self.made, &mut self.own) else { return };
        let at = middle(&own.lines);
        let mut lines = made_lines(m);
        let c = middle(&lines);
        // A line keeps its own ends; the closed shapes keep their middle.
        let shift = if matches!(m, Made::Line { .. }) { (0.0, 0.0) } else { (at.0 - c.0, at.1 - c.1) };
        for (l, _) in &mut lines {
            for p in l.iter_mut() {
                *p = (p.0 + shift.0, p.1 + shift.1);
            }
        }
        own.lines = lines;
    }

    /// A point in the shape's own coordinates, moved like the shape.
    pub fn place(&self, art: &Artwork, p: Pt) -> Pt {
        if !self.moved() {
            return p;
        }
        let (cx, cy) = middle(&self.base(art).lines);
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let (x, y) = ((p.0 - cx) * self.scale, (p.1 - cy) * self.scale);
        (x * cos - y * sin + cx + self.dx, x * sin + y * cos + cy + self.dy)
    }

    /// The inverse of `place`: a point in user units to the shape's own.
    pub fn unplace(&self, art: &Artwork, p: Pt) -> Pt {
        if !self.moved() {
            return p;
        }
        let (cx, cy) = middle(&self.base(art).lines);
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let (x, y) = (p.0 - cx - self.dx, p.1 - cy - self.dy);
        let k = self.scale.max(1e-6);
        ((x * cos + y * sin) / k + cx, (-x * sin + y * cos) / k + cy)
    }

    pub fn base<'a>(&'a self, art: &'a Artwork) -> &'a Shape {
        match &self.own {
            Some(own) => own,
            None => &art.shapes[self.source],
        }
    }

    pub fn moved(&self) -> bool {
        self.dx != 0.0 || self.dy != 0.0 || self.scale != 1.0 || self.rotate != 0.0
    }

    /// The outline with the move, scale and turn applied, in user units.
    /// Scale and turn go around the middle of the shape.
    pub fn lines(&self, art: &Artwork) -> Vec<(Contour, bool)> {
        let sh = self.base(art);
        if !self.moved() {
            return sh.lines.clone();
        }
        let (cx, cy) = middle(&sh.lines);
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let k = self.scale;
        let f = |p: Pt| {
            let (x, y) = ((p.0 - cx) * k, (p.1 - cy) * k);
            (x * cos - y * sin + cx + self.dx, x * sin + y * cos + cy + self.dy)
        };
        sh.lines.iter().map(|(c, closed)| (c.iter().map(|&p| f(p)).collect(), *closed)).collect()
    }

    /// Bakes the move into the points so they can be edited one by one.
    pub fn bake(&mut self, art: &Artwork) {
        // Its points are about to be edited by hand.
        self.made = None;
        if self.own.is_some() && !self.moved() {
            return;
        }
        let mut sh = self.base(art).clone();
        sh.lines = self.lines(art);
        self.angles = self.angles.iter().map(|&(a, b)| (self.place(art, a), self.place(art, b))).collect();
        self.entry = self.entry.map(|p| self.place(art, p));
        self.own = Some(sh);
        (self.dx, self.dy, self.scale, self.rotate) = (0.0, 0.0, 1.0, 0.0);
    }
}

/// Bounding box of some lines: min x, min y, max x, max y. Empty lines give
/// an inverted box.
pub fn extent(lines: &[(Contour, bool)]) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in lines.iter().flat_map(|l| l.0.iter()) {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    b
}

/// Middle of the bounding box of some lines.
pub fn middle(lines: &[(Contour, bool)]) -> Pt {
    let b = extent(lines);
    if b.0 > b.2 { (0.0, 0.0) } else { ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0) }
}

/// How user units map to stitch units (0.1 mm): scale and the frame's middle.
pub fn frame(art: &Artwork, s: &ArtSettings) -> (f32, Pt) {
    let scale = s.width_mm * 10.0 / (art.bounds.2 - art.bounds.0).max(1e-3);
    (scale, ((art.bounds.0 + art.bounds.2) / 2.0, (art.bounds.1 + art.bounds.3) / 2.0))
}

/// The topmost visible shape under a point in stitch units, within `slop`.
pub fn shape_at(art: &Artwork, s: &ArtSettings, p: Pt, slop: f32) -> Option<usize> {
    let (scale, c) = frame(art, s);
    let q = (p.0 / scale + c.0, p.1 / scale + c.1);
    let slop = slop / scale;
    (0..s.shapes.len()).rev().find(|&i| {
        let e = &s.shapes[i];
        if e.hidden || !s.layers.get(e.layer).is_none_or(|l| l.enabled) {
            return false;
        }
        let lines = e.lines(art);
        let inside = matches!(e.base(art).kind, Kind::Fill { .. }) && {
            let mut wind = false;
            for (c, _) in &lines {
                for k in 0..c.len() {
                    let (a, b) = (c[k], c[(k + 1) % c.len()]);
                    if (a.1 > q.1) != (b.1 > q.1) && q.0 < a.0 + (q.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                        wind = !wind;
                    }
                }
            }
            wind
        };
        inside || lines.iter().any(|(c, closed)| near_line(c, *closed, q) <= slop.max(stroke_half(e.base(art))))
    })
}

fn stroke_half(sh: &Shape) -> f32 {
    match sh.kind {
        Kind::Stroke { width } => width / 2.0,
        Kind::Fill { .. } => 0.0,
    }
}

/// Distance from a point to a polyline.
pub fn near_line(c: &Contour, closed: bool, q: Pt) -> f32 {
    let n = c.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    let mut best = c.first().map_or(f32::MAX, |&a| lettering::dist(a, q));
    for k in 0..segs {
        let (a, b) = (c[k], c[(k + 1) % n]);
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let t = (((q.0 - a.0) * vx + (q.1 - a.1) * vy) / (vx * vx + vy * vy).max(1e-9)).clamp(0.0, 1.0);
        best = best.min(lettering::dist((a.0 + vx * t, a.1 + vy * t), q));
    }
    best
}

impl ArtSettings {
    pub fn for_artwork(art: &Artwork) -> Self {
        let width = (art.bounds.2 - art.bounds.0) * 25.4 / 96.0;
        // Start at the artwork's own size when it's sensible, else 80 mm wide.
        let width_mm = if (20.0..=180.0).contains(&width) { width.round() } else { 80.0 };
        ArtSettings {
            width_mm,
            style: Settings { fill: FillKind::Auto, edge: EdgeKind::None, underlay: true, fill_angle: 45.0, ..Default::default() },
            knockout: true,
            off_leaves_fabric: true,
            min_detail_mm: 1.0,
            stroke: EdgeKind::Satin,
            stroke_min_mm: 1.0,
            group_colors: true,
            layers: art.layers.iter().map(|l| LayerChoice { thread: formats::nearest_jef(l.color), enabled: !l.background, look: Look::Default }).collect(),
            shapes: (0..art.shapes.len()).map(|i| ShapeEdit::of(art, i)).collect(),
            groups: Vec::new(),
        }
    }
}

fn area(c: &Contour) -> f32 {
    let mut a = 0.0;
    for i in 0..c.len() {
        let (p, q) = (c[i], c[(i + 1) % c.len()]);
        a += p.0 * q.1 - q.0 * p.1;
    }
    (a / 2.0).abs()
}

fn length(c: &Contour) -> f32 {
    c.windows(2).map(|w| lettering::dist(w[0], w[1])).sum()
}

fn segment_dist(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    lettering::dist(p, (a.0 + dx * t, a.1 + dy * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_lines_split_a_fill_into_parts_with_their_own_direction() {
        let sq = vec![vec![(0.0, 0.0), (400.0, 0.0), (400.0, 200.0), (0.0, 200.0)]];
        // Left: rows across (0°); right: rows up and down (90°).
        let lines = [((50.0, 100.0), (150.0, 100.0)), ((300.0, 50.0), (300.0, 150.0))];
        let parts = angle_parts(&lines, &sq, 45.0);
        assert_eq!(parts.len(), 2);
        assert!((parts[0].1 - 0.0).abs() < 0.1 && (parts[1].1 - 90.0).abs() < 0.1);
        let s = Settings { density_mm: 0.5, pull_mm: 0.0, ..Settings::default() };
        let mut left_h = 0;
        let mut right_v = 0;
        let mut wrong = 0;
        for (cut, angle) in &parts {
            let mut st = Stitcher::new();
            lettering::fill_shape(&mut st, &sq, false, cut, FillKind::Tatami, *angle, &s);
            for w in st.points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
                if dx.max(dy) < 15.0 {
                    continue;
                }
                let mid = (a.0 + b.0) / 2.0;
                match (mid < 200.0, dx > dy) {
                    (true, true) => left_h += 1,
                    (false, false) => right_v += 1,
                    _ => wrong += 1,
                }
            }
        }
        assert!(left_h > 50 && right_v > 50, "{left_h} {right_v}");
        assert!(wrong * 10 < left_h + right_v, "{wrong}");
    }

    const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
        <rect width="100" height="50" fill="#ffffff"/>
        <rect x="10" y="10" width="40" height="30" fill="#ff0000"/>
        <circle cx="45" cy="25" r="10" fill="#0000ff"/>
        <path d="M60 10 L90 40" stroke="#000000" stroke-width="2" fill="none"/>
    </svg>"##;

    #[test]
    fn colors_become_layers_and_background_is_off() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        assert_eq!(art.layers.len(), 4);
        assert!(art.layers[0].background);
        let s = ArtSettings::for_artwork(&art);
        assert!(!s.layers[0].enabled);
        assert_eq!(s.layers[1].thread, formats::nearest_jef([255, 0, 0]));
    }

    #[test]
    fn generates_one_thread_per_color_at_the_chosen_width() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.width_mm = 100.0;
        let p = generate(&art, &s).unwrap();
        assert_eq!(p.blocks().len(), 3);
        let b = p.bounds();
        // Background is off, so the art spans from the red rect to the line's end.
        assert!((b.2 - b.0) / 10.0 > 75.0 && (b.2 - b.0) / 10.0 < 90.0, "{:?}", b);
    }

    #[test]
    fn knockout_leaves_covered_area_unstitched() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.stroke = EdgeKind::None;
        s.style.underlay = false;
        s.width_mm = 100.0;
        // Red stitches well inside the blue circle, which sits on top of the red square.
        let inside = |p: &Pattern| {
            let red = &p.blocks()[0];
            (red.start..red.end).filter(|&i| p.cmds[i] == Cmd::Stitch && lettering::dist(p.points[i], (-50.0, 0.0)) < 70.0).count()
        };
        let with = inside(&generate(&art, &s).unwrap());
        s.knockout = false;
        let without = inside(&generate(&art, &s).unwrap());
        assert!(with * 10 < without, "{with} vs {without}");
    }

    #[test]
    fn lines_under_later_fills_are_cut_away() {
        // A line across a square drawn on top of it: only the parts outside
        // the square, short of its edge by the margin, are left.
        let square = vec![(vec![vec![(100.0, -50.0), (200.0, -50.0), (200.0, 50.0), (100.0, 50.0)]], false)];
        let (pieces, sewn) = clip_line(&(vec![(0.0, 0.0), (300.0, 0.0)], false), &square, 10.0);
        assert_eq!(pieces.len(), 2);
        let ends: Vec<f32> = pieces.iter().flat_map(|(c, _)| [c[0].0, c[c.len() - 1].0]).collect();
        assert!((ends[0] - 0.0).abs() < 1.0 && (ends[1] - 90.0).abs() < 3.0, "{ends:?}");
        assert!((ends[2] - 210.0).abs() < 3.0 && (ends[3] - 300.0).abs() < 1.0, "{ends:?}");
        // As sewn, the pieces run on to the square's edge.
        let ends: Vec<f32> = sewn.iter().flat_map(|(c, _)| [c[0].0, c[c.len() - 1].0]).collect();
        assert!((ends[1] - 100.0).abs() < 3.0 && (ends[2] - 200.0).abs() < 3.0, "{ends:?}");
        // A loop around the square is left whole; a loop inside it is gone.
        let around = (vec![(50.0, -100.0), (250.0, -100.0), (250.0, 100.0), (50.0, 100.0)], true);
        assert_eq!(clip_line(&around, &square, 10.0).0, vec![around.clone()]);
        assert!(clip_line(&(vec![(130.0, -20.0), (170.0, -20.0), (170.0, 20.0), (130.0, 20.0)], true), &square, 10.0).1.is_empty());
    }

    #[test]
    fn outlines_hidden_in_the_drawing_are_not_sewn() {
        // A black-outlined red square under a blue circle: no black stitch
        // may land well inside the circle, and black still sews in one run.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
            <rect x="10" y="10" width="40" height="40" fill="#ff0000" stroke="#000000" stroke-width="1"/>
            <circle cx="50" cy="30" r="12" fill="#0000ff" stroke="#000000" stroke-width="1"/>
        </svg>"##;
        let art = parse("hidden.svg", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.width_mm = 100.0;
        let p = generate(&art, &s).unwrap();
        // The circle's middle in stitch space: design x 50 of 10..62 wide art.
        let (scale, (cx, cy)) = frame(&art, &s);
        let mid = ((50.0 - cx) * scale, (30.0 - cy) * scale);
        let r = 12.0 * scale;
        let black: Vec<_> = p.blocks().into_iter().filter(|b| b.thread.color == [0, 0, 0]).collect();
        assert_eq!(black.len(), 1, "black should sew once");
        let b = &black[0];
        let hidden = (b.start..b.end).filter(|&i| p.cmds[i] == Cmd::Stitch && lettering::dist(p.points[i], mid) < r - 15.0).count();
        assert_eq!(hidden, 0);
    }

    #[test]
    fn grouping_never_sews_a_shape_under_one_below_it() {
        // Black silhouette, white key on it, black detail on the key, and a
        // black mark far away. The detail must wait for the white; the far
        // mark may join the first black run.
        let sq = |x: f32, y: f32, w: f32| vec![(vec![(x, y), (x + w, y), (x + w, y + w), (x, y + w)], true)];
        let cover: Vec<Coverage> = [sq(0.0, 0.0, 100.0), sq(10.0, 10.0, 80.0), sq(40.0, 40.0, 20.0), sq(500.0, 0.0, 20.0)]
            .iter()
            .map(|l| Coverage::new(l, Some(false), 0.0))
            .collect();
        let threads = [1, 2, 1, 1];
        assert_eq!(layered_order(&threads, |a, b| cover[a].overlaps(&cover[b])), vec![0, 3, 1, 2]);
        // A thin line inside a ring's hole doesn't touch the ring.
        let ring = vec![
            (vec![(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)], true),
            (vec![(50.0, 50.0), (50.0, 250.0), (250.0, 250.0), (250.0, 50.0)], true),
        ];
        let line = vec![(vec![(100.0, 150.0), (200.0, 150.0)], false)];
        assert!(!Coverage::new(&ring, Some(false), 0.0).overlaps(&Coverage::new(&line, None, 5.0)));
    }

    #[test]
    fn shared_threads_need_no_thread_change_and_reduce_merges() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.layers[2].thread = s.layers[1].thread;
        assert_eq!(generate(&art, &s).unwrap().blocks().len(), 2);
        let mut s = ArtSettings::for_artwork(&art);
        reduce_threads(&art, &mut s, 1);
        assert_eq!(generate(&art, &s).unwrap().blocks().len(), 1);
        assert_eq!(thread_groups(&art, &s).len(), 2); // background stays apart
    }

    #[test]
    fn a_thread_can_be_outline_only() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.style.fill = FillKind::Tatami;
        s.stroke = EdgeKind::None;
        let filled = generate(&art, &s).unwrap().blocks()[0].stitches;
        s.layers[1].look = Look::Outline;
        let outline = generate(&art, &s).unwrap().blocks()[0].stitches;
        assert!(outline * 2 < filled, "{outline} vs {filled}");
    }

    #[test]
    fn applique_thread_stops_twice_and_covers_with_satin() {
        let art = parse("logo.svg", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.layers[1].look = Look::Applique;
        let p = generate(&art, &s).unwrap();
        assert_eq!(p.threads[0].name, "Placement line");
        assert!(p.threads[1].name.starts_with("Tack-down"));
        assert_eq!(p.blocks().len(), 5);
    }

    #[test]
    fn holes_leave_fabric_under_them() {
        let art = parse("logo", LOGO.as_bytes()).unwrap();
        let red = |s: &ArtSettings| -> Vec<Pt> {
            let p = generate(&art, s).unwrap();
            let name = formats::jef_thread(formats::nearest_jef([255, 0, 0]), 0).name;
            let b = p.blocks().into_iter().find(|b| b.thread.name == name).unwrap();
            (b.start..b.end).filter(|&i| p.cmds[i] == Cmd::Stitch).map(|i| p.points[i]).collect()
        };
        let mut s = ArtSettings::for_artwork(&art);
        let (scale, mid) = frame(&art, &s);
        // The blue circle (center 45,25, r 10) lies on the red square's right edge.
        let c = ((45.0 - mid.0) * scale, (25.0 - mid.1) * scale);
        let r = 8.0 * scale;
        let inside = |pts: &[Pt]| pts.iter().filter(|p| lettering::dist(**p, c) < r).count();
        let blue_shape = s.shapes.iter().position(|e| art.layers[e.layer].color == [0, 0, 255]).unwrap();
        let blue_layer = art.shapes[blue_shape].layer;
        // Knockout on: red stops under the blue circle (a travel run may
        // still pass under it, hidden by the circle).
        assert!(inside(&red(&s)) <= 3);
        // The circle as a hole: not sewn, red still stops there.
        s.shapes[blue_shape].look = Some(Look::Hole);
        let p = generate(&art, &s).unwrap();
        assert!(p.blocks().len() < generate(&art, &ArtSettings::for_artwork(&art)).unwrap().blocks().len());
        assert_eq!(inside(&red(&s)), 0);
        // Even with knockout off, a hole cuts.
        s.knockout = false;
        assert_eq!(inside(&red(&s)), 0);
        // A switched-off color leaves fabric too, unless that is turned off.
        s.shapes[blue_shape].look = None;
        s.layers[blue_layer].enabled = false;
        assert_eq!(inside(&red(&s)), 0);
        s.off_leaves_fabric = false;
        let through = inside(&red(&s));
        assert!(through > 8, "sewn through: {through}");
    }

    #[test]
    fn a_shape_with_its_own_style_changes_alone() {
        let art = parse("logo", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        let per_thread =
            |s: &ArtSettings| -> Vec<(String, usize)> { generate(&art, s).unwrap().blocks().into_iter().map(|b| (b.thread.name, b.stitches)).collect() };
        let before = per_thread(&s);
        let red = s.shapes.iter().position(|e| art.layers[e.layer].color == [255, 0, 0]).unwrap();
        let mut own = s.style.clone();
        lettering::style(&mut own, FillKind::Cross, 45.0, EdgeKind::None);
        s.shapes[red].style = Some(own);
        let after = per_thread(&s);
        let red_name = formats::jef_thread(s.layers[art.shapes[red].layer].thread, 0).name;
        for ((name, b), (_, a)) in before.iter().zip(&after) {
            if *name == red_name {
                assert_ne!(a, b, "the red square changed");
            } else {
                assert_eq!(a, b, "{name} kept its stitches");
            }
        }
    }

    #[test]
    fn pinned_shapes_sew_where_they_sit_in_the_list() {
        // Red, blue, red, apart from each other: grouping sews both reds first.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
            <rect x="0" y="0" width="20" height="20" fill="#ff0000"/>
            <rect x="40" y="0" width="20" height="20" fill="#0000ff"/>
            <rect x="80" y="0" width="20" height="20" fill="#ff0000"/>
        </svg>"##;
        let art = parse("three.svg", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        let names = |s: &ArtSettings| generate(&art, s).unwrap().threads.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        let (red, blue) = (formats::jef_thread(s.layers[0].thread, 0).name, formats::jef_thread(s.layers[1].thread, 0).name);
        assert_eq!(names(&s), [red.clone(), blue.clone()]);
        // Blue placed by hand between the reds: sewn there, as listed.
        s.shapes[1].pinned = true;
        assert_eq!(names(&s), [red.clone(), blue.clone(), red.clone()]);
        // Placed at the bottom, it is sewn first.
        let blue_shape = s.shapes.remove(1);
        s.shapes.insert(0, blue_shape);
        assert_eq!(names(&s), [blue, red]);
    }

    #[test]
    fn groups_sew_as_one_block_and_gather_together() {
        // Red, blue, red, apart: alone the reds sew together.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
            <rect x="0" y="0" width="20" height="20" fill="#ff0000"/>
            <rect x="40" y="0" width="20" height="20" fill="#0000ff"/>
            <rect x="80" y="0" width="20" height="20" fill="#ff0000"/>
        </svg>"##;
        let art = parse("three.svg", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        let names = |s: &ArtSettings| generate(&art, s).unwrap().threads.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        let (red, blue) = (formats::jef_thread(s.layers[0].thread, 0).name, formats::jef_thread(s.layers[1].thread, 0).name);
        assert_eq!(names(&s), [red.clone(), blue.clone()]);
        // The blue between grouped reds can't be passed: the group is a block.
        s.groups.push(Group { id: 1, name: "G".into() });
        s.shapes[0].group = Some(1);
        s.shapes[2].group = Some(1);
        assert_eq!(names(&s), [red.clone(), blue.clone(), red.clone()]);
        // Inside a group, shapes still share runs of thread.
        s.shapes[1].group = Some(1);
        assert_eq!(names(&s), [red.clone(), blue.clone()]);
        // Members that drifted apart are brought back to the lowest one.
        let mut t = ArtSettings::for_artwork(&art);
        t.shapes[0].group = Some(7);
        t.shapes[2].group = Some(7);
        assert_eq!(t.gathered(), Some(vec![0, 2, 1]));
        t.shapes.swap(1, 2);
        assert_eq!(t.gathered(), None);
        t.groups.push(Group { id: 9, name: "empty".into() });
        t.groups.push(Group { id: 7, name: "kept".into() });
        t.prune_groups();
        assert_eq!(t.groups.len(), 1);
        assert_eq!(t.members(7), vec![0, 1]);
    }

    #[test]
    fn attached_lines_follow_their_shapes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
            <rect x="0" y="0" width="20" height="20" fill="#ff0000"/>
            <rect x="80" y="0" width="20" height="20" fill="#0000ff"/>
        </svg>"##;
        let art = parse("two.svg", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        // A line from the red square's right edge to the blue one's left edge.
        let made = Made::Line {
            a: (20.0, 10.0),
            b: (80.0, 10.0),
            bend: 0.0,
            from: Some(Anchor { shape: s.shapes[0].id, at: (20.0, 10.0) }),
            to: Some(Anchor { shape: s.shapes[1].id, at: (80.0, 10.0) }),
        };
        let mut line = ShapeEdit::of(&art, 0);
        line.source = usize::MAX;
        line.own = Some(Shape { kind: Kind::Stroke { width: 1.0 }, lines: made_lines(&made), layer: 0 });
        line.made = Some(made);
        line.id = s.fresh_id();
        s.shapes.push(line);
        assert!(!s.follow_lines(&art));
        // Blue moves down 30: the line's far end goes with it, the near end stays.
        s.shapes[1].dy = 30.0;
        assert!(s.follow_lines(&art));
        let ends = &s.shapes[2].own.as_ref().unwrap().lines[0].0;
        assert_eq!((ends[0], *ends.last().unwrap()), ((20.0, 10.0), (80.0, 40.0)));
        // Red goes away: its end stays put and comes loose.
        s.shapes.remove(0);
        s.follow_lines(&art);
        assert!(matches!(s.shapes[1].made, Some(Made::Line { from: None, to: Some(_), .. })));
        // Copies sharing an id get new ones.
        let copy = s.shapes[0].clone();
        s.shapes.push(copy);
        assert!(s.fix_ids());
        let mut ids: Vec<u32> = s.shapes.iter().map(|e| e.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn moving_hiding_and_reordering_shapes() {
        let art = parse("logo", LOGO.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        let before = generate(&art, &s).unwrap().bounds();
        // Move the red square 20 user units left: the design grows that way.
        let red = s.shapes.iter().position(|e| art.layers[e.layer].color == [255, 0, 0]).unwrap();
        s.shapes[red].dx = -20.0;
        let moved = generate(&art, &s).unwrap().bounds();
        assert!(moved.0 < before.0 - 50.0, "{moved:?} vs {before:?}");
        // The point under the moved square now hits it; the old spot hits nothing red.
        let (scale, mid) = frame(&art, &s);
        let at = |u: Pt| ((u.0 - mid.0) * scale, (u.1 - mid.1) * scale);
        assert_eq!(shape_at(&art, &s, at((5.0, 25.0)), 1.0), Some(red));
        assert_ne!(shape_at(&art, &s, at((40.0, 15.0)), 1.0), Some(red));
        // Hidden shapes are skipped, both for stitching and picking.
        s.shapes[red].hidden = true;
        assert_eq!(shape_at(&art, &s, at((5.0, 25.0)), 1.0), None);
        let threads = generate(&art, &s).unwrap().threads.len();
        assert_eq!(threads, 2);
        // Baking keeps the outline and resets the move.
        s.shapes[red].hidden = false;
        s.shapes[red].rotate = 90.0;
        let lines = s.shapes[red].lines(&art);
        s.shapes[red].bake(&art);
        assert!(!s.shapes[red].moved());
        assert_eq!(s.shapes[red].lines(&art), lines);
    }

    #[test]
    fn blank_artwork_takes_built_in_shapes() {
        let art = blank();
        let mut s = ArtSettings::for_artwork(&art);
        assert_eq!(s.width_mm, 100.0);
        assert!(generate(&art, &s).unwrap_err().starts_with("No shapes"));
        for (k, name) in BUILT_IN.iter().enumerate() {
            let sh = built_in(name, (189.0, 189.0), 150.0, k % art.layers.len());
            let mut e = ShapeEdit::of(&Artwork { shapes: vec![sh.clone()], ..blank() }, 0);
            e.own = Some(sh);
            e.source = usize::MAX;
            s.shapes.push(e);
        }
        let p = generate(&art, &s).unwrap();
        let b = p.bounds();
        // 150 user units of a 378-unit, 100 mm frame is about 40 mm.
        assert!((b.2 - b.0) > 350.0 && (b.2 - b.0) < 450.0, "{b:?}");
        assert!(p.threads.len() >= 6);
    }

    #[test]
    fn reports_files_without_shapes() {
        let empty = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;
        assert!(parse("e.svg", empty.as_bytes()).is_err());
        assert!(parse("x.svg", b"not svg").is_err());
    }
}

#[cfg(test)]
mod samples {
    use super::*;

    /// Converts the SVGs listed in HOOP_SVGS (colon separated) to JEF in HOOP_OUT:
    /// HOOP_OUT=/tmp/dir HOOP_SVGS=a.svg:b.svg cargo test svg_samples -- --ignored
    #[test]
    #[ignore]
    fn svg_samples() {
        let (Some(out), Some(list)) = (std::env::var_os("HOOP_OUT"), std::env::var("HOOP_SVGS").ok()) else { return };
        for path in list.split(':') {
            let art = load(Path::new(path)).unwrap();
            let mut s = ArtSettings::for_artwork(&art);
            if std::env::var("HOOP_FILL").is_ok_and(|f| f == "auto") {
                s.style.fill = FillKind::Auto;
            }
            let t = std::time::Instant::now();
            let p = generate(&art, &s).unwrap();
            eprintln!("{path}: {} layers, {} threads, {} stitches, {:?}", art.layers.len(), p.threads.len(), p.count(Cmd::Stitch), t.elapsed());
            let threads: Vec<i32> = p.threads.iter().map(|t| formats::nearest_jef(t.color)).collect();
            let b = p.bounds();
            let pts: Vec<Pt> = p.points.iter().map(|&(x, y)| (x - (b.0 + b.2) / 2.0, y - (b.1 + b.3) / 2.0)).collect();
            let stem = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
            std::fs::write(Path::new(&out).join(format!("svg_{stem}.jef")), formats::write_jef(&pts, &p.cmds, &threads)).unwrap();
        }
    }
}

/// One-click looks for artwork, changing only how shapes are stitched.
pub const PRESETS: [lettering::Preset<ArtSettings>; 4] = [
    (n!("Solid"), n!("Dense tatami, hidden parts cut away, satin lines"), |s| {
        lettering::style(&mut s.style, FillKind::Tatami, 45.0, EdgeKind::None);
        s.knockout = true;
        s.stroke = EdgeKind::Satin;
    }),
    (n!("Patch"), n!("Dense fill with a satin border on every shape"), |s| {
        lettering::style(&mut s.style, FillKind::Tatami, 45.0, EdgeKind::Satin);
        s.style.edge_width_mm = 1.4;
        s.knockout = true;
        s.stroke = EdgeKind::Satin;
    }),
    (n!("Light"), n!("Open fill with a running outline, soft on thin fabric"), |s| {
        lettering::style(&mut s.style, FillKind::Open, 45.0, EdgeKind::Running);
        s.stroke = EdgeKind::Running;
    }),
    (n!("Line art"), n!("Outlines only, no fills"), |s| {
        lettering::style(&mut s.style, FillKind::None, 45.0, EdgeKind::Triple);
        s.stroke = EdgeKind::Triple;
    }),
];

#[cfg(test)]
mod crest_debug {
    use super::*;

    /// HOOP_SVG=file HOOP_OUT=dir: renders the artwork's stitches over its
    /// outlines, cropped to HOOP_CROP=x0,y0,x1,y1 in 0.1 mm when given.
    #[test]
    #[ignore]
    fn render_artwork() {
        let path = std::env::var("HOOP_SVG").unwrap();
        let out = std::env::var("HOOP_OUT").unwrap_or_else(|_| "/tmp".into());
        let art = load(std::path::Path::new(&path)).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        if std::env::var("NO_LINES").is_ok() {
            s.stroke = EdgeKind::None;
        }
        if std::env::var("RUN_LINES").is_ok() {
            s.stroke = EdgeKind::Running;
        }
        if std::env::var("ONLY_LINES").is_ok() {
            s.style.fill = FillKind::None;
        }
        let p = generate(&art, &s).unwrap();
        let r = crate::check::stitches(&p);
        let peak = r.coverage.iter().copied().fold(0.0, f32::max);
        let over = r.coverage.iter().filter(|&&v| v > crate::check::TOO_DENSE).count();
        eprintln!("peak {peak:.1}, cells over the limit {over}");
        let (scale, (cx, cy)) = frame(&art, &s);
        let mut glyphs: Vec<Vec<Contour>> = s
            .shapes
            .iter()
            .filter(|e| !e.hidden)
            .map(|e| e.lines(&art).iter().map(|(c, _)| c.iter().map(|&q| ((q.0 - cx) * scale, (q.1 - cy) * scale)).collect()).collect())
            .collect();
        let mut pts = p.points.clone();
        let mut cmds = p.cmds.clone();
        if let Ok(c) = std::env::var("HOOP_CROP") {
            let v: Vec<f32> = c.split(',').map(|x| x.parse().unwrap()).collect();
            let inside = |q: &Pt| q.0 >= v[0] && q.0 <= v[2] && q.1 >= v[1] && q.1 <= v[3];
            for g in glyphs.iter_mut() {
                g.retain(|c| c.iter().any(inside));
            }
            let keep: Vec<bool> = pts.iter().map(inside).collect();
            let mut k = 0;
            pts.retain(|_| {
                k += 1;
                keep[k - 1]
            });
            let mut k = 0;
            cmds.retain(|_| {
                k += 1;
                keep[k - 1]
            });
        }
        crate::stitch::column::debug::render(&format!("{out}/crest.png"), &glyphs, &pts, &cmds, 14.0);
    }
}

#[cfg(test)]
mod density_split {
    use super::*;

    /// Thread length per 1 mm cell, unsmoothed, over a fixed grid.
    fn grid(p: &Pattern) -> std::collections::HashMap<(i32, i32), f32> {
        let mut g = std::collections::HashMap::new();
        for i in 1..p.cmds.len() {
            if p.cmds[i] != Cmd::Stitch {
                continue;
            }
            let (a, b) = (p.points[i - 1], p.points[i]);
            let n = (lettering::dist(a, b) / 2.0).ceil().max(1.0) as usize;
            let seg = lettering::dist(a, b) / 10.0 / n as f32;
            for k in 0..n {
                let t = (k as f32 + 0.5) / n as f32;
                let q = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                *g.entry(((q.0 / 10.0).floor() as i32, (q.1 / 10.0).floor() as i32)).or_insert(0.0) += seg;
            }
        }
        g
    }

    #[test]
    #[ignore]
    fn split() {
        let path = std::env::var("HOOP_SVG").unwrap();
        let art = load(std::path::Path::new(&path)).unwrap();
        let mk = |f: &dyn Fn(&mut ArtSettings)| {
            let mut s = ArtSettings::for_artwork(&art);
            f(&mut s);
            generate(&art, &s).unwrap()
        };
        let all = grid(&mk(&|_| {}));
        let fills = grid(&mk(&|s| s.stroke = EdgeKind::None));
        let lines = grid(&mk(&|s| s.style.fill = FillKind::None));
        let mut hot: Vec<((i32, i32), f32)> = all.iter().map(|(k, v)| (*k, *v)).filter(|(_, v)| *v > 12.0).collect();
        hot.sort_by(|a, b| b.1.total_cmp(&a.1));
        eprintln!("cells over 12 mm/mm²: {}", hot.len());
        let (mut sf, mut sl) = (0.0, 0.0);
        for (k, v) in hot.iter().take(400) {
            let (f, l) = (fills.get(k).copied().unwrap_or(0.0), lines.get(k).copied().unwrap_or(0.0));
            sf += f;
            sl += l;
            let _ = v;
        }
        let n = hot.len().clamp(1, 400) as f32;
        eprintln!(
            "in those cells, average: all {:.1}, fills alone {:.1}, lines alone {:.1}",
            hot.iter().take(400).map(|h| h.1).sum::<f32>() / n,
            sf / n,
            sl / n
        );
        for (k, v) in hot.iter().take(8) {
            eprintln!("  cell {k:?}: all {v:.1}, fills {:.1}, lines {:.1}", fills.get(k).copied().unwrap_or(0.0), lines.get(k).copied().unwrap_or(0.0));
        }
    }
}
