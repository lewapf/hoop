//! Library page of the left dock: browse a designs folder by picture and
//! open a design with a click.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Color32, RichText, Sense, vec2};

use super::HoopApp;
use crate::library::{self, Entry, Kind, Painter, SIZE};

#[derive(Default)]
pub(super) struct Library {
    pub dir: Option<PathBuf>,
    entries: Vec<Entry>,
    /// None while a thumbnail is being drawn or when it can't be.
    thumbs: HashMap<PathBuf, Option<egui::TextureHandle>>,
    painter: Option<Painter>,
    filter: String,
}

/// The folder designs live in: ~/Documents/Embroidery Designs, else
/// ~/Documents, else home.
pub(super) fn designs_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    [home.join("Documents/Embroidery Designs"), home.join("Documents"), home.clone()].into_iter().find(|p| p.is_dir()).unwrap_or(home)
}

impl HoopApp {
    fn library_go(&mut self, dir: PathBuf) {
        self.library.entries = library::scan(&dir);
        self.library.dir = Some(dir);
        self.library.filter.clear();
    }

    pub(super) fn library_page(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        if self.library.dir.is_none() {
            let dir = self.prefs.get("library.dir").map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(designs_dir);
            self.library_go(dir);
        }
        if self.library.painter.is_none() {
            let ctx = ui.ctx().clone();
            self.library.painter = Some(Painter::start(move || ctx.request_repaint()));
        }
        // Take finished thumbnails.
        if let Some(p) = &self.library.painter {
            while let Ok((path, px)) = p.done.try_recv() {
                let tex = px.map(|px| {
                    let img = egui::ColorImage::from_rgba_unmultiplied([SIZE as usize, SIZE as usize], &px);
                    ui.ctx().load_texture(format!("thumb:{}", path.display()), img, egui::TextureOptions::LINEAR)
                });
                self.library.thumbs.insert(path, tex);
            }
        }
        let dir = self.library.dir.clone().unwrap();
        let mut go: Option<PathBuf> = None;
        ui.horizontal(|ui| {
            if ui.add_enabled(dir.parent().is_some(), egui::Button::new("Up")).on_hover_text(t!("The folder above")).clicked() {
                go = dir.parent().map(|p| p.to_path_buf());
            }
            if ui.button(t!("Folder…")).on_hover_text(t!("Pick the folder to browse")).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_directory(&dir).pick_folder() {
                    go = Some(d);
                }
            }
            if ui.button(t!("Reload")).clicked() {
                go = Some(dir.clone());
            }
        });
        let name = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned());
        ui.add(egui::Label::new(RichText::new(name).strong()).truncate()).on_hover_text(dir.display().to_string());
        ui.add(egui::TextEdit::singleline(&mut self.library.filter).hint_text(t!("Find by name")).desired_width(ui.available_width()));
        ui.add_space(4.0);

        let needle = self.library.filter.to_lowercase();
        let shown: Vec<Entry> = self.library.entries.iter().filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle)).cloned().collect();
        if shown.is_empty() {
            ui.label(RichText::new(t!("No designs here. Hoop shows JEF, DST, EXP, PES, PEC, SVG and pictures.")).size(12.0).color(muted));
        }
        let open_now = self.design.as_ref().map(|d| d.name.clone());
        let mut open: Option<PathBuf> = None;
        egui::ScrollArea::vertical().id_salt("library").auto_shrink([false, false]).show(ui, |ui| {
            let gap = 6.0;
            let cols = ((ui.available_width() + gap) / (110.0 + gap)).floor().max(1.0) as usize;
            let tile = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32 - 1.0;
            for row in shown.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for e in row {
                        let (rect, resp) = ui.allocate_exact_size(vec2(tile, tile + 18.0), Sense::click());
                        let pic = egui::Rect::from_min_size(rect.min, vec2(tile, tile));
                        let p = ui.painter();
                        let bg = ui.visuals().extreme_bg_color;
                        let current = open_now.as_deref() == Some(e.name.as_str());
                        p.rect_filled(pic, 4, bg);
                        if resp.hovered() || current {
                            p.rect_stroke(pic, 4, egui::Stroke::new(if current { 2.0 } else { 1.0 }, accent), egui::StrokeKind::Inside);
                        }
                        match e.kind {
                            Kind::Folder => {
                                // A plain folder shape.
                                let c = muted.gamma_multiply(0.7);
                                let body = egui::Rect::from_center_size(pic.center() + vec2(0.0, 4.0), vec2(tile * 0.5, tile * 0.36));
                                let tab = egui::Rect::from_min_size(body.min - vec2(0.0, tile * 0.07), vec2(tile * 0.2, tile * 0.08));
                                p.rect_filled(tab, 2, c);
                                p.rect_filled(body, 3, c);
                            }
                            _ => match self.library.thumbs.get(&e.path) {
                                Some(Some(tex)) => {
                                    p.image(tex.id(), pic.shrink(4.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                                }
                                Some(None) => {}
                                None => {
                                    if ui.is_rect_visible(rect) {
                                        self.library.thumbs.insert(e.path.clone(), None);
                                        if let Some(painter) = &self.library.painter {
                                            painter.request(e.path.clone());
                                        }
                                    }
                                }
                            },
                        }
                        if e.kind != Kind::Folder {
                            // File type in the corner, since a JEF and its SVG often share a name.
                            let ext = e.path.extension().map(|x| x.to_string_lossy().to_uppercase()).unwrap_or_default();
                            let g = ui.painter().layout_no_wrap(ext, egui::FontId::monospace(9.0), muted);
                            let r = egui::Rect::from_min_size(pic.right_top() + vec2(-g.size().x - 7.0, 3.0), g.size() + vec2(4.0, 1.0));
                            ui.painter().rect_filled(r, 2, bg);
                            ui.painter().galley(r.min + vec2(2.0, 0.5), g, muted);
                        }
                        let label = egui::Rect::from_min_size(pic.left_bottom(), vec2(tile, 18.0));
                        let text = egui::WidgetText::from(RichText::new(&e.name).size(11.0)).into_galley(ui, Some(egui::TextWrapMode::Truncate), tile, egui::TextStyle::Body);
                        ui.painter().galley(label.center_top() - vec2(text.size().x / 2.0, -2.0), text, ui.visuals().text_color());
                        let resp = resp.on_hover_text(&e.name);
                        if resp.clicked() {
                            if e.kind == Kind::Folder {
                                go = Some(e.path.clone());
                            } else {
                                open = Some(e.path.clone());
                            }
                        }
                    }
                });
                ui.add_space(gap);
            }
        });
        if let Some(d) = go {
            self.prefs.set("library.dir", d.display());
            self.library_go(d);
        }
        if let Some(p) = open {
            self.open_later = Some(p);
        }
    }
}
