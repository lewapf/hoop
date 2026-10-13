//! The window around the hoop: menu bar, tool bar, the Objects/Colors dock
//! on the left, the tabbed properties dock on the right and the status bar.

use super::shapes::Arrange;
use super::*;

/// Pages of the right dock. Each mode shows its own ones first, then the
/// shared Hoop, Stitch-out and View pages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tab {
    Design,
    Text,
    Style,
    Patch,
    Source,
    Shape,
    Threads,
    Hoop,
    Stitch,
    View,
}

impl Tab {
    fn icon(self) -> &'static str {
        match self {
            Tab::Design => "info",
            Tab::Text => "text",
            Tab::Style => "brush",
            Tab::Patch => "patch",
            Tab::Source => "source",
            Tab::Shape => "bezier",
            Tab::Threads => "yarn",
            Tab::Hoop => "fit",
            Tab::Stitch => "needle",
            Tab::View => "eye",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::Design => t!("Design"),
            Tab::Text => t!("Text"),
            Tab::Style => t!("Style"),
            Tab::Patch => t!("Patch"),
            Tab::Source => t!("Source"),
            Tab::Shape => t!("Shape"),
            Tab::Threads => t!("Threads"),
            Tab::Hoop => t!("Hoop"),
            Tab::Stitch => t!("Stitch"),
            Tab::View => t!("View"),
        }
    }
}

fn tabs(mode: Mode) -> &'static [Tab] {
    match mode {
        Mode::Viewer => &[Tab::Design, Tab::Hoop, Tab::Stitch, Tab::View],
        Mode::Lettering => &[Tab::Text, Tab::Style, Tab::Patch, Tab::Hoop, Tab::Stitch, Tab::View],
        Mode::Artwork => &[Tab::Source, Tab::Shape, Tab::Style, Tab::Threads, Tab::Hoop, Tab::Stitch, Tab::View],
    }
}

pub(super) fn tab_named(mode: Mode, name: &str) -> Option<Tab> {
    tabs(mode).iter().copied().find(|t| format!("{t:?}").eq_ignore_ascii_case(name))
}

/// Pages of the left dock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dock {
    Library,
    Symbols,
    Objects,
    Colors,
    Check,
}

/// A row of tabs like a Qt tab widget: the picked one is raised and
/// underlined. Tabs that don't fit wrap onto another row.
/// A segmented tab bar across the full width: a rounded track with the
/// picked tab filled in the accent color. Tabs that don't fit on one row
/// are spread evenly over two.
pub(super) fn tab_row<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)], accent: Color32) {
    let font = egui::FontId::proportional(13.5);
    let v = ui.visuals().clone();
    let width = ui.available_width();
    let galleys: Vec<_> = options.iter().map(|o| ui.painter().layout_no_wrap(o.1.to_string(), font.clone(), v.text_color())).collect();
    let need: f32 = galleys.iter().map(|g| g.size().x + 18.0).sum::<f32>() + 6.0;
    let rows = if need <= width || options.len() < 4 { 1 } else { 2 };
    let per_row = options.len().div_ceil(rows);
    let row_h = 28.0;
    let (track, _) = ui.allocate_exact_size(vec2(width, row_h * rows as f32 + 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(track, 8, v.extreme_bg_color);
    p.rect_stroke(track, 8, egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Inside);
    for (r, chunk) in options.chunks(per_row).enumerate() {
        let start = r * per_row;
        // Widths in proportion to the labels, filling the row.
        let natural: Vec<f32> = (0..chunk.len()).map(|k| galleys[start + k].size().x + 18.0).collect();
        let total: f32 = natural.iter().sum();
        let inner = track.width() - 6.0;
        let mut x = track.left() + 3.0;
        let y = track.top() + 3.0 + r as f32 * row_h;
        for (k, &(val, _)) in chunk.iter().enumerate() {
            let w = natural[k] / total * inner;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), vec2(w, row_h));
            x += w;
            let resp = ui.interact(rect, ui.id().with(("tab", start + k)), Sense::click());
            let on = *value == val;
            let p = ui.painter();
            if on {
                p.rect_filled(rect.shrink(1.0), 6, accent);
            } else if resp.hovered() {
                p.rect_filled(rect.shrink(1.0), 6, v.widgets.hovered.weak_bg_fill);
            }
            let color = if on {
                contrast(accent)
            } else if resp.hovered() {
                v.strong_text_color()
            } else {
                v.text_color().gamma_multiply(0.8)
            };
            let g = galleys[start + k].clone();
            p.galley_with_override_text_color(rect.center() - g.size() / 2.0, g, color);
            if resp.clicked() {
                *value = val;
            }
        }
    }
}

/// A little space between groups of tool buttons.
fn tool_gap(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().rect_filled(r, 0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    ui.add_space(6.0);
}

const SEGMENT_PAD: f32 = 28.0;

/// Room for an option's icon and the gap after it.
const SEGMENT_ICON: f32 = 21.0;

fn segmented_width<T>(ui: &egui::Ui, options: &[(T, &str, &'static str)]) -> f32 {
    let font = egui::FontId::proportional(13.0);
    options.iter().map(|o| ui.painter().layout_no_wrap(o.1.to_string(), font.clone(), Color32::WHITE).size().x + SEGMENT_PAD + SEGMENT_ICON).sum::<f32>() + 4.0
}

/// A compact segmented switch: a rounded track with the picked option
/// filled, each option an icon and a label.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str, &'static str)], accent: Color32) {
    let font = egui::FontId::proportional(13.0);
    let v = ui.visuals().clone();
    let (track, _) = ui.allocate_exact_size(vec2(segmented_width(ui, options), 24.0), Sense::hover());
    ui.painter().rect_filled(track, 7, v.extreme_bg_color);
    ui.painter().rect_stroke(track, 7, egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Inside);
    let mut x = track.left() + 2.0;
    for (k, &(val, text, icon)) in options.iter().enumerate() {
        let g = ui.painter().layout_no_wrap(text.to_string(), font.clone(), v.text_color());
        let rect = egui::Rect::from_min_size(egui::pos2(x, track.top() + 2.0), vec2(g.size().x + SEGMENT_PAD + SEGMENT_ICON, track.height() - 4.0));
        x += rect.width();
        let resp = ui.interact(rect, ui.id().with(("segment", k)), Sense::click());
        let on = *value == val;
        if on {
            ui.painter().rect_filled(rect, 5, accent);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, 5, v.widgets.hovered.weak_bg_fill);
        }
        let color = if on { contrast(accent) } else { v.text_color() };
        let left = rect.center().x - (g.size().x + SEGMENT_ICON) / 2.0;
        super::icons::paint(ui, icon, egui::Rect::from_min_size(egui::pos2(left, rect.center().y - 8.0), vec2(16.0, 16.0)), color);
        ui.painter().galley_with_override_text_color(egui::pos2(left + SEGMENT_ICON, rect.center().y - g.size().y / 2.0), g, color);
        if resp.clicked() {
            *value = val;
        }
    }
}

/// One tab of a dock: its value, label, icon and a count to show on it
/// (0 for none).
pub(super) type IconTab<'a, T> = (T, &'a str, &'static str, usize);

/// A dock's tabs standing on the panel's outer edge, like tool windows in
/// an IDE: an icon over the label turned on its side, reading up on the
/// left edge and down on the right. Returns the tab clicked: the caller
/// opens the panel on it, or closes the panel when it was already showing.
fn rail<T: PartialEq + Copy>(ui: &mut egui::Ui, value: T, open: bool, options: &[IconTab<T>], accent: Color32, left: bool) -> Option<T> {
    let v = ui.visuals().clone();
    let mut clicked = None;
    let w = ui.available_width();
    const ICON: f32 = 16.0;
    for &(val, label, icon, badge) in options {
        let on = open && value == val;
        let color = |hovered: bool| {
            if on {
                accent
            } else if hovered {
                v.strong_text_color()
            } else {
                v.text_color().gamma_multiply(0.75)
            }
        };
        let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), v.text_color());
        let h = 10.0 + ICON + 7.0 + g.size().x + 12.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
        let ink = color(resp.hovered());
        let p = ui.painter();
        let tile = rect.shrink2(vec2(3.0, 2.0));
        if on {
            p.rect_filled(tile, 6, accent.gamma_multiply(0.22));
            let edge = if left { rect.left() + 1.5 } else { rect.right() - 1.5 };
            p.line_segment([egui::pos2(edge, tile.top() + 6.0), egui::pos2(edge, tile.bottom() - 6.0)], egui::Stroke::new(3.0, accent));
        } else if resp.hovered() {
            p.rect_filled(tile, 6, v.widgets.hovered.weak_bg_fill);
        }
        let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 10.0 + ICON / 2.0), vec2(ICON, ICON));
        super::icons::paint(ui, icon, icon_rect, ink);
        // The label runs from under the icon to the bottom of the tab. A
        // galley turns around its top-left corner.
        let (top, bottom) = (icon_rect.bottom() + 7.0, rect.bottom() - 12.0);
        let half = g.size().y / 2.0;
        let (pos, angle) = if left {
            (egui::pos2(rect.center().x - half, bottom), -std::f32::consts::FRAC_PI_2)
        } else {
            (egui::pos2(rect.center().x + half, top), std::f32::consts::FRAC_PI_2)
        };
        p.add(egui::epaint::TextShape::new(pos, g, ink).with_override_text_color(ink).with_angle(angle));
        if badge > 0 {
            let text = p.layout_no_wrap(badge.to_string(), egui::FontId::proportional(9.5), Color32::WHITE);
            let r = (text.size().x / 2.0 + 3.5).max(6.5);
            let pill = egui::Rect::from_center_size(icon_rect.right_top() + vec2(1.0, 0.0), vec2(r * 2.0, 13.0));
            p.rect_filled(pill, 6.5, super::BAD);
            p.galley(pill.center() - text.size() / 2.0, text, Color32::WHITE);
        }
        let tip = if on { t!("{v0} · click to hide the panel", v0 = label) } else { label.to_string() };
        if resp.on_hover_text(tip).clicked() {
            clicked = Some(val);
        }
        ui.add_space(2.0);
    }
    clicked
}

pub(super) fn item(ui: &mut egui::Ui, enabled: bool, text: &str, keys: &str) -> bool {
    let clicked = ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(keys)).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// The name of the page a dock shows, over it.
fn dock_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(15.0).strong());
    ui.add_space(4.0);
}

fn check(ui: &mut egui::Ui, value: &mut bool, text: &str, keys: &str) {
    if ui.add(egui::Button::selectable(*value, text).shortcut_text(keys)).clicked() {
        *value = !*value;
        ui.close();
    }
}

impl HoopApp {
    pub(super) fn layout(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let fill = ui.visuals().panel_fill;
        let bar = egui::Frame::new().fill(fill.lerp_to_gamma(ui.visuals().text_color(), 0.03)).inner_margin(egui::Margin::symmetric(6, 2));
        egui::Panel::top("menu").frame(bar).show(ui, |ui| self.menu_bar(ui, canvas_rect));
        egui::Panel::top("tools").frame(bar.inner_margin(egui::Margin::symmetric(6, 3))).show(ui, |ui| self.tool_bar(ui, canvas_rect));
        egui::Panel::bottom("status").frame(bar.inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| self.status_bar(ui));
        let dock = egui::Frame::new().fill(fill).inner_margin(egui::Margin { left: 8, right: 8, top: 8, bottom: 0 });
        // The tabs of each dock stand on its outer edge, and stay when the
        // dock is hidden: a click on the showing tab hides it.
        let rail_frame = egui::Frame::new().fill(fill.lerp_to_gamma(ui.visuals().text_color(), 0.03)).inner_margin(egui::Margin::symmetric(0, 6));
        let accent = self.accent();
        let left_tabs = self.left_tabs();
        egui::Panel::left("dock-rail").resizable(false).exact_size(32.0).frame(rail_frame).show(ui, |ui| {
            if let Some(d) = rail(ui, self.dock, self.show_left, &left_tabs, accent, true) {
                if self.show_left && self.dock == d {
                    self.show_left = false;
                } else {
                    self.dock = d;
                    self.show_left = true;
                }
            }
        });
        let m = self.mode as usize;
        let right_tabs: Vec<IconTab<Tab>> = tabs(self.mode).iter().map(|&t| (t, t.label(), t.icon(), 0)).collect();
        if !tabs(self.mode).contains(&self.tab[m]) {
            self.tab[m] = tabs(self.mode)[0];
        }
        egui::Panel::right("controls-rail").resizable(false).exact_size(32.0).frame(rail_frame).show(ui, |ui| {
            if let Some(t) = rail(ui, self.tab[m], self.show_right, &right_tabs, accent, false) {
                if self.show_right && self.tab[m] == t {
                    self.show_right = false;
                } else {
                    self.tab[m] = t;
                    self.show_right = true;
                }
            }
        });
        if self.show_left {
            egui::Panel::left("dock").resizable(true).default_size(250.0).size_range(210.0..=420.0).frame(dock).show(ui, |ui| self.left_dock(ui));
        }
        if self.show_right {
            egui::Panel::right("controls")
                .resizable(true)
                .default_size(400.0)
                .size_range(350.0..=620.0)
                .frame(dock)
                .show(ui, |ui| self.right_dock(ui, canvas_rect));
        }
        // Between the docks, so it spans only the hoop.
        let timeline = egui::Frame::new().fill(fill).inner_margin(egui::Margin { left: 6, right: 8, top: 3, bottom: 3 });
        egui::Panel::bottom("timeline").resizable(false).frame(timeline).show(ui, |ui| self.timeline(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas(ui));
    }

    /// The left dock's pages for the mode in use.
    fn left_tabs(&mut self) -> Vec<IconTab<'static, Dock>> {
        let problems = self.check.as_ref().map_or(0, |r| r.count(crate::check::Level::Problem));
        let mut options: Vec<IconTab<Dock>> = vec![
            (Dock::Library, t!("Library"), "library", 0),
            (Dock::Symbols, t!("Symbols"), "symbols", 0),
            (Dock::Colors, t!("Colors"), "palette", 0),
            (Dock::Check, t!("Check"), "check", problems),
        ];
        if self.mode == Mode::Artwork {
            options.insert(2, (Dock::Objects, t!("Objects"), "layers", 0));
        } else if self.dock == Dock::Objects {
            self.dock = Dock::Colors;
        }
        options
    }

    fn left_dock(&mut self, ui: &mut egui::Ui) {
        let title = self.left_tabs().into_iter().find(|o| o.0 == self.dock).map_or("", |o| o.1);
        dock_title(ui, title);
        match self.dock {
            Dock::Library => self.library_page(ui),
            Dock::Symbols => self.symbols_page(ui),
            Dock::Objects => self.objects_list(ui),
            Dock::Colors => {
                egui::ScrollArea::vertical().id_salt("colors").auto_shrink([false, false]).show(ui, |ui| self.colors_list(ui));
            }
            Dock::Check => self.check_list(ui),
        }
    }

    fn right_dock(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let m = self.mode as usize;
        dock_title(ui, self.tab[m].label());
        let tab = self.tab[m];
        let key = egui::Id::new(PAGE_TITLE);
        ui.ctx().data_mut(|d| d.insert_temp(key, tab.label().to_string()));
        egui::ScrollArea::vertical().id_salt(("page", m, tab as usize)).auto_shrink([false, false]).show(ui, |ui| {
            // Leave room for the scroll bar and never grow past the panel.
            ui.set_max_width(ui.available_width() - 10.0);
            self.side_panel(ui, canvas_rect, tab);
        });
        ui.ctx().data_mut(|d| d.remove::<String>(key));
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let has = self.design.is_some();
        let out = has || !self.pinned.is_empty();
        let art = self.mode == Mode::Artwork && self.art.is_some();
        let picked = art && !self.sel.is_empty();
        let format = hoops::machine().out;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t!("File"), |ui| {
                if item(ui, true, t!("New project…"), "N") {
                    self.open_wizard();
                }
                if item(ui, true, t!("Open…"), "O") {
                    self.pick_file();
                }
                let recent: Vec<PathBuf> = self.prefs.recent().into_iter().filter(|p| p.exists()).take(8).collect();
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button(t!("Open recent"), |ui| {
                        for p in recent {
                            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                                self.open_later = Some(p);
                                ui.close();
                            }
                        }
                    });
                });
                if item(ui, true, t!("New from shapes"), "") {
                    self.picture = None;
                    self.use_artwork(Ok(artwork::blank()));
                }
                if item(ui, true, t!("Paste SVG code…"), "Ctrl+V") {
                    self.set_mode(Mode::Artwork);
                    self.tab[Mode::Artwork as usize] = Tab::Source;
                    self.paste_open = true;
                }
                ui.separator();
                let work = self.has_project_work();
                if item(ui, work, t!("Save project"), "Ctrl+Alt+S") {
                    self.save_project(false);
                }
                if item(ui, work, t!("Save project as…"), "") {
                    self.save_project(true);
                }
                if item(ui, has, &t!("Save as {format}…", format = format.ext().to_uppercase()), "Ctrl+S") {
                    self.save_design();
                }
                ui.add_enabled_ui(has, |ui| {
                    ui.menu_button(t!("Export as"), |ui| {
                        for o in crate::export::Out::ALL.into_iter().filter(|&o| o != format) {
                            if ui.button(o.label()).clicked() {
                                self.export(o);
                                ui.close();
                            }
                        }
                    });
                });
                ui.separator();
                if item(ui, has, t!("Save picture…"), "Ctrl+Shift+S") {
                    self.save_picture();
                }
                if item(ui, has, t!("Save worksheet (PDF)…"), "Ctrl+P") {
                    self.save_worksheet();
                }
                ui.separator();
                if item(ui, true, t!("Quit"), "Q") {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(t!("Edit"), |ui| {
                if item(ui, !self.undo.is_empty() || self.pending_since.is_some(), t!("Undo"), "Ctrl+Z") {
                    self.undo_step(&ctx);
                }
                if item(ui, !self.redo.is_empty(), t!("Redo"), "Ctrl+Shift+Z") {
                    self.redo_step();
                }
                ui.separator();
                for (text, keys, op, on) in [
                    (t!("Select all shapes"), "Ctrl+A", Arrange::SelectAll, art),
                    (t!("Deselect"), "Esc", Arrange::Deselect, picked),
                    (t!("Duplicate"), "Ctrl+D", Arrange::Duplicate, picked),
                    (t!("Delete"), "Del", Arrange::Delete, picked),
                    (t!("Hide"), "Ctrl+H", Arrange::Hide, picked),
                    (t!("Group"), "Ctrl+G", Arrange::Group, picked),
                    (t!("Ungroup"), "Ctrl+Shift+G", Arrange::Ungroup, picked),
                    (t!("Show all shapes"), "", Arrange::ShowAll, art),
                ] {
                    if item(ui, on, text, keys) {
                        self.arrange(op);
                    }
                }
                ui.separator();
                if item(ui, true, t!("Settings…"), "Ctrl+,") {
                    self.settings.open = true;
                }
            });
            ui.menu_button(t!("View"), |ui| {
                for (m, text, key) in [(Mode::Viewer, t!("Viewer"), "V"), (Mode::Lettering, t!("Lettering"), "L"), (Mode::Artwork, t!("Artwork"), "A")] {
                    if ui.add(egui::Button::selectable(self.mode == m, text).shortcut_text(key)).clicked() {
                        self.set_mode(m);
                        ui.close();
                    }
                }
                ui.separator();
                if item(ui, true, t!("Whole hoop"), "F") {
                    self.fit_view(canvas_rect, false);
                }
                if item(ui, has, t!("Zoom to design"), "D") {
                    self.fit_view(canvas_rect, true);
                }
                ui.separator();
                check(ui, &mut self.grid, t!("Template grid"), "G");
                check(ui, &mut self.jumps, t!("Jump stitches"), "J");
                check(ui, &mut self.textured, t!("Thread texture"), "T");
                check(ui, &mut self.sizes, t!("Size label"), "");
                check(ui, &mut self.density, t!("Thread density"), "H");
                ui.separator();
                check(ui, &mut self.show_left, t!("Objects and colors panel"), "F9");
                check(ui, &mut self.show_right, t!("Properties panel"), "F10");
            });
            ui.menu_button(t!("Design"), |ui| {
                if item(ui, has, t!("Rotate right"), "R") {
                    self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
                }
                if item(ui, has, t!("Rotate left"), "Shift+R") {
                    self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
                }
                if item(ui, has, t!("Mirror"), "M") {
                    self.change(|s| s.at.mirror = !s.at.mirror);
                }
                if item(ui, has, t!("Center in hoop"), "C") {
                    self.change(|s| {
                        s.at.dx = 0.0;
                        s.at.dy = 0.0;
                    });
                }
                ui.separator();
                if item(ui, has, t!("Pin to hoop"), "P") {
                    self.pin();
                }
                if item(ui, !self.pinned.is_empty(), t!("Clear pinned parts"), "") {
                    self.pinned.clear();
                }
            });
            if self.mode == Mode::Artwork {
                ui.menu_button(t!("Shape"), |ui| {
                    ui.menu_button(t!("Insert"), |ui| {
                        for name in artwork::BUILT_IN {
                            if ui.button(crate::i18n::tr(name)).clicked() {
                                if self.art.is_none() {
                                    self.use_artwork(Ok(artwork::blank()));
                                }
                                self.list_add = Some(name);
                                self.tab[Mode::Artwork as usize] = Tab::Shape;
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    for (text, keys, op) in [
                        (t!("Bring to front"), "Shift+]", Arrange::Front),
                        (t!("Bring forward"), "]", Arrange::Forward),
                        (t!("Send backward"), "[", Arrange::Backward),
                        (t!("Send to back"), "Shift+[", Arrange::Back),
                    ] {
                        if item(ui, picked, text, keys) {
                            self.arrange(op);
                        }
                    }
                    ui.separator();
                    ui.add_enabled_ui(picked, |ui| {
                        ui.menu_button(t!("Align"), |ui| {
                            for (text, op) in [
                                (t!("Left edges"), Arrange::Left),
                                (t!("Centers across"), Arrange::CenterX),
                                (t!("Right edges"), Arrange::Right),
                                (t!("Top edges"), Arrange::Top),
                                (t!("Middles up and down"), Arrange::Middle),
                                (t!("Bottom edges"), Arrange::Bottom),
                            ] {
                                if item(ui, true, text, "") {
                                    self.arrange(op);
                                }
                            }
                        });
                    });
                    if item(ui, self.sel.len() == 1, t!("Edit points"), "") {
                        self.edit_points = !self.edit_points;
                        if let (Some(i), Some(a), Some(s)) = (self.primary(), &self.artwork, &mut self.art) {
                            s.shapes[i].bake(a);
                        }
                        self.tab[Mode::Artwork as usize] = Tab::Shape;
                    }
                });
            }
            ui.menu_button(t!("Machine"), |ui| {
                ui.menu_button(t!("Machine"), |ui| {
                    for (i, m) in hoops::MACHINES.iter().enumerate() {
                        let text = format!("{} · {}", crate::i18n::tr(m.name), m.out.ext().to_uppercase());
                        if ui.add(egui::Button::selectable(hoops::machine_index() == i, text)).clicked() {
                            self.choose_machine(i, canvas_rect);
                            ui.close();
                        }
                    }
                });
                ui.menu_button(t!("Hoop"), |ui| self.hoop_choices(ui, canvas_rect));
                ui.separator();
                if item(ui, out, t!("Send to USB…"), "Ctrl+U") {
                    self.open_usb();
                }
                if item(ui, true, t!("Test stitch-out…"), "Ctrl+T") {
                    self.test.open = true;
                }
                if item(ui, out, t!("Split into hoopings…"), "") {
                    self.open_hoopings();
                }
                ui.separator();
                if item(ui, true, t!("Thread charts and my threads…"), "") {
                    self.threads.open = true;
                }
                if item(ui, true, t!("Google Fonts…"), "") {
                    self.fontwin.show();
                }
            });
            ui.menu_button(t!("Help"), |ui| {
                if item(ui, true, t!("User manual"), "F1") {
                    self.manual.open = true;
                    ui.close();
                }
                if item(ui, true, t!("Keyboard shortcuts"), "?") {
                    self.help = true;
                }
                if item(ui, true, t!("Play demo"), "") {
                    self.start_demo();
                }
                ui.separator();
                if item(ui, true, t!("About Hoop"), "") {
                    self.toast(t!("Hoop {v0} · machine embroidery designs", v0 = env!("CARGO_PKG_VERSION")));
                }
            });
            // The three workspaces, centered in the bar.
            let modes = [(Mode::Viewer, t!("Viewer"), "eye"), (Mode::Lettering, t!("Lettering"), "text"), (Mode::Artwork, t!("Artwork"), "pen")];
            let width = segmented_width(ui, &modes);
            let left = ui.cursor().left();
            let mid = ui.ctx().content_rect().center().x;
            ui.add_space((mid - width / 2.0 - left).max(12.0));
            let mut mode = self.mode;
            segmented(ui, &mut mode, &modes, self.accent());
            if mode != self.mode {
                self.set_mode(mode);
            }
        });
        if let Some(name) = self.list_add.take() {
            self.add_built_in(name);
        }
    }

    /// The current machine's hoops as a pick list, with the number keys.
    fn hoop_choices(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        for (i, h) in hoops::list().iter().enumerate() {
            let text = t!("{name}  {w} × {h} mm", name = h.name, w = h.w, h = h.h);
            let key = ["1", "2", "3", "4"].get(i).copied().unwrap_or("");
            if ui.add(egui::Button::selectable(self.setup.hoop == i, text).shortcut_text(key)).clicked() {
                self.change(|s| s.hoop = i);
                self.fit_view(canvas_rect, false);
                ui.close();
            }
        }
    }

    /// Switches to another machine family: its hoops, file format and USB
    /// folder. Picks the smallest of its hoops the design fits.
    pub(super) fn choose_machine(&mut self, i: usize, canvas_rect: egui::Rect) {
        hoops::set_machine(i);
        self.prefs.set("machine", hoops::machine().key);
        let size = self.design.as_ref().map_or((0.0, 0.0), |d| d.size());
        self.change(|s| s.hoop = hoops::smallest_fitting(size));
        self.fit_view(canvas_rect, false);
    }

    fn tool_bar(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let accent = self.accent();
        let has = self.design.is_some();
        let out = has || !self.pinned.is_empty();
        let format = hoops::machine().out.ext().to_uppercase();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if icons::button(ui, true, false, "new", t!("New project · N")) {
                self.open_wizard();
            }
            if icons::button(ui, true, false, "open", t!("Open a design, SVG or picture · O")) {
                self.pick_file();
            }
            if icons::button(ui, has, false, "save", &t!("Save as {format} · Ctrl+S", format = format)) {
                self.save_design();
            }
            if icons::button(ui, has, false, "print", t!("Save worksheet (PDF) · Ctrl+P")) {
                self.save_worksheet();
            }
            tool_gap(ui);
            if icons::button(ui, !self.undo.is_empty() || self.pending_since.is_some(), false, "undo", t!("Undo · Ctrl+Z")) {
                self.undo_step(&ctx);
            }
            if icons::button(ui, !self.redo.is_empty(), false, "redo", t!("Redo · Ctrl+Shift+Z")) {
                self.redo_step();
            }
            tool_gap(ui);
            if icons::button(ui, has, false, "turn-left", t!("Rotate 90° left · Shift+R")) {
                self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
            }
            if icons::button(ui, has, false, "turn-right", t!("Rotate 90° right · R")) {
                self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
            }
            if icons::button(ui, has, self.setup.at.mirror, "mirror", t!("Mirror · M")) {
                self.change(|s| s.at.mirror = !s.at.mirror);
            }
            if icons::button(ui, has, false, "center", t!("Center in hoop · C")) {
                self.change(|s| {
                    s.at.dx = 0.0;
                    s.at.dy = 0.0;
                });
            }
            if icons::button(ui, has, !self.pinned.is_empty(), "pin", t!("Pin to hoop: keep this design and start the next part · P")) {
                self.pin();
            }
            tool_gap(ui);
            if icons::button(ui, true, false, "fit", t!("Show the whole hoop · F")) {
                self.fit_view(canvas_rect, false);
            }
            if icons::button(ui, has, false, "zoom", t!("Zoom to the design · D")) {
                self.fit_view(canvas_rect, true);
            }
            let (icon, tip) = if self.playing { ("pause", t!("Pause the stitch-out · Space")) } else { ("play", t!("Play the stitch-out · Space")) };
            if icons::button(ui, has, self.playing, icon, tip) {
                self.toggle_play();
            }
            // Machine side on the right: the hoop, a test piece, and sending.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let send = egui::Button::new(RichText::new(t!("Send to machine")).color(contrast(accent))).fill(accent).min_size(vec2(0.0, 26.0));
                if ui.add_enabled(out, send).on_hover_text(t!("Write the {format} straight to the machine's USB stick · Ctrl+U", format = format)).clicked() {
                    self.open_usb();
                }
                if icons::button(ui, true, self.test.open, "test", t!("Test stitch-out and calibration sheet · Ctrl+T")) {
                    self.test.open = !self.test.open;
                }
                let m = hoops::machine();
                let h = hoops::get(self.setup.hoop);
                let label = format!("{}  ·  {}  {}×{}", crate::i18n::tr(m.name), h.name, h.w, h.h);
                egui::ComboBox::from_id_salt("hoop-tool")
                    .selected_text(label)
                    .width(230.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        ui.label(RichText::new(t!("Machine")).size(11.5).color(self.muted()));
                        for (i, m) in hoops::MACHINES.iter().enumerate() {
                            let text = format!("{} · {}", crate::i18n::tr(m.name), m.out.ext().to_uppercase());
                            if ui.selectable_label(hoops::machine_index() == i, text).clicked() {
                                self.choose_machine(i, canvas_rect);
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new(t!("Hoop")).size(11.5).color(self.muted()));
                        for (i, h) in hoops::list().iter().enumerate() {
                            let text = t!("{name}  {w} × {h} mm", name = h.name, w = h.w, h = h.h);
                            if ui.selectable_label(self.setup.hoop == i, text).on_hover_text(crate::i18n::tr(h.note)).clicked() {
                                self.change(|s| s.hoop = i);
                                self.fit_view(canvas_rect, false);
                            }
                        }
                    })
                    .response
                    .on_hover_text(t!("Machine and hoop: sets the hoop shown, the file format and where Send puts it"));
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let text = |s: String| RichText::new(s).size(12.0).color(muted);
        // A fact with a small icon before it.
        let stat = |ui: &mut egui::Ui, icon: &'static str, color: Color32, label: RichText| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                super::icons::paint(ui, icon, r, color);
                ui.label(label)
            })
            .inner
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            match self.hover_at {
                Some((x, y)) => ui.label(text(t!("X {v0:>6.1}  Y {v1:>6.1} mm", v0 = x / 10.0, v1 = -y / 10.0)).monospace()),
                None => ui.label(text("X      –  Y      –   ".into()).monospace()),
            };
            if let Some(d) = &self.design {
                let (w, h) = self.setup.at.size(d.size().0, d.size().1);
                stat(ui, "ruler", muted, text(t!("{v0:.1} × {v1:.1} mm", v0 = w / 10.0, v1 = h / 10.0)));
                stat(ui, "needle", muted, text(tn!(d.pattern.count(Cmd::Stitch), "{n} stitch", "{n} stitches")));
                stat(ui, "palette", muted, text(tn!(d.blocks.len(), "{n} color", "{n} colors")));
                let fits = matches!(hoops::fit(self.hoop_size(), d.size(), &self.setup.at), Fit::Inside(_));
                let (dot, word) = if fits { (OK, t!("fits")) } else { (BAD, t!("doesn't fit")) };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                    super::icons::paint(ui, "fit", r, dot);
                    ui.label(text(format!("{} {word}", hoops::get(self.setup.hoop).name)));
                });
            }
            if !self.pinned.is_empty() {
                ui.label(text(t!("{count} pinned", count = self.pinned.len())));
            }
            if let Some(r) = &self.check {
                use crate::check::Level;
                let (p, w) = (r.count(Level::Problem), r.count(Level::Warning));
                let (color, label, icon) = match (p, w) {
                    (0, 0) => (OK, t!("Check: ready to sew").to_string(), "check"),
                    (0, w) => (WARN, tn!(w, "Check: {n} warning", "Check: {n} warnings"), "warning"),
                    (p, _) => (BAD, tn!(p, "Check: {n} problem", "Check: {n} problems"), "warning"),
                };
                let r = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                        super::icons::paint(ui, icon, r, color);
                        ui.add(egui::Label::new(RichText::new(label).size(12.0).color(color)).sense(Sense::click()))
                    })
                    .inner;
                if r.on_hover_text(t!("Open the Check list")).clicked() {
                    self.dock = Dock::Check;
                    self.show_left = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(text(format!("{:.0}%", self.view.scale * 100.0 / self.hoop_scale.max(1e-6))));
                let hint = match self.mode {
                    Mode::Artwork if self.design.is_some() => {
                        t!("Drag a shape to move it · drag empty fabric to pick several · Shift+drag moves the design · middle-drag pans")
                    }
                    _ => t!("Drag the design to move it · scroll to zoom · ? for keys"),
                };
                ui.add(egui::Label::new(text(hint.into())).truncate());
            });
        });
    }
}
