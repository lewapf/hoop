//! The Settings window (Edit → Settings, Ctrl+,): everything that applies
//! to the whole app in one place. Values live in the prefs file like the
//! rest of what Hoop remembers.

use std::path::PathBuf;

use super::*;

#[derive(Default)]
pub(super) struct SettingsWindow {
    pub open: bool,
    folder_pick: Option<Receiver<Option<PathBuf>>>,
}

fn open_in_files(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

impl HoopApp {
    /// Satin column limit from the settings, for new lettering and artwork.
    pub(super) fn column_max(&self) -> f32 {
        self.prefs.num::<f32>("column_max").filter(|v| (3.0..=12.0).contains(v)).unwrap_or(7.0)
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings.open {
            return;
        }
        if let Some(rx) = &self.settings.folder_pick {
            if let Ok(answer) = rx.try_recv() {
                self.settings.folder_pick = None;
                if let Some(dir) = answer {
                    self.prefs.set("library.dir", dir.display());
                    self.library.dir = None;
                }
            }
        }
        let muted = self.muted();
        let accent = self.accent();
        let mut open = true;
        let mut pick_folder = false;
        let mut reset = false;
        egui::Window::new(t!("Settings")).open(&mut open).collapsible(false).resizable(false).default_width(420.0).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(420.0);
            egui::Grid::new("settings").num_columns(2).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                // Language
                ui.label(t!("Language"));
                let chosen = self.prefs.get("language").unwrap_or("system").to_string();
                let system = crate::i18n::system_language();
                let mut options: Vec<(String, String)> = vec![
                    ("system".into(), t!("System ({lang})", lang = if system.is_empty() { "en".to_string() } else { system })),
                    ("en".into(), "English".into()),
                ];
                options.extend(crate::i18n::available());
                let current = options.iter().find(|o| o.0 == chosen).map_or(chosen.clone(), |o| o.1.clone());
                egui::ComboBox::from_id_salt("set-language").selected_text(current).width(220.0).show_ui(ui, |ui| {
                    for (code, name) in &options {
                        if ui.selectable_label(chosen == *code, name).clicked() {
                            crate::i18n::set_language(code);
                            self.prefs.set("language", code);
                        }
                    }
                });
                ui.end_row();

                // Designs folder
                ui.label(t!("Designs folder"));
                ui.horizontal(|ui| {
                    let dir = self.prefs.get("library.dir").map(PathBuf::from).unwrap_or_else(library::designs_dir);
                    ui.add(egui::Label::new(RichText::new(dir.display().to_string()).size(12.0)).truncate()).on_hover_text(dir.display().to_string());
                    if ui.button(t!("Change…")).clicked() {
                        pick_folder = true;
                    }
                });
                ui.end_row();

                // Machine
                ui.label(t!("Machine speed"));
                egui::ComboBox::from_id_salt("set-spm").selected_text(t!("{spm} spm", spm = SPEEDS[self.speed])).width(120.0).show_ui(ui, |ui| {
                    for (i, s) in SPEEDS.iter().enumerate() {
                        ui.selectable_value(&mut self.speed, i, t!("{spm} spm", spm = s));
                    }
                });
                ui.end_row();

                // Threads
                ui.label(t!("Thread chart"));
                ui.horizontal(|ui| {
                    let before = self.threads.chosen;
                    egui::ComboBox::from_id_salt("set-chart").selected_text(&self.chart().name).width(180.0).show_ui(ui, |ui| {
                        for (i, c) in self.threads.charts.iter().enumerate() {
                            ui.selectable_value(&mut self.threads.chosen, i, &c.name);
                        }
                    });
                    if before != self.threads.chosen {
                        let name = self.chart().name.clone();
                        self.prefs.set("threads.chart", name);
                    }
                    if ui.button(t!("My threads…")).clicked() {
                        self.threads.open = true;
                    }
                });
                ui.end_row();

                // Stitching defaults
                ui.label(t!("Satin columns up to"));
                let mut max = self.column_max();
                if ui.add(egui::Slider::new(&mut max, 3.0..=12.0).suffix(" mm").fixed_decimals(1)).on_hover_text(t!("Auto sews parts this wide or narrower as satin columns, wider ones as tatami")).changed() {
                    self.prefs.set("column_max", max);
                    self.letter.column_max_mm = max;
                    if let Some(a) = &mut self.art {
                        a.style.column_max_mm = max;
                    }
                }
                ui.end_row();

                // Display
                ui.label(t!("Display"));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("Fabric")).color(muted));
                        ui.color_edit_button_srgb(&mut self.fabric);
                    });
                    ui.checkbox(&mut self.textured, t!("Thread texture"));
                    ui.checkbox(&mut self.grid, t!("Template grid, 10 mm"));
                    ui.checkbox(&mut self.jumps, t!("Jump stitches"));
                    ui.checkbox(&mut self.sizes, t!("Size label under the design"));
                });
                ui.end_row();

                // Test sheet
                ui.label(t!("Test sheet"));
                ui.horizontal(|ui| {
                    let text = if self.tuning.is_empty() { t!("Not sewn yet, so Hoop uses general rules.").to_string() } else { t!("Using your test sheet results.").to_string() };
                    ui.label(RichText::new(text).size(12.0).color(muted));
                    if ui.button(t!("Open…")).clicked() {
                        self.test.open = true;
                    }
                });
                ui.end_row();

                // Files
                ui.label(t!("Folders"));
                ui.vertical(|ui| {
                    if ui.button(t!("Translations folder")).on_hover_text(t!("Put <language>.po files here to add or fix a translation")).clicked() {
                        if let Some(d) = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/hoop/locale")) {
                            open_in_files(&d);
                        }
                    }
                    if ui.button(t!("Clear picture cache")).on_hover_text(t!("Library thumbnails are drawn again")).clicked() {
                        if let Some(d) = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/hoop/thumbs")) {
                            let _ = std::fs::remove_dir_all(d);
                        }
                        self.library = Default::default();
                    }
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(primary(t!("Done"), accent)).clicked() {
                    self.settings.open = false;
                }
                if ui.button(t!("Reset all settings")).on_hover_text(t!("Back to how Hoop started; recent files are kept")).clicked() {
                    reset = true;
                }
            });
        });
        if !open {
            self.settings.open = false;
        }
        // Saved when the window closes, not every frame.
        let closing = !self.settings.open;
        if pick_folder && self.settings.folder_pick.is_none() {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            let dir = library::designs_dir();
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new().set_title(t!("Designs folder")).set_directory(dir).pick_folder());
                ctx.request_repaint();
            });
            self.settings.folder_pick = Some(rx);
        }
        if reset {
            self.prefs.reset_keep_recent();
            crate::i18n::set_language("system");
            self.fabric = FABRICS[0].1;
            (self.grid, self.jumps, self.textured, self.sizes) = (true, false, true, true);
            self.speed = 2;
            self.letter = Settings::default();
            self.library = Default::default();
            self.threads.chosen = 0;
            self.toast(t!("Settings are back to the start.").into());
        }
        if closing || reset {
            self.store();
        }
    }
}
