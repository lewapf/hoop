//! Drawing artwork by hand over a backdrop picture, the way digitizers
//! trace a logo: load the picture faintly behind the hoop, then place
//! points with the pen. A click makes a corner, a drag pulls out a curve;
//! clicking the first point closes a filled shape, Enter or a double click
//! ends an open line.

use super::*;
use crate::artwork::{Kind, Shape, ShapeEdit};

/// A placed pen point and the handle pulling a curve out of it.
type PenPoint = ((f32, f32), Option<(f32, f32)>);

pub(super) struct Backdrop {
    tex: egui::TextureHandle,
    /// Width over height.
    aspect: f32,
    pub opacity: f32,
    pub name: String,
}

#[derive(Default)]
pub(super) struct Pen {
    pub on: bool,
    /// Placed points in artwork user units, each with the handle that
    /// pulls the curve out of it (an offset; none for a corner).
    points: Vec<PenPoint>,
    dragging: bool,
    /// Where the pointer is, for the line to the next point.
    hover: Option<(f32, f32)>,
    /// The backdrop file dialog's answer.
    pick: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
}

/// Cubic Bézier point.
fn bezier(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: f32) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
}

type Pt = (f32, f32);

/// The pen's points as a polyline: straight between corners, curves where
/// a point has a handle (mirrored on its incoming side).
pub(super) fn flatten(points: &[(Pt, Option<Pt>)], closed: bool) -> Vec<Pt> {
    let n = points.len();
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let segs = if closed { n } else { n - 1 };
    out.push(points[0].0);
    for i in 0..segs {
        let (a, ha) = points[i];
        let (b, hb) = points[(i + 1) % n];
        if ha.is_none() && hb.is_none() {
            out.push(b);
            continue;
        }
        let p1 = ha.map_or(a, |h| (a.0 + h.0, a.1 + h.1));
        let p2 = hb.map_or(b, |h| (b.0 - h.0, b.1 - h.1));
        for k in 1..=16 {
            out.push(bezier(a, p1, p2, b, k as f32 / 16.0));
        }
    }
    if closed {
        out.pop();
    }
    out
}

impl HoopApp {
    /// The artwork frame in hoop coordinates, even before anything is
    /// stitched (a blank drawing has no design yet).
    fn draw_map(&self) -> Option<shapes::ShapeMap> {
        if self.mode != Mode::Artwork || self.playing {
            return None;
        }
        if let Some(m) = self.shape_map() {
            return Some(m);
        }
        let (art, s) = (self.artwork.as_ref()?, self.art.as_ref()?);
        let (scale, mid) = artwork::frame(art, s);
        let (w, h) = ((art.bounds.2 - art.bounds.0) * scale, (art.bounds.3 - art.bounds.1) * scale);
        Some(shapes::ShapeMap { scale, mid, t: Transform::new((-w / 2.0, -h / 2.0, w / 2.0, h / 2.0), &self.setup.at) })
    }

    pub(super) fn load_backdrop(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        match image::open(path) {
            Ok(img) => {
                let img = img.thumbnail(2048, 2048).to_rgba8();
                let (w, h) = (img.width(), img.height());
                let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
                let tex = ctx.load_texture("backdrop", color, egui::TextureOptions::LINEAR);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.backdrop = Some(Backdrop { tex, aspect: w as f32 / h.max(1) as f32, opacity: 0.5, name });
            }
            Err(e) => self.error = Some(t!("Couldn't open the picture: {e}.", e = e)),
        }
    }

    /// The backdrop, fitted into the artwork's frame, under everything.
    pub(super) fn draw_backdrop(&self, painter: &egui::Painter, frame: &Frame) {
        let (Some(b), Some(m), Some(art)) = (&self.backdrop, self.draw_map(), &self.artwork) else { return };
        let (bw, bh) = (art.bounds.2 - art.bounds.0, art.bounds.3 - art.bounds.1);
        let (w, h) = if bw / bh.max(1e-3) > b.aspect { (bh * b.aspect, bh) } else { (bw, bw / b.aspect) };
        let c = ((art.bounds.0 + art.bounds.2) / 2.0, (art.bounds.1 + art.bounds.3) / 2.0);
        let corners = [(c.0 - w / 2.0, c.1 - h / 2.0), (c.0 + w / 2.0, c.1 + h / 2.0)];
        let r = egui::Rect::from_two_pos(frame.to_screen_pt(m.to_world(corners[0])), frame.to_screen_pt(m.to_world(corners[1])));
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(b.tex.id(), r, uv, Color32::from_white_alpha((b.opacity * 255.0) as u8));
    }

    /// Pen input. Returns true while the pen is on.
    pub(super) fn pen_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        if !self.pen.on {
            return false;
        }
        let Some(m) = self.draw_map() else {
            self.pen.on = false;
            return false;
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let (esc, enter) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter)));
        if esc {
            if self.pen.points.is_empty() {
                self.pen.on = false;
            }
            self.pen.points.clear();
            return true;
        }
        let pos = response.interact_pointer_pos().or(response.hover_pos());
        let user = pos.map(|p| m.to_user(frame.to_world(p)));
        self.pen.hover = user;
        if enter || response.double_clicked() {
            self.finish_pen(false);
            return true;
        }
        if (response.drag_started() || response.clicked())
            && let (Some(u), Some(p)) = (user, pos)
        {
            // A click on the first point closes the shape.
            let first = self.pen.points.first().map(|f| frame.to_screen_pt(m.to_world(f.0)));
            if self.pen.points.len() >= 3 && first.is_some_and(|f| (f - p).length() <= 9.0) {
                self.finish_pen(true);
                return true;
            }
            self.pen.points.push((u, None));
            self.pen.dragging = response.drag_started();
        }
        if self.pen.dragging {
            if let (Some(u), Some(last)) = (user, self.pen.points.last_mut()) {
                let h = (u.0 - last.0.0, u.1 - last.0.1);
                last.1 = (h.0.hypot(h.1) > 0.5).then_some(h);
            }
            if response.drag_stopped() {
                self.pen.dragging = false;
            }
        }
        true
    }

    /// Turns the pen's points into a shape: filled when closed, a line
    /// otherwise.
    fn finish_pen(&mut self, closed: bool) {
        let points = std::mem::take(&mut self.pen.points);
        if points.len() < 2 || (closed && points.len() < 3) {
            return;
        }
        let line = flatten(&points, closed);
        let layer = self.primary().and_then(|i| self.art.as_ref()?.shapes.get(i)).map_or(0, |e| e.layer);
        let Some(scale) = self.draw_map().map(|m| m.scale) else { return };
        let (Some(_), Some(s)) = (&self.artwork, &mut self.art) else { return };
        let kind = if closed { Kind::Fill { even_odd: false } } else { Kind::Stroke { width: 12.0 / scale } };
        let n = s.shapes.iter().filter(|e| e.source == usize::MAX).count() + 1;
        s.shapes.push(ShapeEdit {
            source: usize::MAX,
            own: Some(Shape { kind, lines: vec![(line, closed)], layer }),
            name: if closed { t!("Drawn shape {n}", n = n) } else { t!("Drawn line {n}", n = n) },
            layer,
            look: None,
            hidden: false,
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            rotate: 0.0,
            angles: Vec::new(),
            entry: None,
            style: None,
        });
        if let Some(l) = s.layers.get_mut(layer) {
            l.enabled = true;
        }
        self.sel = vec![s.shapes.len() - 1];
    }

    pub(super) fn pen_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        if !self.pen.on {
            return;
        }
        let Some(m) = self.draw_map() else { return };
        let accent = self.accent();
        let screen = |u: Pt| frame.to_screen_pt(m.to_world(u));
        let mut line: Vec<egui::Pos2> = flatten(&self.pen.points, false).into_iter().map(screen).collect();
        if let (Some(h), false) = (self.pen.hover, self.pen.dragging)
            && !self.pen.points.is_empty()
        {
            line.push(screen(h));
        }
        painter.add(egui::Shape::line(line, egui::Stroke::new(2.0, accent)));
        for (k, (p, h)) in self.pen.points.iter().enumerate() {
            let c = screen(*p);
            if let Some(h) = h {
                for q in [(p.0 + h.0, p.1 + h.1), (p.0 - h.0, p.1 - h.1)] {
                    let s = screen(q);
                    painter.line_segment([c, s], egui::Stroke::new(1.0, accent.gamma_multiply(0.7)));
                    painter.circle_filled(s, 3.0, accent);
                }
            }
            let fill = if k == 0 { Color32::WHITE } else { accent };
            painter.rect(egui::Rect::from_center_size(c, vec2(7.0, 7.0)), 0, fill, egui::Stroke::new(1.0, accent), egui::StrokeKind::Middle);
        }
        let tip = t!("Click for a corner, drag for a curve · click the first point to close · Enter ends a line · Esc stops");
        let galley = painter.layout(tip.into(), egui::FontId::proportional(14.0), Color32::WHITE, frame.rect.width() - 80.0);
        let pos = frame.rect.center_top() + vec2(-galley.size().x / 2.0, 16.0);
        painter.rect_filled(egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0)), 8, Color32::from_black_alpha(190));
        painter.galley(pos, galley, Color32::WHITE);
    }

    /// The Draw card of the Source tab.
    pub(super) fn draw_card(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        let mut load = false;
        let mut clear = false;
        let mut start_blank = false;
        card_folded(ui, t!("Draw"), accent, |ui| {
            ui.label(RichText::new(t!("Trace a picture by hand: put it behind the hoop, then draw shapes over it with the pen.")).size(12.0).color(muted));
            ui.horizontal(|ui| {
                if ui.button(t!("Backdrop picture…")).clicked() {
                    load = true;
                }
                if ui.add_enabled(self.backdrop.is_some(), egui::Button::new(t!("Remove"))).clicked() {
                    clear = true;
                }
            });
            if let Some(b) = &mut self.backdrop {
                ui.label(RichText::new(&b.name).size(12.0).color(muted));
                slider_row(ui, t!("Show"), &mut b.opacity, 0.1..=1.0, "", 2).on_hover_text(t!("How strongly the picture shows"));
            }
            let on = self.pen.on;
            if ui.add(egui::Button::selectable(on, t!("Pen"))).on_hover_text(t!("Draw a shape or line point by point")).clicked() {
                self.pen.on = !on;
                self.pen.points.clear();
                if self.art.is_none() {
                    start_blank = true;
                }
            }
        });
        if start_blank {
            self.picture = None;
            self.use_artwork(Ok(artwork::blank()));
        }
        if clear {
            self.backdrop = None;
        }
        if load && self.pen.pick.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new().add_filter(t!("Pictures"), &["png", "jpg", "jpeg", "webp", "bmp", "gif"]).pick_file());
                ctx.request_repaint();
            });
            self.pen.pick = Some(rx);
        }
        if let Some(rx) = &self.pen.pick
            && let Ok(answer) = rx.try_recv()
        {
            self.pen.pick = None;
            if let Some(path) = answer {
                let ctx = ui.ctx().clone();
                self.load_backdrop(&ctx, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::flatten;

    #[test]
    fn corners_stay_straight_and_handles_curve() {
        let corners = [((0.0, 0.0), None), ((10.0, 0.0), None), ((10.0, 10.0), None)];
        assert_eq!(flatten(&corners, false), vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        assert_eq!(flatten(&corners, true).len(), 3);
        // A handle bends the segment away from the straight line.
        let curve = [((0.0, 0.0), Some((0.0, 10.0))), ((10.0, 0.0), Some((0.0, -10.0)))];
        let pts = flatten(&curve, false);
        assert_eq!(pts.len(), 17);
        assert!(pts[8].1 > 5.0, "{:?}", pts[8]);
        assert_eq!(pts[16], (10.0, 0.0));
    }
}
