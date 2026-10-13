//! Laying text out: glyph placement, kerning, alignment, and bending the
//! result onto an arc, a circle badge or an envelope.

use super::*;
use ttf_parser::OutlineBuilder;

/// Lays the text out and returns the contours of each glyph in 0.1 mm, centered.
/// Puts the first line round the top of a circle and the second round the
/// bottom, both reading left to right with letters upright; further lines
/// stay straight in the middle.
pub(crate) fn circle_badge(glyphs: &mut [Vec<Contour>], line_of: &[usize], s: &Settings, pitch: f32) {
    let cap = s.height_mm * 10.0;
    let r_in = (s.circle_mm * 10.0 / 2.0).max(cap);
    let r_out = r_in + cap;
    let middle: Vec<usize> = line_of.iter().copied().filter(|&l| l >= 2).collect();
    let (first_mid, last_mid) = (middle.iter().copied().min().unwrap_or(2), middle.iter().copied().max().unwrap_or(2));
    // Middle lines are stacked around the center: baseline of the middle of them at cap/2.
    let mid_shift = -((first_mid + last_mid) as f32 / 2.0) * pitch + cap / 2.0;
    for (g, &line) in glyphs.iter_mut().zip(line_of) {
        let base = line as f32 * pitch;
        for c in g.iter_mut() {
            if line < 2 {
                *c = densify(c, 8.0);
            }
            for p in c.iter_mut() {
                let rel = p.1 - base;
                *p = match line {
                    0 => {
                        let rho = r_in - rel;
                        let th = p.0 / r_in;
                        (rho * th.sin(), -rho * th.cos())
                    }
                    1 => {
                        let rho = r_out + rel;
                        let th = -p.0 / r_out;
                        (-rho * th.sin(), rho * th.cos())
                    }
                    _ => (p.0, p.1 + mid_shift),
                };
            }
        }
    }
}

/// Bends the text's box: `amount` is how far the edges move, in 0.1 mm.
pub(crate) fn envelope(glyphs: &mut [Vec<Contour>], shape: TextShape, amount: f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    let (w, h) = ((b.2 - b.0).max(1.0), (b.3 - b.1).max(1.0));
    let cy = (b.1 + b.3) / 2.0;
    for g in glyphs.iter_mut() {
        for c in g.iter_mut() {
            *c = densify(c, 8.0);
            for p in c.iter_mut() {
                let u = (p.0 - b.0) / w;
                let v = (p.1 - b.1) / h;
                let bow = 4.0 * u * (1.0 - u);
                p.1 = match shape {
                    TextShape::Arch => p.1 - amount * bow * (1.0 - v),
                    TextShape::Valley => p.1 + amount * bow * v,
                    TextShape::Bulge => p.1 + amount * bow * (2.0 * v - 1.0),
                    TextShape::Pinch => p.1 - amount * bow * (2.0 * v - 1.0) * (h / (h + 2.0 * amount)).min(0.9),
                    TextShape::Bridge => p.1 - amount * bow,
                    TextShape::Wave => p.1 + amount * (u * std::f32::consts::TAU).sin(),
                    TextShape::Grow => cy + (p.1 - cy) * (1.0 + (amount / h).min(0.9) * (2.0 * u - 1.0)),
                    _ => p.1,
                };
            }
        }
    }
}

pub(crate) fn layout(face: &ttf_parser::Face, s: &Settings) -> Vec<Vec<Contour>> {
    // The height of H as drawn: some fonts (Courgette) give a wrong
    // capital height in their tables.
    let drawn_h = face.glyph_index('H').and_then(|g| face.glyph_bounding_box(g)).map(|b| b.y_max).filter(|h| *h > 0);
    let cap = drawn_h.or(face.capital_height().filter(|h| *h > 0)).unwrap_or(face.ascender()) as f32;
    let k = s.height_mm * 10.0 / cap;
    let pitch = s.height_mm * 10.0 * s.line_spacing;
    let lines: Vec<&str> = s.text.lines().collect();
    let slant = s.slant_deg.to_radians().tan();
    let kern = |a: ttf_parser::GlyphId, b: ttf_parser::GlyphId| -> f32 {
        let Some(table) = face.tables().kern else { return 0.0 };
        table.subtables.into_iter().filter(|t| t.horizontal && !t.variable).find_map(|t| t.glyphs_kerning(a, b)).unwrap_or(0) as f32
    };
    let cap_h = s.height_mm * 10.0;
    let mut rows: Vec<(f32, Vec<Vec<Contour>>)> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let mut pen = 0.0;
        let mut prev = None;
        let mut line_glyphs = Vec::new();
        let y0 = row as f32 * pitch;
        let count = line.chars().filter(|c| !c.is_whitespace()).count();
        let mut nth = 0;
        for ch in line.chars() {
            let Some(id) = face.glyph_index(ch) else { continue };
            // A monogram's middle initial is half again as tall, centered
            // on the others.
            let big = s.shape == TextShape::Monogram && count % 2 == 1 && !ch.is_whitespace() && nth == count / 2;
            if !ch.is_whitespace() {
                nth += 1;
            }
            let kk = if big { k * 1.5 } else { k };
            let drop = if big { cap_h * 0.25 } else { 0.0 };
            if let Some(p) = prev {
                pen += kern(p, id) * k;
            }
            prev = Some(id);
            let mut o = Outline { contours: Vec::new(), current: Vec::new() };
            face.outline_glyph(id, &mut o);
            o.close();
            let contours: Vec<Contour> =
                o.contours.into_iter().map(|c| c.into_iter().map(|(x, y)| (pen + x * kk + y * kk * slant, y0 + drop - y * kk)).collect()).collect();
            if !contours.is_empty() {
                line_glyphs.push(contours);
            }
            pen += face.glyph_hor_advance(id).unwrap_or(0) as f32 * kk + s.spacing_mm * 10.0;
        }
        rows.push((pen - s.spacing_mm * 10.0, line_glyphs));
    }
    let widest = rows.iter().map(|r| r.0).fold(0.0, f32::max);
    let mut glyphs = Vec::new();
    let mut line_of = Vec::new();
    for (row, (width, mut line_glyphs)) in rows.into_iter().enumerate() {
        // Badge lines are centered on the circle whatever the alignment.
        let align = if s.shape == TextShape::Circle && row < 2 { Align::Center } else { s.align };
        let shift = match align {
            Align::Left => -widest / 2.0,
            Align::Center => -width / 2.0,
            Align::Right => widest / 2.0 - width,
        };
        for p in line_glyphs.iter_mut().flatten().flatten() {
            p.0 += shift;
        }
        line_of.extend(std::iter::repeat_n(row, line_glyphs.len()));
        glyphs.extend(line_glyphs);
    }
    match s.shape {
        TextShape::Circle => circle_badge(&mut glyphs, &line_of, s, pitch),
        sh if sh.bends() => envelope(&mut glyphs, sh, s.envelope_mm * 10.0),
        _ => {}
    }

    let all = || glyphs.iter().flatten().flatten();
    let (mut minx, mut maxx) = (f32::MAX, f32::MIN);
    for p in all() {
        minx = minx.min(p.0);
        maxx = maxx.max(p.0);
    }
    let bend = s.arc_deg.to_radians();
    if s.shape == TextShape::Arc && bend.abs() > 0.01 && maxx > minx {
        // Bend the baseline of the first line onto a circle whose arc length
        // over the text width matches the requested angle.
        let r = (maxx - minx) / bend;
        for g in &mut glyphs {
            for c in g.iter_mut() {
                *c = densify(c, 10.0);
                for p in c.iter_mut() {
                    let phi = p.0 / r;
                    let rr = r - p.1;
                    *p = (rr * phi.sin(), r - rr * phi.cos());
                }
            }
        }
    }

    let (mut b0, mut b1, mut b2, mut b3) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in glyphs.iter().flatten().flatten() {
        b0 = b0.min(p.0);
        b1 = b1.min(p.1);
        b2 = b2.max(p.0);
        b3 = b3.max(p.1);
    }
    let (cx, cy) = ((b0 + b2) / 2.0, (b1 + b3) / 2.0);
    for p in glyphs.iter_mut().flatten().flatten() {
        p.0 -= cx;
        p.1 -= cy;
    }
    glyphs
}
