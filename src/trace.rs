//! Turns a picture into artwork: the colors are reduced to a few with
//! k-means, each color's areas are outlined with marching squares, the
//! outlines are smoothed, and holes are matched to the shapes they sit in.

use std::path::Path;

use crate::artwork::{Artwork, Kind, Layer, Shape};
use crate::lettering::{self, Contour, Pt};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceSettings {
    pub colors: usize,
    /// Rounds of corner smoothing, 0 to 3.
    pub smooth: u8,
    /// Leave out the color that fills the picture's border.
    pub drop_background: bool,
    /// Passes of a majority filter that soaks up specks, 0 to 3.
    pub clean: u8,
}

impl Default for TraceSettings {
    fn default() -> Self {
        TraceSettings { colors: 4, smooth: 2, drop_background: true, clean: 1 }
    }
}

pub struct Picture {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 4]>,
}

/// Longest side after loading; more detail than this doesn't stitch anyway.
const MAX_SIDE: u32 = 700;

pub fn load(path: &Path) -> Result<Picture, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let img = image::open(path).map_err(|e| t!("Couldn't open {name}: {e}.", name = name, e = e))?;
    let mut rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w.max(h) > MAX_SIDE {
        let k = MAX_SIDE as f32 / w.max(h) as f32;
        rgba = image::imageops::resize(&rgba, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), image::imageops::FilterType::Triangle);
    }
    let (w, h) = rgba.dimensions();
    Ok(Picture { name, w: w as usize, h: h as usize, px: rgba.pixels().map(|p| p.0).collect() })
}

fn d2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// k-means on the opaque pixels, started from colors far apart so small
/// but distinct logo colors get their own cluster.
fn palette(px: &[[u8; 4]], k: usize) -> Vec<[f32; 3]> {
    let opaque: Vec<[f32; 3]> = px.iter().filter(|p| p[3] >= 128).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    if opaque.is_empty() {
        return Vec::new();
    }
    let step = (opaque.len() / 20_000).max(1);
    let sample: Vec<[f32; 3]> = opaque.iter().step_by(step).copied().collect();
    let mut centers = vec![sample[0]];
    while centers.len() < k {
        let far = sample.iter().copied().max_by(|a, b| {
            let da = centers.iter().map(|c| d2(*a, *c)).fold(f32::MAX, f32::min);
            let db = centers.iter().map(|c| d2(*b, *c)).fold(f32::MAX, f32::min);
            da.total_cmp(&db)
        });
        match far {
            Some(f) if centers.iter().all(|c| d2(f, *c) > 1.0) => centers.push(f),
            _ => break,
        }
    }
    for _ in 0..12 {
        let mut sum = vec![[0.0f32; 4]; centers.len()];
        for p in &sample {
            let i = nearest(&centers, *p);
            for c in 0..3 {
                sum[i][c] += p[c];
            }
            sum[i][3] += 1.0;
        }
        for (c, s) in centers.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                *c = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            }
        }
    }
    centers
}

fn nearest(centers: &[[f32; 3]], p: [f32; 3]) -> usize {
    (0..centers.len()).min_by(|&a, &b| d2(centers[a], p).total_cmp(&d2(centers[b], p))).unwrap_or(0)
}

fn signed_area(c: &Contour) -> f32 {
    let mut a = 0.0;
    for i in 0..c.len() {
        let (p, q) = (c[i], c[(i + 1) % c.len()]);
        a += p.0 * q.1 - q.0 * p.1;
    }
    a / 2.0
}

fn inside(c: &Contour, p: Pt) -> bool {
    let mut odd = false;
    for i in 0..c.len() {
        let (a, b) = (c[i], c[(i + 1) % c.len()]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            odd = !odd;
        }
    }
    odd
}

/// Cuts each corner of a closed line, rounding the pixel stairs.
fn chaikin(c: &Contour) -> Contour {
    let n = c.len();
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (c[i], c[(i + 1) % n]);
        out.push((0.75 * a.0 + 0.25 * b.0, 0.75 * a.1 + 0.25 * b.1));
        out.push((0.25 * a.0 + 0.75 * b.0, 0.25 * a.1 + 0.75 * b.1));
    }
    out
}

/// Drops points that lie within `eps` of a straight line (Ramer-Douglas-Peucker).
pub fn simplify(c: &[Pt], eps: f32) -> Vec<Pt> {
    if c.len() < 3 {
        return c.to_vec();
    }
    let (a, b) = (c[0], c[c.len() - 1]);
    let len = lettering::dist(a, b).max(1e-6);
    let (mut far, mut at) = (0.0, 0);
    for (i, p) in c.iter().enumerate().take(c.len() - 1).skip(1) {
        let d = ((b.0 - a.0) * (a.1 - p.1) - (a.0 - p.0) * (b.1 - a.1)).abs() / len;
        if d > far {
            far = d;
            at = i;
        }
    }
    if far <= eps {
        return vec![a, b];
    }
    let mut left = simplify(&c[..=at], eps);
    left.pop();
    left.extend(simplify(&c[at..], eps));
    left
}

pub fn simplify_closed(c: &Contour, eps: f32) -> Contour {
    // Split at the point farthest from the start so both halves are real lines.
    let far = (0..c.len()).max_by(|&a, &b| lettering::dist(c[0], c[a]).total_cmp(&lettering::dist(c[0], c[b]))).unwrap_or(0);
    if far == 0 {
        return c.clone();
    }
    let mut first = simplify(&c[..=far], eps);
    let mut second: Vec<Pt> = c[far..].to_vec();
    second.push(c[0]);
    let second = simplify(&second, eps);
    first.pop();
    first.extend(second);
    first.pop();
    first
}

/// Each pass gives a pixel the color most of its 3×3 neighbors have, when
/// five or more agree, which removes specks and ragged single pixels.
fn despeckle(mut labels: Vec<u8>, w: usize, h: usize, passes: u8) -> Vec<u8> {
    for _ in 0..passes {
        let src = labels.clone();
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let mut counts: [(u8, u8); 9] = [(0, 0); 9];
                let mut n = 0;
                for dy in 0..3 {
                    for dx in 0..3 {
                        let l = src[(y + dy - 1) * w + x + dx - 1];
                        match counts[..n].iter_mut().find(|c| c.0 == l) {
                            Some(c) => c.1 += 1,
                            None => {
                                counts[n] = (l, 1);
                                n += 1;
                            }
                        }
                    }
                }
                let best = counts[..n].iter().max_by_key(|c| c.1).unwrap();
                if best.1 >= 5 {
                    labels[y * w + x] = best.0;
                }
            }
        }
    }
    labels
}

pub fn trace(pic: &Picture, t: &TraceSettings) -> Result<Artwork, String> {
    let centers = palette(&pic.px, t.colors.clamp(2, 12));
    if centers.is_empty() {
        return Err(t!("{name} is fully transparent, so there's nothing to trace.", name = pic.name));
    }
    let (w, h) = (pic.w, pic.h);
    let labels: Vec<u8> = pic.px.iter().map(|p| if p[3] < 128 { u8::MAX } else { nearest(&centers, [p[0] as f32, p[1] as f32, p[2] as f32]) as u8 }).collect();
    let labels = despeckle(labels, w, h, t.clean);

    // The color covering most of the border is the background.
    let mut border = vec![0usize; centers.len()];
    let mut border_total = 0;
    for x in 0..w {
        for y in [0, h - 1] {
            border_total += 1;
            if let Some(c) = border.get_mut(labels[y * w + x] as usize) {
                *c += 1;
            }
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            border_total += 1;
            if let Some(c) = border.get_mut(labels[y * w + x] as usize) {
                *c += 1;
            }
        }
    }
    let background = (0..centers.len()).max_by_key(|&i| border[i]).filter(|&i| border[i] * 2 > border_total);

    let mut counts = vec![0usize; centers.len()];
    for &l in &labels {
        if let Some(c) = counts.get_mut(l as usize) {
            *c += 1;
        }
    }
    // Biggest areas first, so small details are stitched on top.
    let mut order: Vec<usize> = (0..centers.len()).filter(|&i| counts[i] > 0).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(counts[i]));

    let (fw, fh) = (w + 2, h + 2);
    let center = |i: usize, j: usize| (i as f32 - 0.5, j as f32 - 0.5);
    let mut layers = Vec::new();
    let mut shapes = Vec::new();
    for &k in &order {
        // A padded mask, softened a little so the outline follows the
        // antialiased edge instead of pixel stairs.
        let mut mask = vec![0.0f32; fw * fh];
        for y in 0..h {
            for x in 0..w {
                if labels[y * w + x] as usize == k {
                    mask[(y + 1) * fw + x + 1] = 1.0;
                }
            }
        }
        let mut soft = mask.clone();
        for y in 1..fh - 1 {
            for x in 1..fw - 1 {
                let mut s = 0.0;
                for dy in 0..3 {
                    for dx in 0..3 {
                        s += mask[(y + dy - 1) * fw + x + dx - 1];
                    }
                }
                soft[y * fw + x] = s / 9.0;
            }
        }
        let mut rings: Vec<Contour> = lettering::iso_lines(&soft, fw, fh, 0.5, &center)
            .into_iter()
            .map(|mut r| {
                for _ in 0..t.smooth {
                    r = chaikin(&r);
                }
                simplify_closed(&r, 0.3)
            })
            .filter(|r| r.len() >= 3 && signed_area(r).abs() >= 4.0)
            .collect();
        if rings.is_empty() {
            continue;
        }
        // Outer rings and holes wind opposite ways; the largest ring is outer.
        rings.sort_by(|a, b| signed_area(b).abs().total_cmp(&signed_area(a).abs()));
        let outer_sign = signed_area(&rings[0]).signum();
        let (outers, holes): (Vec<Contour>, Vec<Contour>) = rings.into_iter().partition(|r| signed_area(r).signum() == outer_sign);
        let mut groups: Vec<Vec<(Contour, bool)>> = outers.into_iter().map(|o| vec![(o, true)]).collect();
        for hole in holes {
            // The smallest outer ring around the hole owns it.
            let owner = groups
                .iter()
                .enumerate()
                .filter(|(_, g)| inside(&g[0].0, hole[0]))
                .min_by(|a, b| signed_area(&a.1[0].0).abs().total_cmp(&signed_area(&b.1[0].0).abs()))
                .map(|(i, _)| i);
            if let Some(i) = owner {
                groups[i].push((hole, true));
            }
        }
        let layer = layers.len();
        let c = centers[k];
        layers.push(Layer {
            color: [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8],
            background: t.drop_background && background == Some(k),
            shapes: groups.len(),
        });
        for lines in groups {
            shapes.push(Shape { kind: Kind::Fill { even_odd: true }, lines, layer });
        }
    }
    if shapes.is_empty() {
        return Err(t!("Couldn't find any shapes in {name}.", name = pic.name));
    }
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in shapes.iter().filter(|s| !layers[s.layer].background).flat_map(|s| s.lines.iter().flat_map(|l| &l.0)) {
        b = (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1));
    }
    if b.0 > b.2 {
        b = (0.0, 0.0, w as f32, h as f32);
    }
    Ok(Artwork { name: pic.name.clone(), shapes, layers, bounds: b, skipped_images: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white picture with a red square holding a white hole, and a blue dot.
    fn picture() -> Picture {
        let (w, h) = (60, 40);
        let mut px = vec![[255, 255, 255, 255]; w * h];
        for y in 5..35 {
            for x in 5..35 {
                let hole = (15..25).contains(&x) && (15..25).contains(&y);
                if !hole {
                    px[y * w + x] = [220, 20, 30, 255];
                }
            }
        }
        for y in 15..25 {
            for x in 45..55 {
                px[y * w + x] = [20, 40, 200, 255];
            }
        }
        Picture { name: "test.png".into(), w, h, px }
    }

    #[test]
    fn finds_colors_holes_and_background() {
        let art = trace(&picture(), &TraceSettings { colors: 3, ..Default::default() }).unwrap();
        assert_eq!(art.layers.len(), 3);
        let bg = art.layers.iter().position(|l| l.background).unwrap();
        assert!(art.layers[bg].color[0] > 240 && art.layers[bg].color[2] > 240);
        let red = art.layers.iter().position(|l| l.color[0] > 180 && l.color[2] < 80).unwrap();
        let red_shape = art.shapes.iter().find(|s| s.layer == red).unwrap();
        assert_eq!(red_shape.lines.len(), 2, "square with its hole");
        // Bounds cover the red square and blue dot, not the background.
        assert!(art.bounds.0 > 3.0 && art.bounds.2 < 57.0);
    }

    #[test]
    fn traced_art_stitches_with_two_threads() {
        let art = trace(&picture(), &TraceSettings { colors: 3, ..Default::default() }).unwrap();
        let s = crate::artwork::ArtSettings::for_artwork(&art);
        let p = crate::artwork::generate(&art, &s).unwrap();
        assert_eq!(p.blocks().len(), 2);
    }
}

#[cfg(test)]
mod samples {
    use super::*;

    /// Traces the pictures in HOOP_PICS (colon separated) to JEF in HOOP_OUT.
    #[test]
    #[ignore]
    fn picture_samples() {
        let (Some(out), Some(list)) = (std::env::var_os("HOOP_OUT"), std::env::var("HOOP_PICS").ok()) else { return };
        for path in list.split(':') {
            let t0 = std::time::Instant::now();
            let pic = load(Path::new(path)).unwrap();
            let art = trace(&pic, &TraceSettings::default()).unwrap();
            let mut s = crate::artwork::ArtSettings::for_artwork(&art);
            s.width_mm = 90.0;
            let p = crate::artwork::generate(&art, &s).unwrap();
            eprintln!(
                "{path}: {}x{} px, {} colors, {} shapes, {} stitches, {:?}",
                pic.w,
                pic.h,
                art.layers.len(),
                art.shapes.len(),
                p.count(crate::formats::Cmd::Stitch),
                t0.elapsed()
            );
            let threads: Vec<i32> = p.threads.iter().map(|t| crate::formats::nearest_jef(t.color)).collect();
            let b = p.bounds();
            let pts: Vec<Pt> = p.points.iter().map(|&(x, y)| (x - (b.0 + b.2) / 2.0, y - (b.1 + b.3) / 2.0)).collect();
            let stem = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
            std::fs::write(Path::new(&out).join(format!("pic_{stem}.jef")), crate::formats::write_jef(&pts, &p.cmds, &threads)).unwrap();
        }
    }
}
