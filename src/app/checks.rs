//! The design check in the window: the Check list in the left dock, the
//! density view on the hoop, one-click fixes, and the question before
//! saving a design that has problems.

use super::*;
use crate::check::{self, Fix, Issue, Level};

/// Where a finished design goes once the check is passed or waved through.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Output {
    Save(crate::export::Out),
    Usb,
}

fn level_color(level: Level) -> Color32 {
    match level {
        Level::Problem => BAD,
        Level::Warning => WARN,
        Level::Note => Color32::from_rgb(0x7a, 0xa2, 0xf7),
    }
}

/// Red for thread piling up, amber for dense, nothing for normal coverage.
fn heat(coverage: f32) -> Option<Color32> {
    if coverage < 4.0 {
        return None;
    }
    let t = ((coverage - 4.0) / (check::TOO_DENSE - 4.0)).clamp(0.0, 1.0);
    let (r, g, b) = (0xf0, (0xc8 as f32 * (1.0 - t) + 0x30 as f32 * t) as u8, 0x20);
    Some(Color32::from_rgba_unmultiplied(r, g, b, (70.0 + 120.0 * t) as u8))
}

impl HoopApp {
    /// Keeps the check in step with the design shown, redoing it only when
    /// the stitches or the mode changed.
    pub(super) fn refresh_check(&mut self) {
        let Some(d) = &self.design else {
            self.check = None;
            return;
        };
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.mode as u8).hash(&mut h);
        d.pattern.points.len().hash(&mut h);
        for p in &d.pattern.points {
            p.0.to_bits().hash(&mut h);
            p.1.to_bits().hash(&mut h);
        }
        let key = h.finish();
        if self.check.is_some() && self.check_key == key {
            return;
        }
        self.check_key = key;
        let mut report = check::stitches(&d.pattern);
        match self.mode {
            Mode::Artwork => {
                if let (Some(art), Some(s)) = (&self.artwork, &self.art) {
                    check::artwork(&mut report, art, s, &self.tuning);
                }
            }
            Mode::Lettering => check::lettering(&mut report, &self.letter, &self.tuning),
            Mode::Viewer => {}
        }
        self.check = Some(report);
    }

    /// The Check page of the left dock.
    pub(super) fn check_list(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let Some(report) = &self.check else {
            ui.label(RichText::new(t!("Open or make a design to check how it will sew.")).size(12.0).color(muted));
            return;
        };
        let (problems, warnings) = (report.count(Level::Problem), report.count(Level::Warning));
        let (color, headline) = match (problems, warnings) {
            (0, 0) => (OK, t!("Ready to sew").to_string()),
            (0, w) => (WARN, tn!(w, "{n} thing to look at", "{n} things to look at")),
            (p, _) => (BAD, tn!(p, "{n} problem to fix", "{n} problems to fix")),
        };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 6.0, color);
            ui.label(RichText::new(headline).strong().color(color));
        });
        ui.checkbox(&mut self.density, t!("Show thread density on the hoop  H")).on_hover_text(t!("Amber where thread is dense, red where it piles up"));
        ui.separator();
        let issues: Vec<Issue> = report.issues.clone();
        let mut show = None;
        let mut fix = None;
        egui::ScrollArea::vertical().id_salt("check").auto_shrink([false, false]).show(ui, |ui| {
            if issues.is_empty() {
                ui.label(RichText::new(t!("Density, stitch lengths and shape widths all look right for this design.")).size(12.0).color(muted));
            }
            for (k, issue) in issues.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, level_color(issue.level));
                    ui.add(egui::Label::new(RichText::new(&issue.title).strong()).wrap());
                });
                ui.add(egui::Label::new(RichText::new(&issue.detail).size(12.0).color(muted)).wrap());
                ui.horizontal(|ui| {
                    if (issue.at.is_some() || issue.shape.is_some()) && ui.small_button(t!("Show")).on_hover_text(t!("Zoom to it on the hoop")).clicked() {
                        show = Some(k);
                    }
                    if let Some(f) = issue.fix
                        && ui.small_button(f.label()).on_hover_text(t!("Fix it this way")).clicked()
                    {
                        fix = Some(f);
                    }
                });
                ui.add_space(6.0);
            }
            ui.separator();
            ui.label(RichText::new(t!("{stitches} stitches checked. In the density view, uncolored parts carry about one layer of thread, the way machine embroidery is meant to be.", stitches = report.stitches)).size(11.5).color(muted));
        });
        if let Some(k) = show {
            self.show_issue(&issues[k]);
        }
        if let Some(f) = fix {
            self.apply_check_fix(f);
        }
    }

    pub(super) fn show_issue(&mut self, issue: &Issue) {
        if let Some(i) = issue.shape
            && self.mode == Mode::Artwork
        {
            self.sel = vec![i];
            self.edit_points = false;
        }
        let (Some(d), Some(at)) = (&self.design, issue.at) else { return };
        let w = Transform::new(d.bounds, &self.setup.at).apply(at);
        // Close enough to see stitches, never zooming out.
        let scale = self.view.scale.max(self.hoop_scale * 3.0);
        self.view = View { scale, pan: vec2(-w.0, -w.1) * scale };
        self.user_view = true;
        self.flash = Some((at, std::time::Instant::now()));
    }

    pub(super) fn apply_check_fix(&mut self, f: Fix) {
        match f {
            Fix::Rows(mm) => match self.mode {
                Mode::Lettering => self.letter.density_mm = mm,
                _ => {
                    if let Some(s) = &mut self.art {
                        s.style.density_mm = mm;
                    }
                }
            },
            Fix::Knockout => {
                if let Some(s) = &mut self.art {
                    s.knockout = true;
                }
            }
            Fix::ShapeLook(i, look) => {
                if let Some(e) = self.art.as_mut().and_then(|s| s.shapes.get_mut(i)) {
                    e.look = Some(look);
                }
            }
            Fix::Height(mm) => self.letter.height_mm = mm,
            Fix::LetterFill(k) => {
                let (angle, edge) = (self.letter.fill_angle, self.letter.edge);
                lettering::style(&mut self.letter, k, angle, edge);
            }
            Fix::EdgeWidth(mm) => self.letter.edge_width_mm = mm,
        }
        self.toast(t!("Changed: {fix}", fix = f.label()));
    }

    /// Saving and sending stop once to list problems the check found.
    pub(super) fn checked(&mut self, out: Output) -> bool {
        let problems = self.check.as_ref().map_or(0, |r| r.count(Level::Problem));
        if problems == 0 || self.preflight_passed {
            self.preflight_passed = false;
            return true;
        }
        self.preflight = Some(out);
        false
    }

    pub(super) fn preflight_window(&mut self, ctx: &egui::Context) {
        let Some(out) = self.preflight else { return };
        let problems: Vec<String> =
            self.check.as_ref().map_or(Vec::new(), |r| r.issues.iter().filter(|i| i.level == Level::Problem).map(|i| i.title.clone()).collect());
        let mut close = false;
        egui::Window::new(t!("This design has problems")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label(t!("The check found things that will likely sew badly:"));
            for p in &problems {
                ui.label(RichText::new(format!("•  {p}")).color(BAD));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(primary(t!("Review"), self.accent())).clicked() {
                    self.dock = Dock::Check;
                    self.show_left = true;
                    close = true;
                }
                let label = if matches!(out, Output::Save(_)) { t!("Save anyway") } else { t!("Send anyway") };
                if ui.button(label).clicked() {
                    self.preflight_passed = true;
                    close = true;
                    match out {
                        Output::Save(o) => self.export(o),
                        Output::Usb => self.open_usb(),
                    }
                }
                if ui.button(t!("Cancel")).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.preflight = None;
        }
    }

    /// The density view and the marker on a shown issue, over the stitches.
    pub(super) fn check_overlay(&mut self, painter: &egui::Painter, frame: &Frame) {
        let Some(d) = &self.design else { return };
        let t = Transform::new(d.bounds, &self.setup.at);
        if self.density
            && let Some(r) = &self.check
        {
            let cell = 10.0;
            for y in 0..r.h {
                for x in 0..r.w {
                    let Some(color) = heat(r.coverage[y * r.w + x]) else { continue };
                    let a = (r.origin.0 + x as f32 * cell, r.origin.1 + y as f32 * cell);
                    let b = (a.0 + cell, a.1 + cell);
                    let rect = egui::Rect::from_two_pos(frame.to_screen_pt(t.apply(a)), frame.to_screen_pt(t.apply(b)));
                    painter.rect_filled(rect, 0, color);
                }
            }
            // Legend in the corner.
            let at = frame.rect.left_top() + vec2(12.0, 12.0);
            let bg = egui::Rect::from_min_size(at, vec2(236.0, 30.0));
            painter.rect_filled(bg, 6, Color32::from_black_alpha(170));
            let mut x = at.x + 10.0;
            for (label, cov) in [("dense", 5.5), ("heavy", 7.5), (t!("too much"), 10.0)] {
                let sw = egui::Rect::from_min_size(egui::pos2(x, at.y + 9.0), vec2(12.0, 12.0));
                painter.rect_filled(sw, 2, heat(cov).unwrap_or(Color32::TRANSPARENT).to_opaque());
                let g = painter.layout_no_wrap(label.into(), egui::FontId::proportional(12.0), Color32::WHITE);
                let w = g.size().x;
                painter.galley(egui::pos2(x + 17.0, at.y + 15.0 - g.size().y / 2.0), g, Color32::WHITE);
                x += w + 32.0;
            }
        }
        if let Some((at, when)) = self.flash {
            let age = when.elapsed().as_secs_f32();
            if age > 2.5 {
                self.flash = None;
                return;
            }
            let p = frame.to_screen_pt(t.apply(at));
            let r = 14.0 + 10.0 * (age * 4.0).sin().abs();
            painter.circle_stroke(p, r, egui::Stroke::new(3.0, Color32::from_black_alpha(140)));
            painter.circle_stroke(p, r, egui::Stroke::new(2.0, Color32::from_rgb(0xff, 0x40, 0xa0)));
            painter.ctx().request_repaint();
        }
    }
}
