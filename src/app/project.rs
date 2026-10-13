//! Saving and opening Hoop projects (.hoop), the window title that names
//! the project and marks unsaved changes, and the question on closing with
//! unsaved work.

use super::*;
use crate::project::{PinnedPart, Project, VERSION};

/// What closing the window waits for.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Closing {
    #[default]
    No,
    /// Asking whether to save.
    Asking,
    /// Saving first; close once the project is written.
    AfterSave,
    /// The answer was not to save.
    Allowed,
}

impl HoopApp {
    /// Whether there is Artwork or Lettering work a project could keep.
    pub(super) fn has_project_work(&self) -> bool {
        match self.mode {
            Mode::Artwork => self.art.is_some(),
            Mode::Lettering => !self.letter.text.trim().is_empty(),
            Mode::Viewer => false,
        }
    }

    /// Changes since the project was opened, saved or started.
    pub(super) fn unsaved(&self) -> bool {
        self.has_project_work() && self.snapshot() != self.clean
    }

    fn make_project(&self) -> Option<Project> {
        let (artwork, art, letter, font) = match self.mode {
            Mode::Artwork => (Some(self.artwork.clone()?), Some(self.art.clone()?), None, String::new()),
            Mode::Lettering => (None, None, Some(self.letter.clone()), self.fonts.get(self.letter.font).map_or(String::new(), |f| f.name.clone())),
            Mode::Viewer => return None,
        };
        Some(Project {
            version: VERSION,
            mode: if self.mode == Mode::Artwork { "artwork" } else { "lettering" }.into(),
            machine: hoops::machine().key.into(),
            hoop: self.setup.hoop,
            at: self.setup.at,
            artwork,
            art,
            letter,
            font,
            pinned: self.pinned.iter().map(|p| PinnedPart { name: p.name.clone(), pattern: p.pattern.clone() }).collect(),
        })
    }

    /// A name for a new project file.
    fn project_name(&self) -> String {
        let stem = match self.mode {
            Mode::Artwork => self.artwork.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            _ => self.letter.text.lines().next().unwrap_or("").to_string(),
        };
        let stem = std::path::Path::new(&stem).file_stem().map(|s| s.to_string_lossy().trim().to_string()).unwrap_or_default();
        format!("{}.hoop", if stem.is_empty() { "project".into() } else { stem })
    }

    /// Saves the project where it was opened or last saved; asks where when
    /// it is new or `ask` is set.
    pub(super) fn save_project(&mut self, ask: bool) {
        let Some(p) = self.make_project() else {
            self.toast(t!("Projects keep Artwork and Lettering work. A stitch file is saved with Save.").into());
            return;
        };
        let bytes = p.to_bytes();
        match self.project_path.clone().filter(|_| self.project_mode == self.mode) {
            Some(path) if !ask => self.write_file(path, &bytes, "hoop"),
            _ => self.ask_save(t!("Save project"), self.project_name(), t!("Hoop project"), "hoop", bytes),
        }
    }

    /// After a project was written: it is now the project's file, and
    /// nothing is unsaved.
    pub(super) fn project_saved(&mut self, path: &Path) {
        self.project_path = Some(path.to_path_buf());
        self.project_mode = self.mode;
        self.clean = self.snapshot();
        self.prefs.add_recent(path);
        if self.closing == Closing::AfterSave {
            self.closing = Closing::Allowed;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn open_project(&mut self, path: &Path) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let p = match std::fs::read(path).map_err(|e| t!("Couldn't open {name}: {e}.", name = name, e = e)).and_then(|d| Project::from_bytes(&d)) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        if let Some(i) = hoops::MACHINES.iter().position(|m| m.key == p.machine)
            && i != hoops::machine_index()
        {
            hoops::set_machine(i);
            self.prefs.set("machine", p.machine.as_str());
            self.toast(t!("The project is for {v0}; Hoop switched to it.", v0 = hoops::machine().name));
        }
        self.pinned = p
            .pinned
            .into_iter()
            .map(|part| {
                let blocks = part.pattern.blocks();
                let bounds = part.pattern.bounds();
                Pinned { name: part.name, pattern: part.pattern, blocks, bounds }
            })
            .collect();
        match (p.mode.as_str(), p.artwork, p.art) {
            ("artwork", Some(artwork), Some(art)) => {
                self.picture = None;
                self.use_artwork(Ok(artwork));
                self.art = Some(art);
            }
            _ => {
                let mut letter = p.letter.unwrap_or_default();
                letter.font = self.fonts.iter().position(|f| f.name == p.font).unwrap_or(0);
                self.letter = letter;
                self.set_mode(Mode::Lettering);
            }
        }
        self.setup = Setup { hoop: p.hoop.min(hoops::list().len().saturating_sub(1)), at: p.at };
        self.sel.clear();
        self.reset_history();
        self.clean = self.snapshot();
        self.project_path = Some(path.to_path_buf());
        self.project_mode = self.mode;
        self.prefs.add_recent(path);
        self.fit_pending = true;
        self.user_view = false;
    }

    /// Sets the window title: the project's file, with a dot while
    /// unsaved, or else what the open design asks for.
    pub(super) fn project_title(&mut self, ctx: &egui::Context) {
        let title = match self.project_path.as_ref().filter(|_| self.project_mode == self.mode) {
            Some(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if self.unsaved() { t!("● {name} — Hoop", name = name) } else { t!("{name} — Hoop", name = name) }
            }
            None => self.title_base.clone(),
        };
        if self.title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Holds a close while there is unsaved work, and asks what to do.
    pub(super) fn guard_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.closing != Closing::Allowed && self.unsaved() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = Closing::Asking;
        }
        if self.closing != Closing::Asking {
            return;
        }
        let mut answer = None;
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(t!("Save your work first?"));
            ui.add_space(4.0);
            let what = match &self.project_path {
                Some(p) => t!("{v0} has changes that aren't saved.", v0 = p.file_name().unwrap_or_default().to_string_lossy()),
                None => t!("This work isn't saved as a project yet. Without one, the shapes and settings are lost when Hoop closes.").to_string(),
            };
            ui.label(what);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(primary(t!("Save project"), self.accent())).clicked() {
                    answer = Some(Closing::AfterSave);
                }
                if ui.button(t!("Don't save")).clicked() {
                    answer = Some(Closing::Allowed);
                }
                if ui.button(t!("Cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(Closing::No);
                }
            });
        });
        match answer {
            Some(Closing::AfterSave) => {
                self.closing = Closing::AfterSave;
                self.save_project(false);
            }
            Some(Closing::Allowed) => {
                self.closing = Closing::Allowed;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(_) => self.closing = Closing::No,
            None => {}
        }
    }
}
