//! Thread charts window: pick the brand the thread lists use, fetch more
//! charts, and tick the spools in "My threads".

use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, RichText, Sense, Vec2, vec2};

use super::{HoopApp, contrast, primary, rgb};
use crate::formats::Thread;
use crate::threads::{self, Chart};

pub(super) struct ThreadCharts {
    pub open: bool,
    pub charts: Vec<Chart>,
    /// Index into `charts` of the chart thread lists use.
    pub chosen: usize,
    filter: String,
    only_mine: bool,
    job: Option<Receiver<Result<String, String>>>,
    message: Option<Result<String, String>>,
}

impl Default for ThreadCharts {
    fn default() -> Self {
        ThreadCharts { open: false, charts: threads::load_all(), chosen: 0, filter: String::new(), only_mine: false, job: None, message: None }
    }
}

impl HoopApp {
    pub(super) fn chart(&self) -> &Chart {
        &self.threads.charts[self.threads.chosen.min(self.threads.charts.len() - 1)]
    }

    /// Thread codes ticked in "My threads" for the chosen chart.
    pub(super) fn my_threads(&self) -> Vec<String> {
        self.prefs
            .get(&format!("threads.own.{}", self.chart().name))
            .map(|s| s.split(',').filter(|c| !c.is_empty()).map(String::from).collect())
            .unwrap_or_default()
    }

    /// The thread to buy or load for a design color, when it differs from
    /// what the file says: another brand is chosen, or "My threads" has a
    /// near match.
    pub(super) fn chart_thread(&self, t: &Thread) -> Option<Thread> {
        let own = self.my_threads();
        if self.threads.chosen == 0 && (own.is_empty() || own.contains(&threads::key(t))) && self.chart().threads.contains(t) {
            return None;
        }
        threads::nearest(self.chart(), t.color, &own).cloned()
    }

    pub(super) fn restore_chart(&mut self) {
        if let Some(name) = self.prefs.get("threads.chart")
            && let Some(i) = self.threads.charts.iter().position(|c| c.name == name)
        {
            self.threads.chosen = i;
        }
    }

    pub(super) fn threads_window(&mut self, ctx: &egui::Context) {
        if !self.threads.open {
            return;
        }
        if let Some(rx) = &self.threads.job
            && let Ok(r) = rx.try_recv()
        {
            self.threads.job = None;
            let name = self.chart().name.clone();
            self.threads.charts = threads::load_all();
            if let Ok(got) = &r {
                if let Some(i) = self.threads.charts.iter().position(|c| &c.name == got) {
                    self.threads.chosen = i;
                    self.prefs.set("threads.chart", got);
                }
            } else if let Some(i) = self.threads.charts.iter().position(|c| c.name == name) {
                self.threads.chosen = i;
            }
            self.threads.message = Some(r.map(|n| t!("{n} is ready.", n = n)));
        }
        let muted = self.muted();
        let accent = self.accent();
        let mut open = true;
        let mut fetch: Option<(&'static str, &'static str)> = None;
        let mut own = self.my_threads();
        let mut own_changed = false;
        egui::Window::new(t!("Thread charts")).open(&mut open).collapsible(false).resizable(true).default_size(vec2(520.0, 560.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(t!("Thread lists use"));
                let before = self.threads.chosen;
                egui::ComboBox::from_id_salt("chart").selected_text(&self.chart().name).width(220.0).show_ui(ui, |ui| {
                    for (i, c) in self.threads.charts.iter().enumerate() {
                        ui.selectable_value(&mut self.threads.chosen, i, format!("{} ({})", c.name, c.threads.len()));
                    }
                });
                if before != self.threads.chosen {
                    self.prefs.set("threads.chart", &self.threads.charts[self.threads.chosen].name);
                    own = self.my_threads();
                }
            });
            ui.label(
                RichText::new(t!("The machine file keeps Janome thread numbers. The Colors list, copied lists and worksheets name the closest thread of this chart, preferring ones in My threads."))
                    .size(12.0)
                    .color(muted),
            );
            ui.collapsing(t!("Get more charts"), |ui| {
                ui.label(RichText::new(t!("Downloaded from the Ink/Stitch project (GPL) into ~/.local/share/hoop/threads. Charts in Inkscape's palettes folder show up too.")).size(12.0).color(muted));
                let busy = self.threads.job.is_some();
                egui::Grid::new("downloads").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    for &(name, file) in threads::DOWNLOADS {
                        ui.label(name);
                        let have = threads::is_downloaded(file);
                        if ui.add_enabled(!busy, egui::Button::new(if have { t!("Update") } else { t!("Get") })).clicked() {
                            fetch = Some((name, file));
                        }
                        ui.end_row();
                    }
                });
                if busy {
                    ui.label(RichText::new(t!("Downloading…")).color(muted));
                }
                if let Some(m) = &self.threads.message {
                    match m {
                        Ok(t) => ui.label(RichText::new(t).color(super::OK)),
                        Err(e) => ui.label(RichText::new(e).color(super::BAD)),
                    };
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(t!("My threads · {v0} ticked", v0 = own.len())).strong());
                ui.checkbox(&mut self.threads.only_mine, t!("Only mine"));
                if ui.small_button(t!("None")).clicked() {
                    own.clear();
                    own_changed = true;
                }
            });
            ui.add(egui::TextEdit::singleline(&mut self.threads.filter).hint_text(t!("Find by name or number")).desired_width(ui.available_width()));
            let needle = self.threads.filter.to_lowercase();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let chart = self.chart().clone();
                let cell = vec2(116.0, 30.0);
                let cols = ((ui.available_width() / (cell.x + 4.0)).floor() as usize).max(1);
                let shown: Vec<&Thread> = chart
                    .threads
                    .iter()
                    .filter(|t| needle.is_empty() || t.name.to_lowercase().contains(&needle) || t.code.to_lowercase().contains(&needle))
                    .filter(|t| !self.threads.only_mine || own.contains(&threads::key(t)))
                    .collect();
                egui::Grid::new("spools").num_columns(cols).spacing([4.0, 4.0]).show(ui, |ui| {
                    for (i, t) in shown.iter().enumerate() {
                        let k = threads::key(t);
                        let mine = own.contains(&k);
                        let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                        let p = ui.painter();
                        let sw = egui::Rect::from_min_size(rect.min + vec2(2.0, 3.0), Vec2::splat(24.0));
                        p.rect_filled(sw, 12, rgb(t.color));
                        p.circle_stroke(sw.center(), 12.0, egui::Stroke::new(1.0, Color32::from_black_alpha(80)));
                        if mine {
                            p.text(sw.center(), egui::Align2::CENTER_CENTER, "✔", egui::FontId::proportional(13.0), contrast(rgb(t.color)));
                            p.rect_stroke(rect, 4, egui::Stroke::new(1.5, accent), egui::StrokeKind::Inside);
                        } else if resp.hovered() {
                            p.rect_stroke(rect, 4, egui::Stroke::new(1.0, muted), egui::StrokeKind::Inside);
                        }
                        let text = if t.code.is_empty() { t.name.clone() } else { format!("{}\n{}", t.code, t.name) };
                        let g = ui.painter().layout(text, egui::FontId::proportional(10.5), ui.visuals().text_color(), cell.x - 34.0);
                        ui.painter().galley(egui::pos2(sw.max.x + 5.0, rect.center().y - g.size().y / 2.0), g, ui.visuals().text_color());
                        if resp.on_hover_text(t!("{code} {name} · click to mark as mine", code = t.code, name = t.name)).clicked() {
                            if mine {
                                own.retain(|c| *c != k);
                            } else {
                                own.push(k);
                            }
                            own_changed = true;
                        }
                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
            ui.add_space(4.0);
            if ui.add(primary(t!("Done"), accent)).clicked() {
                self.threads.open = false;
            }
        });
        if !open {
            self.threads.open = false;
        }
        if own_changed {
            let key = format!("threads.own.{}", self.chart().name);
            self.prefs.set(&key, own.join(","));
        }
        if let Some((name, file)) = fetch {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = threads::download(file).map(|_| name.to_string());
                let _ = tx.send(r);
                ctx.request_repaint();
            });
            self.threads.job = Some(rx);
            self.threads.message = None;
        }
    }
}
