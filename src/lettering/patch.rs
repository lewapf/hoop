//! Patch backgrounds round the text: box, rounded, oval, circle and the
//! key-fob tag with its eyelet.

use super::*;

/// The patch shape around the laid-out text, if one is chosen.
pub(crate) fn patch_outline(glyphs: &[Vec<Contour>], s: &Settings) -> Option<Contour> {
    if s.patch == PatchShape::None {
        return None;
    }
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let pad = s.patch_pad_mm * 10.0;
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let (hw, hh) = ((b.2 - b.0) / 2.0, (b.3 - b.1) / 2.0);
    let ellipse = |a: f32, bb: f32| -> Contour {
        (0..96)
            .map(|i| {
                let t = i as f32 / 96.0 * std::f32::consts::TAU;
                (cx + a * t.cos(), cy + bb * t.sin())
            })
            .collect()
    };
    Some(match s.patch {
        PatchShape::None => return None,
        PatchShape::Rect | PatchShape::Rounded => {
            let (w, h) = (hw + pad, hh + pad);
            let r = if s.patch == PatchShape::Rounded { (pad * 1.6).min(w).min(h) } else { 0.0 };
            let mut c = Vec::new();
            for (qx, qy, start) in [(w - r, -h + r, -90.0f32), (w - r, h - r, 0.0), (-w + r, h - r, 90.0), (-w + r, -h + r, 180.0)] {
                for i in 0..=12 {
                    let t = (start + i as f32 * 7.5).to_radians();
                    c.push((cx + qx + r * t.cos(), cy + qy + r * t.sin()));
                }
            }
            c.dedup_by(|a, b| dist(*a, *b) < 0.5);
            c
        }
        // An ellipse with the text box's proportions that passes outside its corners.
        PatchShape::Oval => ellipse(hw * std::f32::consts::SQRT_2 + pad, hh * std::f32::consts::SQRT_2 + pad),
        PatchShape::Circle => {
            let r = (hw * hw + hh * hh).sqrt() + pad;
            ellipse(r, r)
        }
        PatchShape::Fob => {
            // A pill round the text with a tab to the left for the ring.
            let h = hh + pad;
            let (left, right) = (cx - hw - pad - 2.0 * h, cx + hw + pad);
            let mut c = Vec::new();
            for i in 0..=24 {
                let t = (-90.0 + i as f32 * 7.5f32).to_radians();
                c.push((right - h + h * t.cos(), cy + h * t.sin()));
            }
            for i in 0..=24 {
                let t = (90.0 + i as f32 * 7.5f32).to_radians();
                c.push((left + h + h * t.cos(), cy + h * t.sin()));
            }
            c
        }
    })
}

/// The key-ring hole of a fob: a circle in the middle of its tab.
pub(crate) fn fob_eyelet(glyphs: &[Vec<Contour>], s: &Settings) -> Option<Contour> {
    if s.patch != PatchShape::Fob {
        return None;
    }
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let pad = s.patch_pad_mm * 10.0;
    let h = (b.3 - b.1) / 2.0 + pad;
    let (cx, cy) = (b.0 - pad - h, (b.1 + b.3) / 2.0);
    let r = (h * 0.45).clamp(25.0, 45.0);
    Some(
        (0..48)
            .map(|i| {
                let t = i as f32 / 48.0 * std::f32::consts::TAU;
                (cx + r * t.cos(), cy + r * t.sin())
            })
            .collect(),
    )
}
