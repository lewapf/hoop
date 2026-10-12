//! Tools that work on the picked artwork shape by pointing at the hoop:
//! stitch angle lines for its fill and the point where the needle enters.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ShapeTool {
    /// Drag a line across the shape; the fill's rows follow it.
    Angle,
    /// Click where the needle should start the shape.
    Entry,
}

#[derive(Default)]
pub(super) struct Tools {
    pub tool: Option<ShapeTool>,
    /// Where a line drag began and where it is now, on the hoop.
    drag: Option<((f32, f32), (f32, f32))>,
}

impl HoopApp {
    /// The picked shape's index, when artwork is open.
    fn tool_target(&self) -> Option<usize> {
        (self.mode == Mode::Artwork && self.art.is_some()).then(|| self.primary()).flatten()
    }

    /// Pointer input while a tool is on. Returns true when it used the
    /// pointer, so nothing else moves.
    pub(super) fn tool_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        let Some(tool) = self.tools.tool else { return false };
        let Some(i) = self.tool_target() else {
            self.tools.tool = None;
            return false;
        };
        if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            self.tools.tool = None;
            self.tools.drag = None;
            return true;
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let Some(m) = self.shape_map() else { return false };
        let at = response.interact_pointer_pos().map(|p| frame.to_world(p));
        match tool {
            ShapeTool::Entry => {
                if response.clicked() {
                    if let (Some(w), Some(art)) = (at, &self.artwork) {
                        let s = self.art.as_mut().unwrap();
                        let e = &mut s.shapes[i];
                        e.entry = Some(e.unplace(art, m.to_user(w)));
                        self.tools.tool = None;
                    }
                }
            }
            ShapeTool::Angle => {
                if response.drag_started() {
                    if let Some(w) = at {
                        self.tools.drag = Some((w, w));
                    }
                }
                if let (Some(d), Some(w)) = (&mut self.tools.drag, at) {
                    d.1 = w;
                }
                if response.drag_stopped() {
                    if let (Some((a, b)), Some(art)) = (self.tools.drag.take(), &self.artwork) {
                        // A click or a tiny drag isn't a direction.
                        if (a.0 - b.0).hypot(a.1 - b.1) > 20.0 {
                            let s = self.art.as_mut().unwrap();
                            let e = &mut s.shapes[i];
                            let (ua, ub) = (e.unplace(art, m.to_user(a)), e.unplace(art, m.to_user(b)));
                            e.angles.push((ua, ub));
                        }
                    }
                }
            }
        }
        true
    }

    /// The picked shape's angle lines and start point, and the tool hint.
    pub(super) fn tool_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        let accent = self.accent();
        if let (Some(i), Some(m), Some(art)) = (self.tool_target(), self.shape_map(), &self.artwork) {
            let e = &self.art.as_ref().unwrap().shapes[i];
            let screen = |p: (f32, f32)| frame.to_screen_pt(m.to_world(e.place(art, p)));
            for &(a, b) in &e.angles {
                arrow(painter, screen(a), screen(b), accent);
            }
            if let Some(p) = e.entry {
                let c = screen(p);
                painter.circle(c, 6.0, Color32::from_rgb(0x55, 0xc2, 0x8c), egui::Stroke::new(1.5, Color32::WHITE));
                painter.text(c + vec2(9.0, -9.0), egui::Align2::LEFT_BOTTOM, t!("Start"), egui::FontId::proportional(12.0), Color32::WHITE);
            }
        }
        if let Some((a, b)) = self.tools.drag {
            arrow(painter, frame.to_screen_pt(a), frame.to_screen_pt(b), accent);
        }
        if let Some(tool) = self.tools.tool {
            let tip = match tool {
                ShapeTool::Angle => t!("Drag across the shape in the direction the stitches should run · Esc to stop"),
                ShapeTool::Entry => t!("Click where the needle should start this shape · Esc to stop"),
            };
            let galley = painter.layout(tip.into(), egui::FontId::proportional(14.0), Color32::WHITE, frame.rect.width() - 80.0);
            let pos = frame.rect.center_top() + vec2(-galley.size().x / 2.0, 16.0);
            painter.rect_filled(egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0)), 8, Color32::from_black_alpha(190));
            painter.galley(pos, galley, Color32::WHITE);
        }
    }

    /// The Stitching card of the Shape tab: angle lines and start point.
    pub(super) fn tool_card(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.tool_target() else { return };
        let muted = self.muted();
        let accent = self.accent();
        let (count, has_entry) = {
            let e = &self.art.as_ref().unwrap().shapes[i];
            (e.angles.len(), e.entry.is_some())
        };
        let mut clear_angles = false;
        let mut clear_entry = false;
        card(ui, t!("Stitching"), accent, |ui| {
            ui.horizontal(|ui| {
                let on = self.tools.tool == Some(ShapeTool::Angle);
                if ui
                    .add(egui::Button::selectable(on, t!("Add stitch angle")))
                    .on_hover_text(t!("Drag a line across the shape; its fill rows follow it. Several lines split the fill into parts."))
                    .clicked()
                {
                    self.tools.tool = if on { None } else { Some(ShapeTool::Angle) };
                }
                if ui.add_enabled(count > 0, egui::Button::new(t!("Clear"))).clicked() {
                    clear_angles = true;
                }
            });
            if count > 0 {
                ui.label(RichText::new(tn!(count, "{n} angle line", "{n} angle lines")).size(12.0).color(muted));
            }
            ui.horizontal(|ui| {
                let on = self.tools.tool == Some(ShapeTool::Entry);
                if ui
                    .add(egui::Button::selectable(on, t!("Set start point")))
                    .on_hover_text(t!("Where the needle comes in, to save a jump or hide the tie-in"))
                    .clicked()
                {
                    self.tools.tool = if on { None } else { Some(ShapeTool::Entry) };
                }
                if ui.add_enabled(has_entry, egui::Button::new(t!("Clear"))).clicked() {
                    clear_entry = true;
                }
            });
        });
        let s = self.art.as_mut().unwrap();
        if clear_angles {
            s.shapes[i].angles.clear();
        }
        if clear_entry {
            s.shapes[i].entry = None;
        }
    }
}

/// A line with an arrow head at `b`.
fn arrow(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, color: Color32) {
    let stroke = egui::Stroke::new(2.0, color);
    painter.line_segment([a, b], egui::Stroke::new(4.0, Color32::from_black_alpha(90)));
    painter.line_segment([a, b], stroke);
    let d = b - a;
    if d.length() < 4.0 {
        return;
    }
    let u = d.normalized();
    let side = vec2(-u.y, u.x);
    painter.line_segment([b, b - u * 10.0 + side * 5.0], stroke);
    painter.line_segment([b, b - u * 10.0 - side * 5.0], stroke);
}
