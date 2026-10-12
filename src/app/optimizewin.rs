//! The Optimize objects window: choose what to tidy, see the result in
//! numbers before applying it. Undo puts everything back.

use super::*;
use crate::artwork::{Optimize, Optimized};

#[derive(Default)]
pub(super) struct OptimizeWin {
    pub open: bool,
    opts: Option<Optimize>,
    /// The preview and the options it was made for.
    preview: Option<(Optimize, usize, Optimized)>,
}

impl HoopApp {
    pub(super) fn optimize_window(&mut self, ctx: &egui::Context) {
        if !self.optimize.open {
            return;
        }
        let (Some(art), Some(s)) = (&self.artwork, &self.art) else {
            self.optimize.open = false;
            return;
        };
        let muted = self.muted();
        let accent = self.accent();
        let mut opts = self.optimize.opts.unwrap_or_default();
        // Re-run the preview on a copy when the options or the shapes change.
        let key = s.shapes.len();
        if self.optimize.preview.as_ref().is_none_or(|p| p.0 != opts || p.1 != key) {
            let mut copy = s.clone();
            let r = crate::artwork::optimize(art, &mut copy, opts);
            self.optimize.preview = Some((opts, key, r));
        }
        let r = self.optimize.preview.as_ref().unwrap().2;
        let mut open = true;
        let mut apply = false;
        egui::Window::new(t!("Optimize objects")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(
            ctx,
            |ui| {
                ui.set_width(400.0);
                ui.label(
                    RichText::new(t!("Tidies the Objects list so shapes are easier to pick and style. Undo (Ctrl+Z) puts everything back."))
                        .size(12.0)
                        .color(muted),
                );
                ui.add_space(6.0);
                let row = |ui: &mut egui::Ui, value: &mut bool, title: &str, about: &str| {
                    ui.checkbox(value, RichText::new(title).strong());
                    ui.indent(title, |ui| {
                        ui.label(RichText::new(about).size(12.0).color(muted));
                    });
                    ui.add_space(2.0);
                };
                row(ui, &mut opts.rename, t!("Name by thread"), t!("\"Black line 3\", \"Green fill 2\" instead of \"Line 31\""));
                row(
                    ui,
                    &mut opts.merge,
                    t!("Merge same-thread shapes"),
                    t!("Shapes of one thread and look become one object, unless a shape between them overlaps (the stacking stays as drawn)"),
                );
                row(
                    ui,
                    &mut opts.split,
                    t!("Break apart fills (adds objects)"),
                    t!("A fill made of separate pieces (five diamonds in one shape) becomes one shape per piece, each with its holes"),
                );
                row(
                    ui,
                    &mut opts.split_lines,
                    t!("Break apart lines (adds objects)"),
                    t!("Every loop or stroke of a line becomes its own shape. Makes long lists; useful to style one outline"),
                );
                row(
                    ui,
                    &mut opts.join,
                    t!("Join touching lines"),
                    t!("Lines of one color and width that meet end to end become one, so the needle doesn't stop and jump between them"),
                );
                row(ui, &mut opts.drop_small, t!("Remove specks and duplicates"), t!("Shapes smaller than Detail (Style → Clean-up) and exact copies"));
                ui.separator();
                let mut parts = Vec::new();
                if r.split > 0 {
                    parts.push(tn!(r.split, "{n} piece split off", "{n} pieces split off"));
                }
                if r.merged > 0 {
                    parts.push(tn!(r.merged, "{n} shape merged", "{n} shapes merged"));
                }
                if r.joined > 0 {
                    parts.push(tn!(r.joined, "{n} line joined", "{n} lines joined"));
                }
                if r.dropped > 0 {
                    parts.push(tn!(r.dropped, "{n} shape removed", "{n} shapes removed"));
                }
                ui.label(RichText::new(t!("{before} → {after} objects", before = r.before, after = r.after)).strong().size(15.0));
                if !parts.is_empty() {
                    ui.label(RichText::new(parts.join(" · ")).size(12.0).color(muted));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(primary(t!("Apply"), accent)).clicked() {
                        apply = true;
                    }
                    if ui.button(t!("Cancel")).clicked() {
                        self.optimize.open = false;
                    }
                });
            },
        );
        self.optimize.opts = Some(opts);
        if !open {
            self.optimize.open = false;
        }
        if apply && let (Some(art), Some(s)) = (&self.artwork, &mut self.art) {
            let r = crate::artwork::optimize(art, s, opts);
            self.sel.clear();
            self.optimize.open = false;
            self.optimize.preview = None;
            self.toast(t!("{before} → {after} objects", before = r.before, after = r.after));
        }
    }
}
