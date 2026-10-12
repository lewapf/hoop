//! Team names: one design per name with the current lettering, saved as
//! separate JEF files in a folder, like the team-name tools of digitizing
//! programs.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::RichText;

use super::{HoopApp, card_folded, primary};
use crate::canvas::Transform;
use crate::hoops::Placement;
use crate::lettering::{self, Settings};

#[derive(Default)]
pub(super) struct Team {
    names: String,
    job: Option<Receiver<Option<PathBuf>>>,
}

/// A file name for a name: letters, digits, spaces as underscores.
fn file_stem(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "name".into() } else { s }
}

impl HoopApp {
    fn team_list(&self) -> Vec<String> {
        self.team.names.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    }

    pub(super) fn team_card(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        let count = self.team_list().len();
        let mut save = false;
        card_folded(ui, t!("Team names"), accent, |ui| {
            ui.label(
                RichText::new(t!("One name per line. Each is sewn with the lettering settings above and saved as its own JEF, ready for a stack of shirts."))
                    .size(12.0)
                    .color(muted),
            );
            ui.add(egui::TextEdit::multiline(&mut self.team.names).desired_rows(4).desired_width(f32::INFINITY).hint_text(t!("ANNA\nBEN\nCHARLOTTE")));
            let busy = self.team.job.is_some();
            let label = tn!(count, "Save {n} design…", "Save {n} designs…");
            if ui.add_enabled(count > 0 && !busy, primary(&label, accent)).on_hover_text(t!("Pick a folder; each name becomes NAME.jef")).clicked() {
                save = true;
            }
        });
        if save {
            let (tx, rx) = channel();
            let ctx = ui.ctx().clone();
            let dir = super::library::designs_dir();
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new().set_title(t!("Folder for the team names")).set_directory(dir).pick_folder());
                ctx.request_repaint();
            });
            self.team.job = Some(rx);
        }
    }

    /// Writes the team designs once a folder has been picked.
    pub(super) fn poll_team(&mut self) {
        let Some(rx) = &self.team.job else { return };
        let Ok(folder) = rx.try_recv() else { return };
        self.team.job = None;
        let Some(folder) = folder else { return };
        let font = &self.fonts[self.letter.font.min(self.fonts.len() - 1)];
        let mut saved = 0;
        let mut failed = Vec::new();
        for name in self.team_list() {
            let s = Settings { text: name.clone(), ..self.letter.clone() };
            let bytes = lettering::generate(font, &s).map(|p| {
                let at = Placement { dx: 0.0, dy: 0.0, ..self.setup.at };
                let t = Transform::new(p.bounds(), &at);
                let points: Vec<(f32, f32)> = p.points.iter().map(|&q| t.apply(q)).collect();
                let threads: Vec<_> = p.blocks().into_iter().map(|b| b.thread).collect();
                crate::formats::write_jef(&points, &p.cmds, &crate::export::jef_numbers(&threads))
            });
            match bytes.map_err(|e| e.to_string()).and_then(|b| std::fs::write(folder.join(format!("{}.jef", file_stem(&name))), b).map_err(|e| e.to_string()))
            {
                Ok(()) => saved += 1,
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        if failed.is_empty() {
            self.toast(tn!(saved, "Saved {n} design to {folder}", "Saved {n} designs to {folder}", folder = folder.display()));
        } else {
            self.error = Some(t!("Couldn't save: {list}", list = failed.join("; ")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::file_stem;

    #[test]
    fn names_become_safe_file_names() {
        assert_eq!(file_stem("Anna-Lena"), "Anna-Lena");
        assert_eq!(file_stem("Zoë M."), "Zoë_M");
        assert_eq!(file_stem("///"), "name");
    }
}
