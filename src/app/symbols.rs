//! Symbols page of the left dock: the bundled symbols by category, with
//! search, favorites and the recently used ones. A click adds a symbol to
//! the drawing in Artwork, or opens it as a new design elsewhere.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use egui::{Color32, RichText, Sense, vec2};

use super::library::shapes_dir;
use super::*;
use crate::symbols::{Category, SYMBOLS, Symbol};

/// Which symbols are shown.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Shelf {
    #[default]
    All,
    Favorites,
    Recent,
    In(Category),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Order {
    #[default]
    Category,
    Name,
    /// The ones that stitch small first.
    Bold,
}

/// Symbols up to this size, in mm, count as fit for small designs.
const SMALL_MM: f32 = 25.0;
const RECENT: usize = 24;

#[derive(Default)]
pub(super) struct Symbols {
    filter: String,
    shelf: Shelf,
    order: Order,
    small_only: bool,
    thumbs: HashMap<&'static str, egui::TextureHandle>,
    /// How small each symbol stitches cleanly, worked out in the background.
    smallest: HashMap<&'static str, f32>,
    sizing: Option<Receiver<(&'static str, f32)>>,
}

/// Tile widths for the small, medium and large views.
const TILES: [f32; 3] = [44.0, 72.0, 104.0];

impl HoopApp {
    fn symbol_list(&self, key: &str) -> Vec<&'static Symbol> {
        self.prefs.get(key).unwrap_or("").split(' ').filter_map(crate::symbols::find).collect()
    }

    fn set_symbol_list(&mut self, key: &str, list: &[&Symbol]) {
        let v: Vec<&str> = list.iter().map(|s| s.file).collect();
        self.prefs.set(key, v.join(" "));
    }

    fn toggle_favorite(&mut self, sym: &'static Symbol) {
        let mut favs = self.symbol_list("symbols.favorites");
        if let Some(i) = favs.iter().position(|s| s.file == sym.file) {
            favs.remove(i);
        } else {
            favs.push(sym);
        }
        self.set_symbol_list("symbols.favorites", &favs);
    }

    /// Adds the symbol to the drawing in Artwork, else opens it as a design.
    fn use_symbol(&mut self, sym: &'static Symbol, open_new: bool) {
        let Some(path) = shapes_dir().map(|d| d.join(format!("{}.svg", sym.file))) else { return };
        if !open_new && self.mode == Mode::Artwork && self.art.is_some() {
            let added = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|svg| self.add_symbol(sym, &svg));
            if let Err(e) = added {
                self.toast(e);
            }
        } else {
            self.open_later = Some(path);
        }
        let mut recent = self.symbol_list("symbols.recent");
        recent.retain(|s| s.file != sym.file);
        recent.insert(0, sym);
        recent.truncate(RECENT);
        self.set_symbol_list("symbols.recent", &recent);
    }

    pub(super) fn symbols_page(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        let Some(dir) = shapes_dir() else {
            ui.label(RichText::new(t!("The symbols aren't installed.")).color(muted));
            return;
        };
        // Work out how small each symbol stitches, once, off the UI thread.
        if self.symbols.sizing.is_none() && self.symbols.smallest.is_empty() {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ui.ctx().clone();
            let dir = dir.clone();
            std::thread::spawn(move || {
                for s in &SYMBOLS {
                    if let Some(mm) = std::fs::read_to_string(dir.join(format!("{}.svg", s.file))).ok().and_then(|svg| crate::symbols::smallest_mm(&svg))
                        && tx.send((s.file, mm)).is_err()
                    {
                        return;
                    }
                }
                ctx.request_repaint();
            });
            self.symbols.sizing = Some(rx);
        }
        if let Some(rx) = &self.symbols.sizing {
            self.symbols.smallest.extend(rx.try_iter());
        }

        ui.add(egui::TextEdit::singleline(&mut self.symbols.filter).hint_text(t!("Find: cat, Christmas, sport…")).desired_width(ui.available_width()));
        ui.add_space(4.0);
        let favs = self.symbol_list("symbols.favorites");
        let recent = self.symbol_list("symbols.recent");
        let mut shelves = vec![(Shelf::All, t!("All").to_string()), (Shelf::Favorites, t!("Favorites ({v0})", v0 = favs.len()))];
        if !recent.is_empty() {
            shelves.push((Shelf::Recent, t!("Recent").to_string()));
        }
        shelves.extend(Category::ALL.iter().map(|&c| (Shelf::In(c), c.name().to_string())));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            ui.spacing_mut().button_padding = vec2(5.0, 1.0);
            for (shelf, text) in shelves {
                let on = self.symbols.shelf == shelf;
                if ui.add(egui::Button::selectable(on, RichText::new(text).size(12.0))).clicked() {
                    self.symbols.shelf = shelf;
                }
            }
        });
        ui.add_space(2.0);
        let mut size = self.prefs.num::<usize>("symbols.tiles").unwrap_or(1).min(TILES.len() - 1);
        ui.horizontal(|ui| {
            let orders = [(Order::Category, t!("By category")), (Order::Name, t!("A to Z")), (Order::Bold, t!("Smallest first"))];
            egui::ComboBox::from_id_salt("symbol-order")
                .selected_text(orders.iter().find(|o| o.0 == self.symbols.order).map_or("", |o| o.1))
                .width(110.0)
                .show_ui(ui, |ui| {
                    for (o, text) in orders {
                        ui.selectable_value(&mut self.symbols.order, o, text);
                    }
                })
                .response
                .on_hover_text(t!("Smallest first: the bold symbols that still stitch cleanly when small"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, (text, tip)) in [("L", t!("Large tiles")), ("M", t!("Medium tiles")), ("S", t!("Small tiles"))].into_iter().enumerate() {
                    let i = TILES.len() - 1 - i;
                    if ui.add(egui::Button::selectable(size == i, RichText::new(text).size(11.0))).on_hover_text(tip).clicked() {
                        size = i;
                    }
                }
            });
        });
        ui.checkbox(&mut self.symbols.small_only, t!("Only ones that stitch small"))
            .on_hover_text(t!("Only symbols that stitch cleanly at {v0} mm across", v0 = SMALL_MM as u32));
        if self.prefs.num::<usize>("symbols.tiles") != Some(size) {
            self.prefs.set("symbols.tiles", size);
        }
        ui.add_space(4.0);

        let needle = self.symbols.filter.trim().to_lowercase();
        let smallest = self.symbols.smallest.clone();
        let small_only = self.symbols.small_only;
        let fits = |s: &Symbol| !small_only || smallest.get(s.file).is_some_and(|&mm| mm <= SMALL_MM);
        let mut shown: Vec<&'static Symbol> = match self.symbols.shelf {
            Shelf::All => SYMBOLS.iter().collect(),
            Shelf::Favorites => favs.clone(),
            Shelf::Recent => recent,
            Shelf::In(c) => SYMBOLS.iter().filter(|s| s.category == c).collect(),
        };
        shown.retain(|s| s.matches(&needle) && fits(s));
        match self.symbols.order {
            Order::Category if self.symbols.shelf == Shelf::All => shown.sort_by_key(|s| Category::ALL.iter().position(|&c| c == s.category)),
            Order::Category => {}
            Order::Name => shown.sort_by_cached_key(|s| crate::i18n::tr(s.name).to_lowercase()),
            Order::Bold => shown.sort_by(|a, b| smallest.get(a.file).unwrap_or(&999.0).total_cmp(smallest.get(b.file).unwrap_or(&999.0))),
        }
        if shown.is_empty() {
            let text = match self.symbols.shelf {
                Shelf::Favorites if favs.is_empty() => t!("No favorites yet. Click the star on a symbol to keep it here."),
                _ => t!("No symbols match."),
            };
            ui.label(RichText::new(text).size(12.0).color(muted));
        }
        // Category headings only make sense when everything is in category order.
        let headed = self.symbols.order == Order::Category && self.symbols.shelf == Shelf::All;
        let adds = self.mode == Mode::Artwork && self.art.is_some();
        let mut used: Option<(&'static Symbol, bool)> = None;
        let mut starred: Option<&'static Symbol> = None;
        let tile = TILES[size];
        egui::ScrollArea::vertical().id_salt("symbols").auto_shrink([false, false]).show(ui, |ui| {
            let gap = 4.0;
            let cols = ((ui.available_width() + gap) / (tile + gap)).floor().max(1.0) as usize;
            let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32 - 0.5;
            let label = if size == 0 { 0.0 } else { 16.0 };
            let groups: Vec<&[&Symbol]> = if headed { shown.chunk_by(|a, b| a.category == b.category).collect() } else { vec![&shown[..]] };
            for group in groups {
                if headed && let Some(first) = group.first() {
                    ui.add_space(2.0);
                    ui.label(RichText::new(first.category.name()).size(12.0).strong().color(muted));
                }
                for row in group.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for &s in row {
                            let (rect, resp) = ui.allocate_exact_size(vec2(w, w + label), Sense::click());
                            if !ui.is_rect_visible(rect) {
                                continue;
                            }
                            let pic = egui::Rect::from_min_size(rect.min, vec2(w, w));
                            let v = ui.visuals();
                            let p = ui.painter();
                            p.rect_filled(pic, 4, v.extreme_bg_color);
                            if resp.hovered() {
                                p.rect_stroke(pic, 4, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
                            }
                            if let Some(tex) = self.symbol_thumb(ui.ctx(), &dir, s) {
                                let tint = if resp.hovered() { v.strong_text_color() } else { v.text_color() };
                                p.image(tex.id(), pic.shrink(w * 0.18), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
                            }
                            let name = crate::i18n::tr(s.name);
                            if label > 0.0 {
                                let g = egui::WidgetText::from(RichText::new(name).size(10.5)).into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Truncate),
                                    w,
                                    egui::TextStyle::Body,
                                );
                                ui.painter().galley(pic.center_bottom() + vec2(-g.size().x / 2.0, 2.0), g, ui.visuals().text_color());
                            }
                            // The star: always shown on favorites, on hover for the rest.
                            let fav = favs.iter().any(|f| f.file == s.file);
                            let star = egui::Rect::from_min_size(pic.right_top() + vec2(-17.0, 3.0), vec2(14.0, 14.0));
                            let over_star = resp.hover_pos().is_some_and(|q| star.expand(2.0).contains(q));
                            if fav || resp.hovered() {
                                let tint = if fav {
                                    Color32::from_rgb(0xe8, 0xb0, 0x20)
                                } else if over_star {
                                    ui.visuals().strong_text_color()
                                } else {
                                    muted
                                };
                                icons::paint(ui, if fav { "star-fill" } else { "star" }, star, tint);
                            }
                            let mm = smallest.get(s.file).copied();
                            let resp = resp.on_hover_ui(|ui| {
                                ui.label(RichText::new(name).strong());
                                ui.label(RichText::new(s.category.name()).color(muted));
                                if let Some(mm) = mm {
                                    ui.label(t!("Stitches cleanly from about {v0} mm across.", v0 = mm as u32));
                                }
                                ui.label(
                                    RichText::new(if adds { t!("Click to add it to the drawing.") } else { t!("Click to open it as a new design.") })
                                        .color(muted),
                                );
                            });
                            resp.context_menu(|ui| {
                                if adds && ui.button(t!("Add to the drawing")).clicked() {
                                    used = Some((s, false));
                                }
                                if ui.button(t!("Open as a new design")).clicked() {
                                    used = Some((s, true));
                                }
                                if ui.button(if fav { t!("Remove from favorites") } else { t!("Add to favorites") }).clicked() {
                                    starred = Some(s);
                                }
                            });
                            if resp.clicked() {
                                if over_star {
                                    starred = Some(s);
                                } else {
                                    used = Some((s, false));
                                }
                            }
                        }
                    });
                    ui.add_space(gap);
                }
            }
        });
        if let Some(s) = starred {
            self.toggle_favorite(s);
        }
        if let Some((s, open_new)) = used {
            self.use_symbol(s, open_new);
        }
    }

    /// The symbol drawn white for tinting, made the first time it shows.
    fn symbol_thumb(&mut self, ctx: &egui::Context, dir: &std::path::Path, s: &'static Symbol) -> Option<egui::TextureHandle> {
        if let Some(t) = self.symbols.thumbs.get(s.file) {
            return Some(t.clone());
        }
        let svg = std::fs::read_to_string(dir.join(format!("{}.svg", s.file))).ok()?;
        let tex = ctx.load_texture(format!("symbol:{}", s.file), icons::render(&svg, 128)?, egui::TextureOptions::LINEAR);
        self.symbols.thumbs.insert(s.file, tex.clone());
        Some(tex)
    }
}
