//! The hoop canvas: drawing the fabric, hoop and design, pointer input,
//! zoom and pan, playback, and the keyboard shortcuts.

use super::*;

impl HoopApp {
    pub(super) fn fit_view(&mut self, rect: egui::Rect, design: bool) {
        let (w, h, cx, cy) = match (&self.design, design) {
            (Some(d), true) => {
                let (dw, dh) = self.setup.at.size(d.size().0, d.size().1);
                (dw * 1.15 + 40.0, dh * 1.15 + 40.0, self.setup.at.dx, self.setup.at.dy)
            }
            _ => {
                let (hw, hh) = self.hoop_size();
                let frame = 2.0 * (canvas::OPENING_MARGIN + canvas::RING);
                (hw * 10.0 + frame + 500.0, hh * 10.0 + frame + 80.0, 0.0, 0.0)
            }
        };
        let scale = (rect.width() / w).min(rect.height() / h);
        if !design {
            self.hoop_scale = scale;
        }
        self.view = View { scale, pan: vec2(-cx, -cy) * scale };
        // Showing the whole hoop hands framing back to the window size.
        self.user_view = design;
    }

    pub(super) fn total(&self) -> usize {
        self.design.as_ref().map_or(0, |d| d.pattern.points.len())
    }

    pub(super) fn toggle_play(&mut self) {
        if self.design.is_none() {
            return;
        }
        if !self.playing && self.shown >= self.total() {
            self.shown = 0;
        }
        self.playing = !self.playing;
        self.play_acc = 0.0;
    }

    pub(super) fn jump_thread(&mut self, forward: bool) {
        let Some(d) = &self.design else { return };
        self.playing = false;
        let ends: Vec<usize> = std::iter::once(0).chain(d.blocks.iter().map(|b| b.end)).collect();
        self.shown = if forward {
            ends.iter().copied().find(|&e| e > self.shown).unwrap_or(self.total())
        } else {
            ends.iter().rev().copied().find(|&e| e < self.shown).unwrap_or(0)
        };
    }

    pub(super) fn keys(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let press = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        let typed_question = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "?")));
        if typed_question {
            self.help = !self.help;
        }
        if self.test.picking && press(Modifiers::NONE, Key::Escape) {
            self.test.picking = false;
        }
        if !self.help {
            self.shape_keys(ctx);
        }
        if press(Modifiers::NONE, Key::Escape) {
            self.help = false;
        }
        if press(Modifiers::NONE, Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if press(Modifiers::NONE, Key::O) || press(Modifiers::COMMAND, Key::O) {
            self.pick_file();
        }
        if press(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            self.save_picture();
        }
        if press(Modifiers::COMMAND, Key::P) && self.design.is_some() {
            self.save_worksheet();
        }
        if press(Modifiers::NONE, Key::N) || press(Modifiers::COMMAND, Key::N) {
            self.open_wizard();
        }
        if self.demo_running() && press(Modifiers::NONE, Key::Escape) {
            self.stop_demo();
        }
        if press(Modifiers::NONE, Key::F8) {
            self.timeline_open = !self.timeline_open;
        }
        if press(Modifiers::NONE, Key::F9) {
            self.show_left = !self.show_left;
        }
        if press(Modifiers::NONE, Key::F10) {
            self.show_right = !self.show_right;
        }
        if press(Modifiers::NONE, Key::F1) {
            self.manual.open = !self.manual.open;
        }
        if press(Modifiers::COMMAND, Key::Comma) {
            self.settings.open = !self.settings.open;
        }
        if press(Modifiers::COMMAND, Key::U) {
            self.open_usb();
        }
        if press(Modifiers::COMMAND, Key::T) {
            self.test.open = !self.test.open;
        }
        if press(Modifiers::COMMAND, Key::S) {
            self.save_design();
        }
        if press(Modifiers::NONE, Key::P) {
            self.pin();
        }
        if press(Modifiers::NONE, Key::A) {
            self.set_mode(Mode::Artwork);
        }
        if press(Modifiers::NONE, Key::L) {
            self.set_mode(Mode::Lettering);
        }
        if press(Modifiers::NONE, Key::V) {
            self.set_mode(Mode::Viewer);
        }
        if press(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || press(Modifiers::COMMAND, Key::Y) {
            self.redo_step();
        }
        if press(Modifiers::COMMAND, Key::Z) {
            self.undo_step(ctx);
        }
        if press(Modifiers::NONE, Key::Space) {
            self.toggle_play();
        }
        if press(Modifiers::NONE, Key::Home) {
            self.playing = false;
            self.shown = 0;
        }
        if press(Modifiers::NONE, Key::End) {
            self.playing = false;
            self.shown = self.total();
        }
        if press(Modifiers::NONE, Key::OpenBracket) {
            self.jump_thread(false);
        }
        if press(Modifiers::NONE, Key::CloseBracket) {
            self.jump_thread(true);
        }
        if press(Modifiers::NONE, Key::Comma) {
            self.playing = false;
            self.shown = self.shown.saturating_sub(1);
        }
        if press(Modifiers::NONE, Key::Period) {
            self.playing = false;
            self.shown = (self.shown + 1).min(self.total());
        }
        for (key, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            for (m, step) in [(Modifiers::NONE, 10.0), (Modifiers::SHIFT, 100.0)] {
                if !self.stitch_edit.on && press(m, key) && self.design.is_some() {
                    self.change(|s| {
                        s.at.dx += dx * step;
                        s.at.dy += dy * step;
                    });
                }
            }
        }
        if press(Modifiers::SHIFT, Key::R) {
            self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
        }
        if press(Modifiers::NONE, Key::R) {
            self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
        }
        if press(Modifiers::NONE, Key::M) {
            self.change(|s| s.at.mirror = !s.at.mirror);
        }
        if press(Modifiers::NONE, Key::C) {
            self.change(|s| {
                s.at.dx = 0.0;
                s.at.dy = 0.0;
            });
        }
        for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
            if press(Modifiers::NONE, key) && i < hoops::list().len() {
                self.change(|s| s.hoop = i);
                self.fit_view(canvas_rect, false);
            }
        }
        if press(Modifiers::NONE, Key::F) {
            self.fit_view(canvas_rect, false);
        }
        if press(Modifiers::COMMAND | Modifiers::ALT, Key::S) {
            self.save_project(false);
        }
        if press(Modifiers::NONE, Key::D) {
            self.fit_view(canvas_rect, true);
        }
        if press(Modifiers::NONE, Key::G) {
            self.grid = !self.grid;
        }
        if press(Modifiers::NONE, Key::H) {
            self.density = !self.density;
        }
        if press(Modifiers::NONE, Key::J) {
            self.jumps = !self.jumps;
        }
        if press(Modifiers::NONE, Key::T) {
            self.textured = !self.textured;
        }
    }

    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // Keep the hoop framed as the window resizes until the user zooms or pans.
        if rect.size() != self.canvas_rect.size() && !self.user_view {
            self.fit_pending = true;
        }
        self.canvas_rect = rect;
        if self.fit_pending && rect.width() > 10.0 {
            self.fit_view(rect, false);
            self.fit_pending = false;
            // For screenshots: HOOP_ZOOM=factor,x,y zooms in on (x, y) mm.
            if let Some(z) =
                std::env::var("HOOP_ZOOM").ok().map(|v| v.split(',').filter_map(|n| n.parse::<f32>().ok()).collect::<Vec<_>>()).filter(|z| z.len() == 3)
            {
                self.view.scale *= z[0];
                self.view.pan = -vec2(z[1] * 10.0, z[2] * 10.0) * self.view.scale;
                self.user_view = true;
            }
        }
        let painter = ui.painter_at(rect);
        let frame = Frame { painter: &painter, rect, view: self.view };

        // Drawing the test area takes over the pointer until it is done.
        let picking = self.test_area_input(ui, &response, &frame)
            || self.tool_input(ui, &response, &frame)
            || self.stitch_edit_input(ui, &response, &frame)
            || self.pen_input(ui, &response, &frame);
        // Artwork shapes: pick, drag, resize, turn, edit points, rubber band.
        let handled_double = !picking && self.shape_input(ui, &response, &frame);
        if !picking {
            self.right_click_pick(&response, &frame);
        }
        let hover_shape = if picking { None } else { self.shape_hover(ui, &response, &frame) };

        // Interaction: drag the design to move it, drag elsewhere to pan, scroll to zoom.
        if response.drag_started() && self.drag.is_none() && !picking {
            let over = response.interact_pointer_pos().map(|p| frame.to_world(p));
            let middle = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
            self.drag = match (over, &self.design) {
                _ if middle => Some(Drag::Pan),
                (Some(w), Some(d)) => {
                    let (dw, dh) = self.setup.at.size(d.size().0, d.size().1);
                    let at = self.setup.at;
                    if (w.0 - at.dx).abs() <= dw / 2.0 + 20.0 && (w.1 - at.dy).abs() <= dh / 2.0 + 20.0 {
                        Some(Drag::Move { start: w, from: at })
                    } else {
                        Some(Drag::Pan)
                    }
                }
                _ => Some(Drag::Pan),
            };
        }
        if response.dragged() {
            match &self.drag {
                Some(Drag::Move { start, from }) => {
                    if let Some(p) = response.interact_pointer_pos() {
                        let w = frame.to_world(p);
                        // Snap to 0.5 mm, the step the machine's jog keys use.
                        self.setup.at.dx = ((from.dx + w.0 - start.0) / 5.0).round() * 5.0;
                        self.setup.at.dy = ((from.dy + w.1 - start.1) / 5.0).round() * 5.0;
                    }
                }
                Some(Drag::Pan) => {
                    self.view.pan += response.drag_delta();
                    self.user_view = true;
                }
                _ => {}
            }
        }
        if response.drag_stopped() {
            self.drag = None;
        }
        if response.double_clicked() && !handled_double {
            self.fit_view(rect, false);
        }
        if response.hovered() {
            let (scroll, zoom, pointer) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.pointer.hover_pos()));
            let factor = zoom * (scroll * 0.0025).exp();
            if (factor - 1.0).abs() > 1e-4
                && let Some(p) = pointer
            {
                let old = self.view.scale;
                let new = (old * factor).clamp(0.05, 40.0);
                let anchor = p - rect.center();
                self.view.pan = anchor - (anchor - self.view.pan) * (new / old);
                self.view.scale = new;
                self.user_view = true;
            }
        }

        let frame = Frame { painter: &painter, rect, view: self.view };
        let weave = self.weave.get_or_insert_with(|| {
            ui.ctx().load_texture(
                "weave",
                canvas::weave_image(),
                egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::NEAREST },
            )
        });
        canvas::draw_fabric(&frame, self.fabric, weave);
        let hoop = hoops::get(self.setup.hoop);
        canvas::draw_hoop(&frame, hoop.w * 10.0, hoop.h * 10.0, &t!("{name}  {w} × {h} mm", name = hoop.name, w = hoop.w, h = hoop.h), self.fabric, self.grid);
        self.draw_backdrop(&painter, &frame);

        let pinned_style = DesignStyle { textured: self.textured, show_jumps: false, focus: None, fabric: self.fabric };
        let identity = Transform::new((0.0, 0.0, 0.0, 0.0), &Placement::default());
        for p in &self.pinned {
            canvas::draw_design(&frame, &p.pattern, &p.blocks, &identity, p.pattern.points.len(), &pinned_style);
        }
        if let Some(d) = &self.design {
            let t = Transform::new(d.bounds, &self.setup.at);
            let style = DesignStyle { textured: self.textured, show_jumps: self.jumps, focus: self.timeline_hover.or(self.focus), fabric: self.fabric };
            canvas::draw_design(&frame, &d.pattern, &d.blocks, &t, self.shown, &style);
            let size = self.setup.at.size(d.size().0, d.size().1);
            let off = !matches!(hoops::fit(self.hoop_size(), d.size(), &self.setup.at), Fit::Inside(_));
            if off {
                canvas::draw_bounds(&frame, size, &self.setup.at, Color32::from_rgb(0xe0, 0x36, 0x2c));
            } else if matches!(self.drag, Some(Drag::Move { .. })) {
                canvas::draw_bounds(&frame, size, &self.setup.at, self.accent());
            }
            if self.sizes && !self.playing {
                // Finished size just under the design, like a dimension on a drawing.
                let at = self.setup.at;
                let below = frame.to_screen(at.dx, at.dy + size.1 / 2.0) + vec2(0.0, 14.0);
                let text = t!("{v0:.1} × {v1:.1} mm", v0 = size.0 / 10.0, v1 = size.1 / 10.0);
                let ink = if canvas::luminance(self.fabric) < 0.45 { Color32::from_white_alpha(200) } else { Color32::from_black_alpha(170) };
                painter.text(below, egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(12.0), ink);
            }
        } else {
            painter.text(
                rect.center_bottom() - vec2(0.0, 28.0),
                egui::Align2::CENTER_CENTER,
                t!("Press O to open a design, N for the new project wizard, or drop a file here"),
                egui::FontId::proportional(15.0),
                Color32::from_white_alpha(230),
            );
        }

        if self.capture == Capture::Requested {
            // This frame has no overlays; grab it once it is on screen.
            self.capture = Capture::Waiting(rect, std::time::Instant::now());
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            return;
        }
        if let Some((text, at)) = &self.status {
            let age = at.elapsed().as_secs_f32();
            if age < 4.0 {
                let alpha = ((4.0 - age) / 0.5).min(1.0);
                let galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(14.0), Color32::WHITE.gamma_multiply(alpha));
                let pos = rect.center_top() + vec2(-galley.size().x / 2.0, 16.0);
                let bg = egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0));
                painter.rect_filled(bg, 8, Color32::from_black_alpha((190.0 * alpha) as u8));
                painter.galley(pos, galley, Color32::WHITE);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(if age > 3.5 { 30 } else { 500 }));
            }
        }

        self.check_overlay(&painter, &frame);
        self.shape_overlay(&painter, &frame, hover_shape.or(self.objects.hover));
        self.tool_overlay(&painter, &frame);
        self.hoopings_overlay(&painter, &frame);
        self.stitch_edit_overlay(&painter, &frame);
        self.pen_overlay(&painter, &frame);
        self.tool_palette(ui.ctx(), rect);
        if !self.edit_points && self.tools.tool.is_none() && !self.pen.on {
            response.context_menu(|ui| self.canvas_menu(ui, rect));
        }
        self.demo_overlay(&painter, &frame);
        self.test_overlay(&painter, &frame);
        self.hover_at = response.hover_pos().map(|p| frame.to_world(p));
    }
}
