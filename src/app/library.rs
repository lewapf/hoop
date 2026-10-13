//! Library page of the left dock: browse a designs folder by picture and
//! open a design with a click. Designs can be sorted, narrowed to a kind
//! of file or to the ones that fit the hoop, and searched below the folder.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Color32, RichText, Sense, vec2};

use super::HoopApp;
use crate::hoops;
use crate::library::{self, Entry, Facts, Kind, Painter, SIZE};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Show {
    #[default]
    All,
    Stitches,
    Svg,
    Pictures,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Sort {
    #[default]
    Name,
    Newest,
    Smallest,
    Stitches,
    Type,
}

#[derive(Default)]
pub(super) struct Library {
    pub dir: Option<PathBuf>,
    entries: Vec<Entry>,
    /// None while a thumbnail is being drawn or when it can't be.
    thumbs: HashMap<PathBuf, Option<egui::TextureHandle>>,
    facts: HashMap<PathBuf, Facts>,
    painter: Option<Painter>,
    filter: String,
    show: Show,
    sort: Sort,
    /// Lists the files in the folders below too.
    deep: bool,
    /// Only stitch files that fit the hoop.
    fits: bool,
}

/// Files listed at most when looking in the folders below.
const DEEP_LIMIT: usize = 3000;
/// Tile widths for the small, medium and large views.
const TILES: [f32; 3] = [72.0, 110.0, 150.0];

/// The bundled symbols: the source tree's assets/shapes when running from a
/// build, so they match the catalog built in, else /usr/share/hoop/shapes.
pub(super) fn shapes_dir() -> Option<PathBuf> {
    [PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shapes")), PathBuf::from("/usr/share/hoop/shapes")].into_iter().find(|p| p.is_dir())
}

/// The folder designs live in: ~/Documents/Embroidery Designs, else
/// ~/Documents, else home.
pub(super) fn designs_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    [home.join("Documents/Embroidery Designs"), home.join("Documents"), home.clone()].into_iter().find(|p| p.is_dir()).unwrap_or(home)
}

/// How long ago a file changed, in words.
fn ago(t: std::time::SystemTime) -> String {
    let days = t.elapsed().map_or(0, |d| d.as_secs() / 86_400);
    match days {
        0 => t!("today").to_string(),
        1 => t!("yesterday").to_string(),
        2..=59 => tn!(days, "{n} day ago", "{n} days ago"),
        60..=729 => tn!(days / 30, "{n} month ago", "{n} months ago"),
        _ => tn!(days / 365, "{n} year ago", "{n} years ago"),
    }
}

impl HoopApp {
    fn library_go(&mut self, dir: PathBuf) {
        self.library.entries = if self.library.deep { library::scan_deep(&dir, DEEP_LIMIT) } else { library::scan(&dir) };
        self.library.dir = Some(dir);
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
            while let Ok((path, px, facts)) = p.done.try_recv() {
                let tex = px.map(|px| {
                    let img = egui::ColorImage::from_rgba_unmultiplied([SIZE as usize, SIZE as usize], &px);
                    ui.ctx().load_texture(format!("thumb:{}", path.display()), img, egui::TextureOptions::LINEAR)
                });
                if let Some(f) = facts {
                    self.library.facts.insert(path.clone(), f);
                }
                self.library.thumbs.insert(path, tex);
            }
        }
        let dir = self.library.dir.clone().unwrap();
        let mut go: Option<PathBuf> = None;

        // Where we are: the last folders of the path, each one a click away.
        let parts: Vec<PathBuf> = dir.ancestors().map(|p| p.to_path_buf()).collect::<Vec<_>>().into_iter().rev().collect();
        let label = |p: &PathBuf| p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.spacing_mut().button_padding = vec2(4.0, 1.0);
            let shown = parts.len().saturating_sub(3);
            if shown > 0 {
                ui.menu_button("…", |ui| {
                    for p in parts[..shown].iter().rev() {
                        if ui.button(label(p)).clicked() {
                            go = Some(p.clone());
                        }
                    }
                })
                .response
                .on_hover_text(t!("Folders further up"));
            }
            for (i, p) in parts.iter().enumerate().skip(shown) {
                if i > shown {
                    ui.label(RichText::new("›").color(muted));
                }
                let last = i + 1 == parts.len();
                let text = RichText::new(label(p)).size(12.0);
                if ui.add(egui::Button::new(if last { text.strong() } else { text }).frame(false)).on_hover_text(p.display().to_string()).clicked() && !last {
                    go = Some(p.clone());
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button(t!("Folder…")).on_hover_text(t!("Pick the folder to browse")).clicked()
                && let Some(d) = rfd::FileDialog::new().set_directory(&dir).pick_folder()
            {
                go = Some(d);
            }
            if ui.button(t!("Reload")).clicked() {
                go = Some(dir.clone());
            }
            if ui.checkbox(&mut self.library.deep, t!("Subfolders")).on_hover_text(t!("Also list the designs in the folders inside this one")).changed() {
                go = Some(dir.clone());
            }
        });
        ui.add(egui::TextEdit::singleline(&mut self.library.filter).hint_text(t!("Find by name")).desired_width(ui.available_width()));
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            ui.spacing_mut().button_padding = vec2(5.0, 1.0);
            for (show, text, tip) in [
                (Show::All, t!("All"), t!("Every file Hoop can open")),
                (Show::Stitches, t!("Embroidery"), t!("Stitch files: JEF, PES, VP3, EXP, DST, PEC")),
                (Show::Svg, "SVG", t!("Drawings to turn into stitches")),
                (Show::Pictures, t!("Pictures"), t!("PNG, JPEG and other pictures to trace")),
            ] {
                if ui.add(egui::Button::selectable(self.library.show == show, RichText::new(text).size(12.0))).on_hover_text(tip).clicked() {
                    self.library.show = show;
                }
            }
        });
        let hoop = hoops::get(self.setup.hoop);
        let mut size = self.prefs.num::<usize>("library.tiles").unwrap_or(1).min(TILES.len() - 1);
        ui.horizontal(|ui| {
            let sorts = [
                (Sort::Name, t!("Name")),
                (Sort::Newest, t!("Newest first")),
                (Sort::Smallest, t!("Smallest design first")),
                (Sort::Stitches, t!("Fewest stitches first")),
                (Sort::Type, t!("File type")),
            ];
            egui::ComboBox::from_id_salt("library-sort")
                .selected_text(sorts.iter().find(|s| s.0 == self.library.sort).map_or("", |s| s.1))
                .width(110.0)
                .show_ui(ui, |ui| {
                    for (s, text) in sorts {
                        ui.selectable_value(&mut self.library.sort, s, text);
                    }
                });
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
        if self.prefs.num::<usize>("library.tiles") != Some(size) {
            self.prefs.set("library.tiles", size);
        }
        ui.checkbox(&mut self.library.fits, t!("Fits the {v0}", v0 = hoop.name))
            .on_hover_text(t!("Only embroidery that fits the hoop as it is or turned a quarter"));
        ui.add_space(4.0);

        // Sorting by size or picking what fits needs every file looked at, not just the ones in view.
        if (self.library.fits || matches!(self.library.sort, Sort::Smallest | Sort::Stitches))
            && let Some(painter) = &self.library.painter
        {
            for e in self.library.entries.iter().filter(|e| e.kind == Kind::Stitches) {
                if !self.library.thumbs.contains_key(&e.path) {
                    self.library.thumbs.insert(e.path.clone(), None);
                    painter.request(e.path.clone());
                }
            }
        }
        let fits = |f: &Facts| (f.w_mm <= hoop.w && f.h_mm <= hoop.h) || (f.h_mm <= hoop.w && f.w_mm <= hoop.h);
        let needle = self.library.filter.to_lowercase();
        let lib = &self.library;
        let mut shown: Vec<Entry> = lib
            .entries
            .iter()
            .filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle))
            .filter(|e| match lib.show {
                Show::All => true,
                Show::Stitches => matches!(e.kind, Kind::Folder | Kind::Stitches),
                Show::Svg => matches!(e.kind, Kind::Folder | Kind::Svg),
                Show::Pictures => matches!(e.kind, Kind::Folder | Kind::Picture),
            })
            // Files not looked at yet stay until their size is known.
            .filter(|e| !lib.fits || e.kind == Kind::Folder || (e.kind == Kind::Stitches && lib.facts.get(&e.path).is_none_or(fits)))
            .cloned()
            .collect();
        let ext = |e: &Entry| e.path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        let area = |e: &Entry| lib.facts.get(&e.path).map_or(f32::MAX, |f| f.w_mm * f.h_mm);
        let stitches = |e: &Entry| lib.facts.get(&e.path).map_or(usize::MAX, |f| f.stitches);
        // Folders always come first.
        shown.sort_by(|a, b| {
            (a.kind != Kind::Folder).cmp(&(b.kind != Kind::Folder)).then_with(|| match lib.sort {
                Sort::Name => std::cmp::Ordering::Equal,
                Sort::Newest => b.modified.cmp(&a.modified),
                Sort::Smallest => area(a).total_cmp(&area(b)),
                Sort::Stitches => stitches(a).cmp(&stitches(b)),
                Sort::Type => ext(a).cmp(&ext(b)),
            })
        });
        if shown.is_empty() {
            ui.label(RichText::new(t!("No designs here. Hoop shows JEF, DST, EXP, PES, PEC, VP3, SVG and pictures.")).size(12.0).color(muted));
        }
        let open_now = self.design.as_ref().map(|d| d.name.clone());
        let mut open: Option<PathBuf> = None;
        let tile_w = TILES[size];
        let caption = if size == 0 { 16.0 } else { 30.0 };
        egui::ScrollArea::vertical().id_salt("library").auto_shrink([false, false]).show(ui, |ui| {
            let gap = 6.0;
            let cols = ((ui.available_width() + gap) / (tile_w + gap)).floor().max(1.0) as usize;
            let tile = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32 - 1.0;
            for row in shown.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for e in row {
                        let (rect, resp) = ui.allocate_exact_size(vec2(tile, tile + caption), Sense::click());
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let pic = egui::Rect::from_min_size(rect.min, vec2(tile, tile));
                        let p = ui.painter();
                        let bg = ui.visuals().extreme_bg_color;
                        let current = open_now.as_deref() == Some(e.name.as_str());
                        p.rect_filled(pic, 4, bg);
                        if resp.hovered() || current {
                            p.rect_stroke(pic, 4, egui::Stroke::new(if current { 2.0 } else { 1.0 }, accent), egui::StrokeKind::Inside);
                        }
                        let facts = self.library.facts.get(&e.path).copied();
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
                                    self.library.thumbs.insert(e.path.clone(), None);
                                    if let Some(painter) = &self.library.painter {
                                        painter.request(e.path.clone());
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
                        // A dot for whether it fits the hoop.
                        if let Some(f) = facts {
                            let c = if fits(&f) { Color32::from_rgb(0x3c, 0xb0, 0x5a) } else { Color32::from_rgb(0xe0, 0x90, 0x20) };
                            ui.painter().circle_filled(pic.left_top() + vec2(8.0, 8.0), 3.5, c);
                        }
                        let text = egui::WidgetText::from(RichText::new(&e.name).size(11.0)).into_galley(
                            ui,
                            Some(egui::TextWrapMode::Truncate),
                            tile,
                            egui::TextStyle::Body,
                        );
                        ui.painter().galley(pic.center_bottom() - vec2(text.size().x / 2.0, -2.0), text, ui.visuals().text_color());
                        if size > 0
                            && let Some(f) = facts
                        {
                            let line = t!("{w} × {h} mm", w = format!("{:.0}", f.w_mm), h = format!("{:.0}", f.h_mm));
                            let g = egui::WidgetText::from(RichText::new(line).size(10.0).color(muted)).into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                tile,
                                egui::TextStyle::Body,
                            );
                            ui.painter().galley(pic.center_bottom() + vec2(-g.size().x / 2.0, 16.0), g, muted);
                        }
                        let resp = resp.on_hover_ui(|ui| {
                            ui.label(RichText::new(&e.name).strong());
                            if let Some(f) = facts {
                                ui.label(t!(
                                    "{w} × {h} mm · {s} stitches · {c} colors",
                                    w = format!("{:.1}", f.w_mm),
                                    h = format!("{:.1}", f.h_mm),
                                    s = f.stitches,
                                    c = f.colors
                                ));
                                ui.label(if fits(&f) { t!("Fits the {v0}", v0 = hoop.name) } else { t!("Too big for the {v0}", v0 = hoop.name) });
                            }
                            if e.kind != Kind::Folder {
                                let when = e.modified.map(ago).unwrap_or_default();
                                ui.label(RichText::new(t!("{kb} KB · changed {when}", kb = e.bytes.div_ceil(1024), when = when)).color(muted));
                            }
                            if self.library.deep
                                && let Some(parent) = e.path.parent()
                            {
                                ui.label(RichText::new(parent.display().to_string()).size(11.0).color(muted));
                            }
                        });
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
            self.library.filter.clear();
            self.library_go(d);
        }
        if let Some(p) = open {
            self.open_later = Some(p);
        }
    }
}
