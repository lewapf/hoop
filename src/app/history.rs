//! Undo and redo: snapshots of everything undo covers, committed once a
//! change settles.

use super::*;

impl HoopApp {
    pub(super) fn change(&mut self, f: impl FnOnce(&mut Setup)) {
        f(&mut self.setup);
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot { setup: self.setup, letter: self.letter.clone(), art: self.art.clone(), stitches: self.edit_snapshot() }
    }

    pub(super) fn restore_snapshot(&mut self, s: Snapshot) {
        self.restore_stitches(&s.stitches);
        self.setup = s.setup;
        self.letter = s.letter.clone();
        if s.art.as_ref().map(|a| a.layers.len()) == self.art.as_ref().map(|a| a.layers.len()) {
            self.keep_anchor = s.art != self.art;
            self.art = s.art.clone();
            let n = self.art.as_ref().map_or(0, |a| a.shapes.len());
            self.sel.retain(|&i| i < n);
        }
        self.committed = self.snapshot();
        self.pending_since = None;
    }

    /// Starts a fresh history, for a newly opened design.
    pub(super) fn reset_history(&mut self) {
        self.stitch_edit = Default::default();
        self.undo.clear();
        self.redo.clear();
        self.committed = self.snapshot();
        // Fresh work counts as saved until it is changed.
        self.clean = self.committed.clone();
        self.recovered = false;
        self.pending_since = None;
    }

    /// Turns settled changes into undo steps. A change counts once the
    /// mouse is up and nothing changed for half a second, so dragging a
    /// slider or typing a word is one step.
    pub(super) fn track_history(&mut self, ctx: &egui::Context, force: bool) {
        let now = self.snapshot();
        if now == self.committed {
            self.pending_since = None;
            return;
        }
        let since = *self.pending_since.get_or_insert_with(std::time::Instant::now);
        let settled = since.elapsed().as_millis() > 500 && !ctx.input(|i| i.pointer.any_down());
        if force || settled {
            self.undo.push(std::mem::replace(&mut self.committed, now));
            self.redo.clear();
            self.pending_since = None;
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(550));
        }
    }

    pub(super) fn undo_step(&mut self, ctx: &egui::Context) {
        self.track_history(ctx, true);
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore_snapshot(s);
            self.toast(t!("Undone").into());
        }
    }

    pub(super) fn redo_step(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore_snapshot(s);
            self.toast(t!("Redone").into());
        }
    }
}
