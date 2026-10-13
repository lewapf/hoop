//! The command palette (Ctrl+K): every command by name, found by typing.

use super::shapes::Arrange;
use super::*;

type Run = Box<dyn Fn(&mut HoopApp, &egui::Context, egui::Rect)>;

/// A command: its name, its keys (shown, not bound here), whether it can
/// run now, and what it does.
struct Command {
    name: String,
    keys: &'static str,
    enabled: bool,
    run: Run,
}

fn cmd(name: &str, keys: &'static str, enabled: bool, run: impl Fn(&mut HoopApp, &egui::Context, egui::Rect) + 'static) -> Command {
    Command { name: name.to_string(), keys, enabled, run: Box::new(run) }
}

#[derive(Default)]
pub(super) struct Palette {
    pub open: bool,
    pub query: String,
    picked: usize,
}

impl Palette {
    /// Opens the palette empty.
    pub fn show(&mut self) {
        *self = Palette { open: true, ..Default::default() };
    }
}

impl HoopApp {
    fn commands(&mut self) -> Vec<Command> {
        let has = self.design.is_some();
        let out = has || !self.pinned.is_empty();
        let picked = self.mode == Mode::Artwork && !self.sel.is_empty();
        let format = hoops::machine().out.ext().to_uppercase();
        let mut list = vec![
            cmd(t!("New project…"), "N", true, |a, _, _| a.open_wizard()),
            cmd(t!("Open…"), "O", true, |a, _, _| a.pick_file()),
            cmd(&t!("Save as {format}…", format = format), "Ctrl+S", has, |a, _, _| a.save_design()),
            cmd(t!("Save project"), "Ctrl+Alt+S", out, |a, _, _| a.save_project(false)),
            cmd(t!("Save picture…"), "Ctrl+Shift+S", has, |a, _, _| a.save_picture()),
            cmd(t!("Save worksheet (PDF)…"), "Ctrl+P", has, |a, _, _| a.save_worksheet()),
            cmd(t!("Send to USB…"), "Ctrl+U", out, |a, _, _| a.open_usb()),
            cmd(t!("Test stitch-out…"), "Ctrl+T", true, |a, _, _| a.test.open = true),
            cmd(t!("Split into hoopings…"), "", out, |a, _, _| a.open_hoopings()),
            cmd(t!("Undo"), "Ctrl+Z", !self.undo.is_empty() || self.pending_since.is_some(), |a, ctx, _| a.undo_step(ctx)),
            cmd(t!("Redo"), "Ctrl+Shift+Z", !self.redo.is_empty(), |a, _, _| a.redo_step()),
            cmd(t!("Rotate right"), "R", has, |a, _, _| a.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4)),
            cmd(t!("Rotate left"), "Shift+R", has, |a, _, _| a.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4)),
            cmd(t!("Mirror"), "M", has, |a, _, _| a.change(|s| s.at.mirror = !s.at.mirror)),
            cmd(t!("Center in hoop"), "C", has, |a, _, _| {
                a.change(|s| {
                    s.at.dx = 0.0;
                    s.at.dy = 0.0;
                })
            }),
            cmd(t!("Pin to hoop"), "P", has, |a, _, _| a.pin()),
            cmd(t!("Whole hoop"), "F", true, |a, _, r| a.fit_view(r, false)),
            cmd(t!("Zoom to design"), "D", has, |a, _, r| a.fit_view(r, true)),
            cmd(t!("Play or pause the stitch-out"), "Space", has, |a, _, _| a.toggle_play()),
            cmd(t!("Next thread"), "]", has, |a, _, _| a.jump_thread(true)),
            cmd(t!("Previous thread"), "[", has, |a, _, _| a.jump_thread(false)),
            cmd(t!("Template grid"), "G", true, |a, _, _| a.grid = !a.grid),
            cmd(t!("Jump stitches"), "J", true, |a, _, _| a.jumps = !a.jumps),
            cmd(t!("Thread texture"), "T", true, |a, _, _| a.textured = !a.textured),
            cmd(t!("Thread density"), "H", true, |a, _, _| a.density = !a.density),
            cmd(t!("Hide or show the thread renders under the hoop"), "F8", true, |a, _, _| a.timeline_open = !a.timeline_open),
            cmd(t!("Hide or show the left / right panel"), "F9 / F10", true, |a, _, _| {
                let hide = a.show_left || a.show_right;
                a.show_left = !hide;
                a.show_right = !hide;
            }),
            cmd(t!("Viewer"), "V", true, |a, _, _| a.set_mode(Mode::Viewer)),
            cmd(t!("Lettering"), "L", true, |a, _, _| a.set_mode(Mode::Lettering)),
            cmd(t!("Artwork"), "A", true, |a, _, _| a.set_mode(Mode::Artwork)),
            cmd(t!("Settings…"), "Ctrl+,", true, |a, _, _| a.settings.open = true),
            cmd(t!("Thread charts and my threads…"), "", true, |a, _, _| a.threads.open = true),
            cmd(t!("Google Fonts…"), "", true, |a, _, _| a.fontwin.show()),
            cmd(t!("User manual"), "F1", true, |a, _, _| a.manual.open = true),
            cmd(t!("Keyboard shortcuts"), "?", true, |a, _, _| a.help = true),
            cmd(t!("Play demo"), "", true, |a, _, _| a.start_demo()),
        ];
        if self.mode == Mode::Artwork {
            for (name, keys, op) in [
                (t!("Duplicate"), "Ctrl+D", Arrange::Duplicate),
                (t!("Delete"), "Del", Arrange::Delete),
                (t!("Group"), "Ctrl+G", Arrange::Group),
                (t!("Bring to front"), "Shift+]", Arrange::Front),
                (t!("Send to back"), "Shift+[", Arrange::Back),
                (t!("Hide"), "", Arrange::Hide),
            ] {
                list.push(cmd(name, keys, picked, move |a, _, _| a.arrange(op)));
            }
            list.push(cmd(t!("Select all"), "Ctrl+A", self.art.is_some(), |a, _, _| a.arrange(Arrange::SelectAll)));
            list.push(cmd(t!("Show all shapes"), "", self.art.is_some(), |a, _, _| a.arrange(Arrange::ShowAll)));
        }
        // Every page of both docks, to go straight to it.
        for &tab in shell::tabs(self.mode) {
            list.push(cmd(&t!("Go to {page}", page = tab.label()), "", true, move |a, _, _| {
                a.tab[a.mode as usize] = tab;
                a.show_right = true;
            }));
        }
        for (dock, label, _, _) in self.left_tabs() {
            list.push(cmd(&t!("Go to {page}", page = label), "", true, move |a, _, _| {
                a.dock = dock;
                a.show_left = true;
            }));
        }
        list
    }

    /// Opens on Ctrl+K and runs the command picked with Enter or a click.
    pub(super) fn palette(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::K))) {
            if self.palette.open {
                self.palette.open = false;
            } else {
                self.palette.show();
            }
        }
        if !self.palette.open {
            return;
        }
        let query = self.palette.query.clone();
        let list: Vec<Command> = self.commands().into_iter().filter(|c| matches(&c.name, &query)).collect();
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let last = list.len().saturating_sub(1);
        if up {
            self.palette.picked = self.palette.picked.saturating_sub(1);
        }
        if down {
            self.palette.picked = (self.palette.picked + 1).min(last);
        }
        self.palette.picked = self.palette.picked.min(last);
        let mut run = enter.then_some(self.palette.picked);
        let accent = self.accent();
        let muted = self.muted();
        let area = egui::Modal::default_area(egui::Id::new("palette")).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 90.0));
        let modal = egui::Modal::new(egui::Id::new("palette")).area(area).backdrop_color(Color32::from_black_alpha(70)).show(ctx, |ui| {
            ui.set_width(460.0);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.palette.query)
                    .hint_text(t!("Type a command…"))
                    .desired_width(f32::INFINITY)
                    .font(egui::FontId::proportional(15.0)),
            );
            edit.request_focus();
            if edit.changed() {
                self.palette.picked = 0;
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                if list.is_empty() {
                    ui.label(RichText::new(t!("No command matches")).color(muted));
                }
                for (k, c) in list.iter().enumerate() {
                    let on = k == self.palette.picked;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), if c.enabled { Sense::click() } else { Sense::hover() });
                    if on || resp.hovered() {
                        ui.painter().rect_filled(rect, 5, if on { accent.gamma_multiply(0.22) } else { ui.visuals().widgets.hovered.weak_bg_fill });
                    }
                    if on && (up || down) {
                        ui.scroll_to_rect(rect, None);
                    }
                    let ink = if c.enabled { ui.visuals().text_color() } else { ui.visuals().text_color().gamma_multiply(0.35) };
                    let p = ui.painter();
                    p.text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, &c.name, egui::FontId::proportional(13.5), ink);
                    p.text(rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, c.keys, egui::FontId::monospace(12.0), muted);
                    if resp.clicked() {
                        run = Some(k);
                    }
                }
            });
        });
        if escape || modal.should_close() {
            self.palette.open = false;
        }
        if let Some(c) = run.and_then(|k| list.get(k)).filter(|c| c.enabled) {
            self.palette.open = false;
            (c.run)(self, ctx, canvas_rect);
        }
    }
}

/// Whether every word typed is somewhere in the name, in any case.
fn matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| name.contains(w))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn matches_every_word_in_any_order_and_case() {
        assert!(matches("Save worksheet (PDF)…", ""));
        assert!(matches("Save worksheet (PDF)…", "pdf save"));
        assert!(matches("Zoom to design", "ZOOM"));
        assert!(!matches("Zoom to design", "zoom hoop"));
    }
}
