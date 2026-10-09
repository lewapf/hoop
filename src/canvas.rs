//! Draws the fabric, the hoop and the stitches. World units are 0.1 mm with the
//! hoop center at the origin; `View` maps them to screen points.

use egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, TextureHandle, Vec2, vec2};

use crate::formats::{Block, Cmd, Pattern};
use crate::hoops::Placement;

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub scale: f32,
    pub pan: Vec2,
}

impl Default for View {
    fn default() -> Self {
        View { scale: 1.0, pan: Vec2::ZERO }
    }
}

/// Space between the sewing area and the inner edge of the hoop frame, and the
/// frame width, in 0.1 mm. Roughly the proportions of the MC9900 hoops.
pub const OPENING_MARGIN: f32 = 90.0;
pub const RING: f32 = 150.0;

pub struct Frame<'a> {
    pub painter: &'a Painter,
    pub rect: Rect,
    pub view: View,
}

impl Frame<'_> {
    pub fn center(&self) -> Pos2 {
        self.rect.center() + self.view.pan
    }

    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        self.center() + vec2(x, y) * self.view.scale
    }

    pub fn to_screen_pt(&self, (x, y): (f32, f32)) -> Pos2 {
        self.to_screen(x, y)
    }

    pub fn to_world(&self, p: Pos2) -> (f32, f32) {
        let v = (p - self.center()) / self.view.scale;
        (v.x, v.y)
    }
}

pub fn luminance(c: [u8; 3]) -> f32 {
    (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0
}

fn mix(c: [u8; 3], toward: u8, amount: f32) -> Color32 {
    let f = |v: u8| (v as f32 + (toward as f32 - v as f32) * amount).round() as u8;
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

/// A tiny plain-weave tile, tinted by the fabric color when drawn.
pub fn weave_image() -> egui::ColorImage {
    let mut img = egui::ColorImage::new([6, 6], vec![Color32::WHITE; 36]);
    for y in 0..6 {
        for x in 0..6 {
            let v: u8 = if ((x / 2) + (y / 2)) % 2 == 1 { 255 } else { 222 };
            img.pixels[y * 6 + x] = Color32::from_gray(v);
        }
    }
    img
}

pub fn draw_fabric(f: &Frame, fabric: [u8; 3], weave: &TextureHandle) {
    let mut mesh = Mesh::with_texture(weave.id());
    let tiles = f.rect.size() / 6.0;
    mesh.add_rect_with_uv(f.rect, Rect::from_min_size(Pos2::ZERO, tiles), Color32::from_rgb(fabric[0], fabric[1], fabric[2]));
    f.painter.add(Shape::mesh(mesh));
}

fn rounded_rect_path(f: &Frame, half_w: f32, half_h: f32, radius: f32) -> Vec<Pos2> {
    let r = radius.min(half_w).min(half_h);
    let corners = [
        (half_w - r, -half_h + r, -90.0f32),
        (half_w - r, half_h - r, 0.0),
        (-half_w + r, half_h - r, 90.0),
        (-half_w + r, -half_h + r, 180.0),
    ];
    let mut pts = Vec::with_capacity(4 * 9);
    for (cx, cy, start) in corners {
        for i in 0..=8 {
            let a = (start + i as f32 * 90.0 / 8.0).to_radians();
            pts.push(f.to_screen(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts
}

/// Draws the hoop frame around a sewing area of `w` x `h` (0.1 mm).
pub fn draw_hoop(f: &Frame, w: f32, h: f32, label: &str, fabric: [u8; 3], grid: bool) {
    let s = f.view.scale;
    let (iw, ih) = (w / 2.0 + OPENING_MARGIN, h / 2.0 + OPENING_MARGIN);
    let radius = (iw.min(ih) / 2.0).min(150.0);
    let mid = |off: f32| rounded_rect_path(f, iw + off, ih + off, radius + off);

    let plastic = Color32::from_rgb(0xee, 0xef, 0xeb);
    let edge = Color32::from_black_alpha(70);

    // Mounting bracket on the left, where the hoop clips onto the carriage.
    let bh = (ih * 2.0 + RING * 2.0) * 0.45;
    let bh = bh.clamp(300.0, 900.0);
    let bracket = Rect::from_min_max(
        f.to_screen(-iw - RING - 200.0, -bh / 2.0),
        f.to_screen(-iw - RING + 40.0, bh / 2.0),
    );
    let corner = (50.0 * s).min(120.0) as u8;
    f.painter.rect_filled(bracket.translate(vec2(0.0, 4.0)), corner, Color32::from_black_alpha(60));
    f.painter.rect(bracket, corner, plastic, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    for dy in [-bh / 4.0, bh / 4.0] {
        f.painter.circle_filled(f.to_screen(-iw - RING - 90.0, dy), 32.0 * s, Color32::from_rgb(0x6d, 0x6f, 0x69));
    }

    // The ring is one thick closed stroke along its centerline, with a soft
    // shadow under it and thin lines for the outer edge, groove and inner lip.
    let center_line = mid(RING / 2.0);
    let shadow: Vec<Pos2> = center_line.iter().map(|p| *p + vec2(0.0, 5.0)).collect();
    f.painter.add(Shape::closed_line(shadow, Stroke::new(RING * s + 6.0, Color32::from_black_alpha(55))));
    f.painter.add(Shape::closed_line(center_line, Stroke::new(RING * s, plastic)));
    f.painter.add(Shape::closed_line(mid(RING), Stroke::new(1.0, edge)));
    f.painter.add(Shape::closed_line(mid(RING * 0.45), Stroke::new(1.0, Color32::from_black_alpha(30))));
    f.painter.add(Shape::closed_line(mid(0.0), Stroke::new(2.0, Color32::from_black_alpha(55))));
    let knob = f.to_screen(iw + RING * 0.5, ih + RING * 0.5);
    f.painter.circle(knob, 55.0 * s, Color32::from_rgb(0xb9, 0xbb, 0xb4), Stroke::new(1.0, edge));

    f.painter.text(
        f.to_screen(0.0, -ih - RING / 2.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional((RING * 0.4 * s).clamp(9.0, 22.0)),
        Color32::from_rgba_unmultiplied(40, 40, 40, 170),
    );

    // Sewing area: the 10 mm grid of Janome's clear template, and the outline.
    let ink: u8 = if luminance(fabric) < 0.45 { 255 } else { 20 };
    let line = |a: u8| Color32::from_rgba_unmultiplied(ink, ink, ink, a);
    let (hw, hh) = (w / 2.0, h / 2.0);
    if grid {
        let mut x = 100.0;
        while x < hw {
            for sx in [x, -x] {
                f.painter.line_segment([f.to_screen(sx, -hh), f.to_screen(sx, hh)], Stroke::new(1.0, line(40)));
            }
            x += 100.0;
        }
        let mut y = 100.0;
        while y < hh {
            for sy in [y, -y] {
                f.painter.line_segment([f.to_screen(-hw, sy), f.to_screen(hw, sy)], Stroke::new(1.0, line(40)));
            }
            y += 100.0;
        }
        f.painter.line_segment([f.to_screen(0.0, -hh), f.to_screen(0.0, hh)], Stroke::new(1.5, line(100)));
        f.painter.line_segment([f.to_screen(-hw, 0.0), f.to_screen(hw, 0.0)], Stroke::new(1.5, line(100)));
    }
    let outline = [
        f.to_screen(-hw, -hh), f.to_screen(hw, -hh), f.to_screen(hw, hh), f.to_screen(-hw, hh), f.to_screen(-hw, -hh),
    ];
    f.painter.extend(Shape::dashed_line(&outline, Stroke::new(1.5, line(140)), 6.0, 5.0));
}

/// Maps design coordinates to world coordinates for a placement.
#[derive(Clone, Copy)]
pub struct Transform {
    cx: f32,
    cy: f32,
    cos: f32,
    sin: f32,
    flip: f32,
    dx: f32,
    dy: f32,
}

impl Transform {
    pub fn new(bounds: (f32, f32, f32, f32), at: &Placement) -> Self {
        let a = at.quarter_turns as f32 * std::f32::consts::FRAC_PI_2;
        Transform {
            cx: (bounds.0 + bounds.2) / 2.0,
            cy: (bounds.1 + bounds.3) / 2.0,
            cos: a.cos().round(),
            sin: a.sin().round(),
            flip: if at.mirror { -1.0 } else { 1.0 },
            dx: at.dx,
            dy: at.dy,
        }
    }

    pub fn apply(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let (x, y) = ((x - self.cx) * self.flip, y - self.cy);
        (x * self.cos - y * self.sin + self.dx, x * self.sin + y * self.cos + self.dy)
    }

    /// Hoop coordinates back to design coordinates.
    pub fn invert(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let (x, y) = (x - self.dx, y - self.dy);
        let (x, y) = (x * self.cos + y * self.sin, -x * self.sin + y * self.cos);
        (x * self.flip + self.cx, y + self.cy)
    }

    /// A move in hoop coordinates as a move in design coordinates.
    pub fn invert_delta(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let (x, y) = (x * self.cos + y * self.sin, -x * self.sin + y * self.cos);
        (x * self.flip, y)
    }
}

/// Adds one stitch as a quad plus a round end cap, so dense fills read as
/// thread rather than as hairlines.
fn add_segment(mesh: &mut Mesh, a: Pos2, b: Pos2, width: f32, color: Color32) {
    let d = b - a;
    let len = d.length();
    let r = width / 2.0;
    if len > 0.01 {
        let n = vec2(-d.y, d.x) / len * r;
        let i = mesh.vertices.len() as u32;
        for p in [a + n, a - n, b - n, b + n] {
            mesh.colored_vertex(p, color);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    if r >= 1.0 {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(b, color);
        for k in 0..8 {
            let t = k as f32 * std::f32::consts::TAU / 8.0;
            mesh.colored_vertex(b + vec2(t.cos(), t.sin()) * r, color);
        }
        for k in 0..8 {
            mesh.add_triangle(i, i + 1 + k, i + 1 + (k + 1) % 8);
        }
    }
}

pub struct DesignStyle {
    pub textured: bool,
    pub show_jumps: bool,
    pub focus: Option<usize>,
    pub fabric: [u8; 3],
}

/// Draws the stitches up to (not including) record `upto`.
pub fn draw_design(f: &Frame, pattern: &Pattern, blocks: &[Block], t: &Transform, upto: usize, style: &DesignStyle) {
    let screen: Vec<Pos2> = pattern.points.iter().map(|&p| {
        let (x, y) = t.apply(p);
        f.to_screen(x, y)
    }).collect();
    // Thread is about 0.4 mm wide; keep it visible when zoomed far out.
    let width = (4.0 * f.view.scale).max(1.2);

    for (k, block) in blocks.iter().enumerate() {
        if block.start >= upto {
            break;
        }
        let dim = matches!(style.focus, Some(fk) if fk != k);
        let alpha = if dim { 0.15 } else { 1.0 };
        let c = block.thread.color;
        let passes: Vec<(f32, Color32, Vec2)> = if style.textured {
            vec![
                (width * 1.15, mix(c, 0, 0.45), Vec2::ZERO),
                (width * 0.8, Color32::from_rgb(c[0], c[1], c[2]), Vec2::ZERO),
                (width * 0.22, mix(c, 255, 0.5).gamma_multiply(0.55), vec2(-0.12, -0.12) * width),
            ]
        } else {
            vec![(width, Color32::from_rgb(c[0], c[1], c[2]), Vec2::ZERO)]
        };
        let end = block.end.min(upto);
        for (w, color, shift) in passes {
            let mut mesh = Mesh::default();
            for i in block.start.max(1)..end {
                if pattern.cmds[i] == Cmd::Stitch {
                    add_segment(&mut mesh, screen[i - 1] + shift, screen[i] + shift, w, color.gamma_multiply(alpha));
                }
            }
            f.painter.add(Shape::mesh(mesh));
        }
    }

    if style.show_jumps {
        let ink = if luminance(style.fabric) < 0.45 { Color32::from_white_alpha(190) } else { Color32::from_black_alpha(150) };
        for i in 1..upto.min(pattern.cmds.len()) {
            if pattern.cmds[i] == Cmd::Jump {
                f.painter.extend(Shape::dashed_line(&[screen[i - 1], screen[i]], Stroke::new(1.2, ink), 4.0, 3.0));
            }
        }
    }

    if upto > 0 && upto < pattern.points.len() {
        let p = screen[upto - 1];
        let needle = Stroke::new(2.0, Color32::from_rgb(0xff, 0x2d, 0x7a));
        f.painter.circle_stroke(p, 7.0, needle);
        f.painter.line_segment([p - vec2(13.0, 0.0), p + vec2(13.0, 0.0)], needle);
        f.painter.line_segment([p - vec2(0.0, 13.0), p + vec2(0.0, 13.0)], needle);
    }
}

pub fn draw_bounds(f: &Frame, size: (f32, f32), at: &Placement, color: Color32) {
    let (hw, hh) = (size.0 / 2.0, size.1 / 2.0);
    let pts = [
        f.to_screen(at.dx - hw, at.dy - hh), f.to_screen(at.dx + hw, at.dy - hh),
        f.to_screen(at.dx + hw, at.dy + hh), f.to_screen(at.dx - hw, at.dy + hh),
        f.to_screen(at.dx - hw, at.dy - hh),
    ];
    f.painter.extend(Shape::dashed_line(&pts, Stroke::new(1.5, color), 5.0, 4.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turn_and_mirror_move_points_around_center() {
        let bounds = (0.0, 0.0, 100.0, 50.0);
        let turn = Transform::new(bounds, &Placement { quarter_turns: 1, ..Default::default() });
        assert_eq!(turn.apply((100.0, 25.0)), (0.0, 50.0));
        let flip = Transform::new(bounds, &Placement { mirror: true, dx: 10.0, ..Default::default() });
        assert_eq!(flip.apply((100.0, 25.0)), (-40.0, 0.0));
        for t in [&turn, &flip] {
            let p = t.apply((37.0, -12.0));
            let back = t.invert(p);
            assert!((back.0 - 37.0).abs() < 1e-4 && (back.1 + 12.0).abs() < 1e-4);
        }
    }
}
