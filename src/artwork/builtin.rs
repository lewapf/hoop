//! Built-in shapes (circle, heart, star…) and the empty drawing that
//! starts from them.

use super::*;

/// Built-in shapes that can be added to any artwork.
pub const BUILT_IN: [&str; 7] = [n!("Circle"), n!("Square"), n!("Heart"), n!("Star"), n!("Shield"), n!("Banner"), n!("Line")];

/// One built-in shape, `size` user units across, centered on `at`.
pub fn built_in(name: &str, at: Pt, size: f32, layer: usize) -> Shape {
    use std::f32::consts::{PI, TAU};
    let r = size / 2.0;
    let mut kind = Kind::Fill { even_odd: false };
    let pts: Vec<Pt> = match name {
        "Circle" => (0..72).map(|k| (k as f32 * TAU / 72.0).sin_cos()).map(|(s, c)| (c * r, s * r)).collect(),
        "Square" => {
            // Rounded corners stitch cleaner than sharp ones.
            let (h, rc) = (r, r * 0.12);
            let mut v = Vec::new();
            for (cx, cy, a0) in [(h - rc, h - rc, 0.0), (-h + rc, h - rc, 0.5 * PI), (-h + rc, -h + rc, PI), (h - rc, -h + rc, 1.5 * PI)] {
                for k in 0..=6 {
                    let a = a0 + k as f32 * PI / 12.0;
                    v.push((cx + rc * a.cos(), cy + rc * a.sin()));
                }
            }
            v
        }
        "Heart" => (0..96)
            .map(|k| {
                let t = k as f32 * TAU / 96.0;
                let x = 16.0 * t.sin().powi(3);
                let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                (x * r / 17.0, -y * r / 17.0 - r * 0.1)
            })
            .collect(),
        "Star" => (0..10)
            .map(|k| {
                let a = -PI / 2.0 + k as f32 * PI / 5.0;
                let rr = if k % 2 == 0 { r } else { r * 0.42 };
                (rr * a.cos(), rr * a.sin())
            })
            .collect(),
        "Shield" => {
            let mut v = vec![(-r * 0.85, -r), (r * 0.85, -r)];
            for k in 0..=16 {
                let t = k as f32 / 16.0;
                // Straight sides that curve in to a point at the bottom.
                let y = -r * 0.2 + t * r * 1.2;
                let x = r * 0.85 * (1.0 - t.powf(1.8));
                v.push((x, y));
            }
            for k in (0..16).rev() {
                let t = k as f32 / 16.0;
                v.push((-r * 0.85 * (1.0 - t.powf(1.8)), -r * 0.2 + t * r * 1.2));
            }
            v
        }
        "Banner" => {
            let (h, notch) = (r * 0.3, r * 0.15);
            let mut v = Vec::new();
            for k in 0..=24 {
                let x = -r + k as f32 * r / 12.0;
                v.push((x, -h + (x / r * PI).cos() * -r * 0.08));
            }
            v.push((r - notch, 0.0));
            for k in (0..=24).rev() {
                let x = -r + k as f32 * r / 12.0;
                v.push((x, h + (x / r * PI).cos() * -r * 0.08));
            }
            v.push((-r + notch, 0.0));
            v
        }
        _ => {
            kind = Kind::Stroke { width: size / 40.0 };
            vec![(-r, 0.0), (r, 0.0)]
        }
    };
    let closed = !matches!(kind, Kind::Stroke { .. });
    Shape { kind, lines: vec![(pts.into_iter().map(|p| (p.0 + at.0, p.1 + at.1)).collect(), closed)], layer }
}

/// A shape drawn with a shape tool, kept as the numbers it is made of.
/// Sizes are user units across; the closed shapes are centered on 0, 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Made {
    Rect {
        w: f32,
        h: f32,
        round: f32,
    },
    Ellipse {
        w: f32,
        h: f32,
    },
    Polygon {
        w: f32,
        h: f32,
        sides: u32,
    },
    Star {
        w: f32,
        h: f32,
        points: u32,
        inner: f32,
    },
    /// A line between two points, bowed sideways by `bend` times its
    /// length (negative bends the other way). An end drawn onto a shape
    /// stays attached to it as that shape moves.
    Line {
        a: Pt,
        b: Pt,
        bend: f32,
        from: Option<Anchor>,
        to: Option<Anchor>,
    },
}

/// Where a line end is attached: a shape (by `ShapeEdit::id`) and the point
/// in that shape's own coordinates, before its move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub shape: u32,
    pub at: Pt,
}

impl Made {
    pub fn closed(&self) -> bool {
        !matches!(self, Made::Line { .. })
    }
}

/// The outline of a made shape.
pub fn made_lines(m: &Made) -> Vec<(Contour, bool)> {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let pts: Vec<Pt> = match *m {
        Made::Rect { w, h, round } => {
            let (hw, hh) = (w / 2.0, h / 2.0);
            let r = round.clamp(0.0, 0.5) * w.min(h);
            if r < 0.01 {
                vec![(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
            } else {
                let mut v = Vec::new();
                for (cx, cy, a0) in [(hw - r, hh - r, 0.0), (-hw + r, hh - r, FRAC_PI_2), (-hw + r, -hh + r, PI), (hw - r, -hh + r, 1.5 * PI)] {
                    for k in 0..=8 {
                        let a = a0 + k as f32 * FRAC_PI_2 / 8.0;
                        v.push((cx + r * a.cos(), cy + r * a.sin()));
                    }
                }
                v
            }
        }
        Made::Ellipse { w, h } => (0..96).map(|k| (k as f32 * TAU / 96.0).sin_cos()).map(|(s, c)| (c * w / 2.0, s * h / 2.0)).collect(),
        Made::Polygon { w, h, sides } => {
            let n = sides.max(3);
            (0..n).map(|k| (-FRAC_PI_2 + k as f32 * TAU / n as f32).sin_cos()).map(|(s, c)| (c * w / 2.0, s * h / 2.0)).collect()
        }
        Made::Star { w, h, points, inner } => {
            let n = points.max(3) * 2;
            (0..n)
                .map(|k| {
                    let r = if k % 2 == 0 { 1.0 } else { inner.clamp(0.05, 0.95) };
                    let (s, c) = (-FRAC_PI_2 + k as f32 * TAU / n as f32).sin_cos();
                    (c * r * w / 2.0, s * r * h / 2.0)
                })
                .collect()
        }
        Made::Line { a, b, bend, .. } => {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            // A quadratic curve through the bowed middle.
            let ctrl = ((a.0 + b.0) / 2.0 - dy * bend * 2.0, (a.1 + b.1) / 2.0 + dx * bend * 2.0);
            let steps = if bend == 0.0 { 1 } else { 24 };
            (0..=steps)
                .map(|k| {
                    let t = k as f32 / steps as f32;
                    let u = 1.0 - t;
                    (u * u * a.0 + 2.0 * u * t * ctrl.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * ctrl.1 + t * t * b.1)
                })
                .collect()
        }
    };
    vec![(pts, m.closed())]
}

/// An empty artwork to build from the built-in shapes: a 100 mm frame and a
/// few starting colors.
pub fn blank() -> Artwork {
    let colors = [[0xd0, 0x20, 0x30], [0x1d, 0x2a, 0x5c], [0xe8, 0xb0, 0x20], [0x20, 0x20, 0x20], [0xf4, 0xf4, 0xf0], [0x2a, 0x8a, 0x3e]];
    Artwork {
        name: t!("Shapes").into(),
        shapes: Vec::new(),
        layers: colors.iter().map(|&color| Layer { color, background: false, shapes: 0 }).collect(),
        bounds: (0.0, 0.0, 377.95, 377.95),
        skipped_images: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(m: &Made) -> (f32, f32) {
        let b = extent(&made_lines(m));
        (b.2 - b.0, b.3 - b.1)
    }

    #[test]
    fn made_shapes_fill_their_size() {
        for m in [Made::Rect { w: 40.0, h: 20.0, round: 0.0 }, Made::Rect { w: 40.0, h: 20.0, round: 0.25 }, Made::Ellipse { w: 40.0, h: 20.0 }] {
            let (w, h) = size(&m);
            assert!((w - 40.0).abs() < 0.1 && (h - 20.0).abs() < 0.1, "{m:?}: {w} × {h}");
        }
        assert_eq!(made_lines(&Made::Polygon { w: 10.0, h: 10.0, sides: 6 })[0].0.len(), 6);
        assert_eq!(made_lines(&Made::Star { w: 10.0, h: 10.0, points: 5, inner: 0.4 })[0].0.len(), 10);
    }

    #[test]
    fn lines_bend_sideways_and_keep_their_ends() {
        let straight = made_lines(&Made::Line { a: (0.0, 0.0), b: (100.0, 0.0), bend: 0.0, from: None, to: None });
        assert_eq!(straight[0].0, vec![(0.0, 0.0), (100.0, 0.0)]);
        assert!(!straight[0].1);
        let bent = &made_lines(&Made::Line { a: (0.0, 0.0), b: (100.0, 0.0), bend: 0.25, from: None, to: None })[0].0;
        assert_eq!((bent[0], *bent.last().unwrap()), ((0.0, 0.0), (100.0, 0.0)));
        // Bowed a quarter of its length at the middle.
        assert!((bent[12].1 - 25.0).abs() < 0.5, "{:?}", bent[12]);
    }

    #[test]
    fn remake_keeps_the_middle_and_hand_edits_drop_the_numbers() {
        let art = blank();
        let m = Made::Rect { w: 40.0, h: 20.0, round: 0.0 };
        let mut lines = made_lines(&m);
        for p in lines[0].0.iter_mut() {
            *p = (p.0 + 100.0, p.1 + 50.0);
        }
        let mut e = ShapeEdit::of(&Artwork { shapes: vec![Shape { kind: Kind::Fill { even_odd: false }, lines: lines.clone(), layer: 0 }], ..art.clone() }, 0);
        e.own = Some(Shape { kind: Kind::Fill { even_odd: false }, lines, layer: 0 });
        e.made = Some(Made::Ellipse { w: 60.0, h: 30.0 });
        e.remake();
        let b = extent(&e.own.as_ref().unwrap().lines);
        assert!(((b.0 + b.2) / 2.0 - 100.0).abs() < 0.1 && ((b.1 + b.3) / 2.0 - 50.0).abs() < 0.1);
        assert!((b.2 - b.0 - 60.0).abs() < 0.1);
        e.bake(&art);
        assert_eq!(e.made, None);
    }
}
