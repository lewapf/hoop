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
