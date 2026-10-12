//! Editing an opened stitch file: thread order, deleting and merging
//! threads, removing tiny stitches, and moving or deleting single stitches
//! on the hoop. Edits are undoable: the pattern rides along in snapshots
//! behind a shared pointer, so taking a snapshot every frame stays cheap.

use std::rc::Rc;

use super::*;

/// A pattern shared between snapshots; equal only when it is the same edit.
#[derive(Clone)]
pub(super) struct Shared(pub Rc<Pattern>);

impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Copy)]
pub(super) enum BlockOp {
    Earlier,
    Later,
    Delete,
}

#[derive(Default)]
pub(super) struct StitchEdit {
    /// The pattern as last edited, for undo.
    pub shared: Option<Shared>,
    /// Picking and moving single stitches on the hoop.
    pub on: bool,
    pub picked: Option<usize>,
    dragging: bool,
}

/// Pattern operations, kept free of the app so they can be tested.
pub(super) mod ops {
    use crate::formats::{Block, Cmd, Pattern, Thread};

    /// The pattern rebuilt from its blocks in `order`, with threads to match.
    pub fn reorder(p: &Pattern, blocks: &[Block], order: &[usize]) -> Pattern {
        let mut points = Vec::new();
        let mut cmds = Vec::new();
        let mut threads: Vec<Thread> = Vec::new();
        for (n, &k) in order.iter().enumerate() {
            let b = &blocks[k];
            let pts = &p.points[b.start..b.end];
            let cs = &p.cmds[b.start..b.end];
            for (i, (&q, &c)) in pts.iter().zip(cs).enumerate() {
                let c = if i == 0 { if n == 0 { if c == Cmd::ColorChange { Cmd::Jump } else { c } } else { Cmd::ColorChange } } else { c };
                // A thread change happens where the needle is; then it jumps
                // to the block's first point.
                if i == 0 && n > 0 {
                    points.push(*points.last().unwrap_or(&q));
                    cmds.push(Cmd::ColorChange);
                    points.push(q);
                    cmds.push(Cmd::Jump);
                    continue;
                }
                points.push(q);
                cmds.push(c);
            }
            threads.push(b.thread.clone());
        }
        Pattern { format: p.format, points, cmds, threads }
    }

    /// Neighbouring blocks in the same thread joined into one.
    pub fn merge_repeats(p: &Pattern, blocks: &[Block]) -> Pattern {
        let mut q = reorder(p, blocks, &(0..blocks.len()).collect::<Vec<_>>());
        let mut threads: Vec<Thread> = Vec::new();
        let mut k = 0;
        for i in 0..q.cmds.len() {
            if q.cmds[i] == Cmd::ColorChange {
                k += 1;
                if blocks[k].thread == blocks[k - 1].thread {
                    q.cmds[i] = Cmd::Jump;
                    continue;
                }
            }
            if i == 0 || q.cmds[i] == Cmd::ColorChange {
                threads.push(blocks[k].thread.clone());
            }
        }
        q.threads = threads;
        q
    }

    /// Drops stitches shorter than `min` (0.1 mm) that aren't ties after a
    /// jump; returns the new pattern and how many went.
    pub fn remove_short(p: &Pattern, min: f32) -> (Pattern, usize) {
        let mut points: Vec<(f32, f32)> = Vec::with_capacity(p.points.len());
        let mut cmds = Vec::with_capacity(p.cmds.len());
        let mut removed = 0;
        for (i, (&q, &c)) in p.points.iter().zip(&p.cmds).enumerate() {
            if c == Cmd::Stitch
                && i > 0
                && let (Some(&l), Some(&lc)) = (points.last(), cmds.last())
            {
                let d = (q.0 - l.0).hypot(q.1 - l.1);
                if d < min && lc == Cmd::Stitch {
                    removed += 1;
                    continue;
                }
            }
            points.push(q);
            cmds.push(c);
        }
        (Pattern { format: p.format, points, cmds, threads: p.threads.clone() }, removed)
    }
}

impl HoopApp {
    pub(super) fn edit_snapshot(&self) -> Option<Shared> {
        self.stitch_edit.shared.clone()
    }

    /// Puts an undone or redone pattern back.
    pub(super) fn restore_stitches(&mut self, s: &Option<Shared>) {
        if *s == self.stitch_edit.shared {
            return;
        }
        if let (Some(sh), Some(d)) = (s, &mut self.design) {
            d.pattern = (*sh.0).clone();
            d.blocks = d.pattern.blocks();
            d.bounds = d.pattern.bounds();
            self.shown = d.pattern.points.len();
        }
        self.stitch_edit.shared = s.clone();
        self.stitch_edit.picked = None;
    }

    /// Applies an edit to the opened pattern, keeping colors set in the
    /// Colors list.
    fn edit(&mut self, f: impl FnOnce(&Pattern, &[Block]) -> Pattern) {
        if self.mode != Mode::Viewer {
            return;
        }
        let Some(d) = &mut self.design else { return };
        let p = f(&d.pattern, &d.blocks);
        d.pattern = p;
        d.blocks = d.pattern.blocks();
        d.bounds = d.pattern.bounds();
        self.shown = d.pattern.points.len();
        self.focus = None;
        self.stitch_edit.shared = Some(Shared(Rc::new(d.pattern.clone())));
    }

    pub(super) fn move_block(&mut self, k: usize, up: bool) {
        let n = self.design.as_ref().map_or(0, |d| d.blocks.len());
        let j = if up { k.checked_sub(1) } else { (k + 1 < n).then_some(k + 1) };
        let Some(j) = j else { return };
        let mut order: Vec<usize> = (0..n).collect();
        order.swap(k, j);
        self.edit(|p, b| ops::reorder(p, b, &order));
    }

    pub(super) fn delete_block(&mut self, k: usize) {
        let n = self.design.as_ref().map_or(0, |d| d.blocks.len());
        if n <= 1 {
            return;
        }
        let order: Vec<usize> = (0..n).filter(|&i| i != k).collect();
        self.edit(|p, b| ops::reorder(p, b, &order));
    }

    /// The Stitches card of the Viewer's Design tab.
    pub(super) fn stitch_tools_card(&mut self, ui: &mut egui::Ui) {
        if self.mode != Mode::Viewer || self.design.is_none() {
            return;
        }
        let muted = self.muted();
        let accent = self.accent();
        let mut merge = false;
        let mut short = false;
        card(ui, t!("Edit stitches"), accent, |ui| {
            ui.label(RichText::new(t!("Right-click a thread in Colors to move or delete it.")).size(12.0).color(muted));
            ui.horizontal_wrapped(|ui| {
                if ui.button(t!("Merge repeated threads")).on_hover_text(t!("Join neighbouring threads of the same color, saving thread changes")).clicked() {
                    merge = true;
                }
                if ui.button(t!("Remove tiny stitches")).on_hover_text(t!("Drop stitches under 0.3 mm, which can break the thread")).clicked() {
                    short = true;
                }
            });
            let on = self.stitch_edit.on;
            if ui
                .add(egui::Button::selectable(on, t!("Pick stitches on the hoop")))
                .on_hover_text(t!("Click a needle point to pick it, drag to move it, Del to delete it, arrows to nudge; Esc to stop"))
                .clicked()
            {
                self.stitch_edit.on = !on;
                self.stitch_edit.picked = None;
            }
            if self.stitch_edit.on {
                if ui.button(t!("Pick the stitch at the playback position")).clicked() {
                    self.stitch_edit.picked = self.shown.checked_sub(1);
                }
                if let Some(i) = self.stitch_edit.picked {
                    ui.label(RichText::new(t!("Stitch {n} picked", n = i + 1)).size(12.0).color(muted));
                }
            }
        });
        if merge {
            let before = self.design.as_ref().map_or(0, |d| d.blocks.len());
            self.edit(ops::merge_repeats);
            let after = self.design.as_ref().map_or(0, |d| d.blocks.len());
            self.toast(tn!(before - after, "Saved {n} thread change", "Saved {n} thread changes"));
        }
        if short {
            let mut gone = 0;
            self.edit(|p, _| {
                let (q, n) = ops::remove_short(p, 3.0);
                gone = n;
                q
            });
            self.toast(tn!(gone, "Removed {n} tiny stitch", "Removed {n} tiny stitches"));
        }
    }

    /// Right-click menu on a row of the Colors list: what was picked.
    pub(super) fn block_menu(ui: &mut egui::Ui) -> Option<BlockOp> {
        let mut op = None;
        if ui.button(t!("Sew earlier")).clicked() {
            op = Some(BlockOp::Earlier);
        }
        if ui.button(t!("Sew later")).clicked() {
            op = Some(BlockOp::Later);
        }
        if ui.button(t!("Delete this thread's stitches")).clicked() {
            op = Some(BlockOp::Delete);
        }
        if op.is_some() {
            ui.close();
        }
        op
    }

    pub(super) fn block_op(&mut self, k: usize, op: BlockOp) {
        match op {
            BlockOp::Earlier => self.move_block(k, true),
            BlockOp::Later => self.move_block(k, false),
            BlockOp::Delete => self.delete_block(k),
        }
    }

    /// Picking, dragging and deleting single stitches. Returns true while
    /// the mode is on, so the pointer doesn't move the design.
    pub(super) fn stitch_edit_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        if !self.stitch_edit.on || self.mode != Mode::Viewer {
            return false;
        }
        let Some(d) = &self.design else { return false };
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.stitch_edit.on = false;
            self.stitch_edit.picked = None;
            return true;
        }
        let t = Transform::new(d.bounds, &self.setup.at);
        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        let nearest = |p: egui::Pos2| -> Option<usize> {
            d.pattern
                .points
                .iter()
                .enumerate()
                .filter(|(i, _)| d.pattern.cmds[*i] == Cmd::Stitch)
                .map(|(i, &q)| (i, (frame.to_screen_pt(t.apply(q)) - p).length()))
                .filter(|(_, dd)| *dd <= 8.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        if (response.clicked() || response.drag_started())
            && let Some(p) = pointer
        {
            self.stitch_edit.picked = nearest(p);
            self.stitch_edit.dragging = response.drag_started() && self.stitch_edit.picked.is_some();
        }
        let picked = self.stitch_edit.picked;
        if self.stitch_edit.dragging {
            if let (Some(i), Some(p)) = (picked, pointer) {
                let w = t.invert(frame.to_world(p));
                if let Some(d) = &mut self.design {
                    d.pattern.points[i] = (w.0.round(), w.1.round());
                }
            }
            if response.drag_stopped() {
                self.stitch_edit.dragging = false;
                self.edit(|p, _| p.clone());
            }
        }
        if let Some(i) = picked {
            let nudge = ui.input(|inp| {
                let step = if inp.modifiers.shift { 10.0 } else { 1.0 };
                let mut v = (0.0, 0.0);
                for (k, dx, dy) in
                    [(egui::Key::ArrowLeft, -1.0, 0.0), (egui::Key::ArrowRight, 1.0, 0.0), (egui::Key::ArrowUp, 0.0, -1.0), (egui::Key::ArrowDown, 0.0, 1.0)]
                {
                    if inp.key_pressed(k) {
                        v = (v.0 + dx * step, v.1 + dy * step);
                    }
                }
                v
            });
            if nudge != (0.0, 0.0) {
                self.edit(|p, _| {
                    let mut q = p.clone();
                    // Nudges are in hoop directions; turn them back into the design's.
                    let a = t.invert((0.0, 0.0));
                    let b = t.invert(nudge);
                    q.points[i] = (q.points[i].0 + b.0 - a.0, q.points[i].1 + b.1 - a.1);
                    q
                });
            }
            if ui.input(|inp| inp.key_pressed(egui::Key::Delete) || inp.key_pressed(egui::Key::Backspace)) {
                self.edit(|p, _| {
                    let mut q = p.clone();
                    if i < q.points.len() && q.cmds[i] == Cmd::Stitch {
                        q.points.remove(i);
                        q.cmds.remove(i);
                    }
                    q
                });
                self.stitch_edit.picked = None;
            }
        }
        true
    }

    pub(super) fn stitch_edit_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        if !self.stitch_edit.on || self.mode != Mode::Viewer {
            return;
        }
        let Some(d) = &self.design else { return };
        let t = Transform::new(d.bounds, &self.setup.at);
        let accent = self.accent();
        if let Some(i) = self.stitch_edit.picked.filter(|&i| i < d.pattern.points.len()) {
            let p = frame.to_screen_pt(t.apply(d.pattern.points[i]));
            for j in [i.wrapping_sub(1), i + 1] {
                if let Some(&q) = d.pattern.points.get(j) {
                    painter.line_segment([p, frame.to_screen_pt(t.apply(q))], egui::Stroke::new(2.0, accent));
                }
            }
            painter.circle(p, 5.0, accent, egui::Stroke::new(1.5, Color32::WHITE));
        }
        let tip = t!("Click a needle point to pick it · drag to move · Del deletes · Esc stops");
        let galley = painter.layout(tip.into(), egui::FontId::proportional(14.0), Color32::WHITE, frame.rect.width() - 80.0);
        let pos = frame.rect.center_top() + vec2(-galley.size().x / 2.0, 16.0);
        painter.rect_filled(egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0)), 8, Color32::from_black_alpha(190));
        painter.galley(pos, galley, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::ops;
    use crate::formats::{self, Cmd, Format, Pattern};

    fn three() -> Pattern {
        // Red, black, red: three blocks of two stitches each.
        let points = vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (20.0, 0.0), (20.0, 10.0), (20.0, 20.0), (20.0, 20.0), (30.0, 20.0), (40.0, 20.0)];
        let cmds = vec![Cmd::Jump, Cmd::Stitch, Cmd::Stitch, Cmd::ColorChange, Cmd::Stitch, Cmd::Stitch, Cmd::ColorChange, Cmd::Stitch, Cmd::Stitch];
        let threads = vec![formats::jef_thread(10, 0), formats::jef_thread(1, 1), formats::jef_thread(10, 2)];
        Pattern { format: Format::Jef, points, cmds, threads }
    }

    #[test]
    fn reordering_moves_whole_threads() {
        let p = three();
        let q = ops::reorder(&p, &p.blocks(), &[1, 0, 2]);
        let b = q.blocks();
        assert_eq!(b.iter().map(|b| b.thread.name.as_str()).collect::<Vec<_>>(), ["Black", "Red", "Red"]);
        assert_eq!(q.count(Cmd::Stitch), 6);
        // The black stitches come first.
        assert_eq!(q.points[q.cmds.iter().position(|c| *c == Cmd::Stitch).unwrap()], (20.0, 10.0));
        let m = ops::merge_repeats(&q, &b);
        assert_eq!(m.blocks().len(), 2);
        assert_eq!(m.count(Cmd::Stitch), 6);
    }

    #[test]
    fn tiny_stitches_go() {
        let mut p = three();
        p.points.insert(2, (10.5, 0.0));
        p.cmds.insert(2, Cmd::Stitch);
        let (q, n) = ops::remove_short(&p, 3.0);
        assert_eq!(n, 1);
        assert_eq!(q.count(Cmd::Stitch), 6);
    }
}
