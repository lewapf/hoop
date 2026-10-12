//! The "Split into hoopings" window: parts shown on the hoop, saved as
//! numbered JEF files.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use super::*;
use crate::split::{self, Part};

#[derive(Default)]
pub(super) struct Hoopings {
    pub open: bool,
    pub hoop: usize,
    /// The parts and what they were made from (stitch count, hoop).
    parts: Option<(Vec<Part>, (usize, usize))>,
    job: Option<Receiver<Option<PathBuf>>>,
}

impl HoopApp {
    pub(super) fn open_hoopings(&mut self) {
        self.hoopings.open = true;
        self.hoopings.hoop = self.setup.hoop;
        self.hoopings.parts = None;
    }

    fn hooping_parts(&mut self) -> Option<&Vec<Part>> {
        let (points, cmds, threads) = self.hoop_stitches()?;
        let key = (points.len(), self.hoopings.hoop);
        if self.hoopings.parts.as_ref().is_none_or(|(_, k)| *k != key) {
            let h = HOOPS[self.hoopings.hoop];
            let parts = split::split(&points, &cmds, &threads, (h.w * 10.0, h.h * 10.0));
            self.hoopings.parts = Some((parts, key));
        }
        self.hoopings.parts.as_ref().map(|(p, _)| p)
    }

    /// Where the hoop-centered stitches sit on the canvas.
    fn hooping_offset(&self) -> (f32, f32) {
        if self.pinned.is_empty() { (self.setup.at.dx, self.setup.at.dy) } else { (0.0, 0.0) }
    }

    pub(super) fn hoopings_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        if !self.hoopings.open {
            return;
        }
        let Some((parts, _)) = &self.hoopings.parts else { return };
        let accent = self.accent();
        let (ox, oy) = self.hooping_offset();
        let h = HOOPS[self.hoopings.hoop];
        for (k, p) in parts.iter().enumerate() {
            let cell = egui::Rect::from_two_pos(frame.to_screen(p.cell.0 + ox, p.cell.1 + oy), frame.to_screen(p.cell.2 + ox, p.cell.3 + oy));
            painter.rect_stroke(cell, 0, egui::Stroke::new(2.0, accent), egui::StrokeKind::Middle);
            let hoop = egui::Rect::from_center_size(frame.to_screen(p.center.0 + ox, p.center.1 + oy), vec2(h.w * 10.0, h.h * 10.0) * frame.view.scale);
            painter.rect_stroke(hoop, 6, egui::Stroke::new(1.0, accent.gamma_multiply(0.5)), egui::StrokeKind::Middle);
            let label = painter.layout_no_wrap((k + 1).to_string(), egui::FontId::proportional(22.0), contrast(accent));
            let at = cell.center() - label.size() / 2.0;
            painter.circle_filled(cell.center(), 18.0, accent);
            painter.galley(at, label, contrast(accent));
        }
    }

    pub(super) fn hoopings_window(&mut self, ctx: &egui::Context) {
        if !self.hoopings.open {
            return;
        }
        if let Some(rx) = &self.hoopings.job
            && let Ok(folder) = rx.try_recv()
        {
            self.hoopings.job = None;
            if let Some(folder) = folder {
                self.save_hoopings(&folder);
            }
        }
        let muted = self.muted();
        let accent = self.accent();
        let mut open = true;
        let mut pick = false;
        let summary: Vec<(usize, usize)> =
            self.hooping_parts().map(|parts| parts.iter().map(|p| (p.pattern.count(Cmd::Stitch), p.pattern.threads.len())).collect()).unwrap_or_default();
        egui::Window::new(t!("Split into hoopings")).open(&mut open).collapsible(false).resizable(false).default_pos(egui::pos2(80.0, 120.0)).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(t!("For designs bigger than the hoop: each part is sewn in its own hooping. Objects stay whole where they fit; the rest is cut at the part edges.")).size(12.0).color(muted));
            ui.horizontal(|ui| {
                ui.label(t!("Hoop"));
                egui::ComboBox::from_id_salt("split-hoop").selected_text(HOOPS[self.hoopings.hoop].name).show_ui(ui, |ui| {
                    for (i, h) in HOOPS.iter().enumerate() {
                        ui.selectable_value(&mut self.hoopings.hoop, i, format!("{}  {} × {} mm", h.name, h.w, h.h));
                    }
                });
            });
            ui.add_space(4.0);
            for (k, (stitches, threads)) in summary.iter().enumerate() {
                ui.label(t!("Part {k}: {stitches} stitches, {threads} threads", k = k + 1, stitches = stitches, threads = threads));
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(t!("Sew the parts in order. Each starts with gray registration crosses on its shared edges: after re-hooping, line the needle up with the crosses of the part before. Pick the crosses out at the end."))
                    .size(12.0)
                    .color(muted),
            );
            ui.add_space(6.0);
            let busy = self.hoopings.job.is_some();
            if ui.add_enabled(!summary.is_empty() && !busy, primary(&tn!(summary.len(), "Save {n} part…", "Save {n} parts…"), accent)).clicked() {
                pick = true;
            }
        });
        if !open {
            self.hoopings.open = false;
        }
        if pick {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            let dir = library::designs_dir();
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new().set_title(t!("Folder for the hoopings")).set_directory(dir).pick_folder());
                ctx.request_repaint();
            });
            self.hoopings.job = Some(rx);
        }
    }

    fn save_hoopings(&mut self, folder: &std::path::Path) {
        let stem = self.design_stem();
        let Some((parts, _)) = &self.hoopings.parts else { return };
        let n = parts.len();
        let mut err = None;
        for (k, p) in parts.iter().enumerate() {
            // Each part is centered on its own hoop.
            let pts: Vec<(f32, f32)> = p.pattern.points.iter().map(|q| (q.0 - p.center.0, q.1 - p.center.1)).collect();
            let bytes = formats::write_jef(&pts, &p.pattern.cmds, &export::jef_numbers(&p.pattern.threads));
            let file = folder.join(format!("{stem}-{}of{n}.jef", k + 1));
            if let Err(e) = std::fs::write(&file, bytes) {
                err = Some(t!("Couldn't save {file}: {e}.", file = file.display(), e = e));
                break;
            }
        }
        match err {
            Some(e) => self.error = Some(e),
            None => self.toast(tn!(n, "Saved {n} hooping to {folder}", "Saved {n} hoopings to {folder}", folder = folder.display())),
        }
    }
}
