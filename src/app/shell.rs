//! The window around the hoop: menu bar, tool bar, the Objects/Colors dock
//! on the left, the tabbed properties dock on the right and the status bar.

use super::*;
use super::shapes::Arrange;

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
    fn label(self) -> &'static str {
        match self {
            Tab::Design => "Design",
            Tab::Text => "Text",
            Tab::Style => "Style",
            Tab::Patch => "Patch",
            Tab::Source => "Source",
            Tab::Shape => "Shape",
            Tab::Threads => "Threads",
            Tab::Hoop => "Hoop",
            Tab::Stitch => "Stitch",
            Tab::View => "View",
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

/// Pages of the left dock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dock {
    Objects,
    Colors,
    Check,
}

/// A row of tabs like a Qt tab widget: the picked one is raised and
/// underlined. Tabs that don't fit wrap onto another row.
fn tab_row<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)], accent: Color32) {
    let raised = ui.visuals().panel_fill.lerp_to_gamma(ui.visuals().text_color(), 0.07);
    let line = ui.visuals().widgets.noninteractive.bg_stroke.color;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for &(v, label) in options {
            let on = *value == v;
            let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), ui.visuals().text_color());
            let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 12.0, 28.0), Sense::click());
            let p = ui.painter();
            if on {
                p.rect_filled(rect, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, raised);
                p.hline(rect.x_range(), rect.bottom() - 1.0, egui::Stroke::new(2.0, accent));
            } else {
                if resp.hovered() {
                    p.rect_filled(rect, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, raised.gamma_multiply(0.6));
                }
                p.hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, line));
            }
            let color = if on { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
            p.galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
            if resp.clicked() {
                *value = v;
            }
        }
    });
}

/// A flat tool button that shows its frame on hover, like a Qt tool bar.
fn tool(ui: &mut egui::Ui, enabled: bool, text: &str, hover: &str) -> bool {
    ui.add_enabled(enabled, egui::Button::new(text).frame_when_inactive(false).min_size(vec2(0.0, 26.0))).on_hover_text(hover).clicked()
}

fn item(ui: &mut egui::Ui, enabled: bool, text: &str, keys: &str) -> bool {
    let clicked = ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(keys)).clicked();
    if clicked {
        ui.close();
    }
    clicked
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
        let dock = egui::Frame::new().fill(fill).inner_margin(egui::Margin { left: 8, right: 8, top: 6, bottom: 0 });
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
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas(ui));
    }

    fn left_dock(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent();
        let check = match &self.check {
            Some(r) if r.count(crate::check::Level::Problem) > 0 => format!("Check ({})", r.count(crate::check::Level::Problem)),
            _ => "Check".to_string(),
        };
        let mut options = vec![(Dock::Colors, "Colors"), (Dock::Check, check.as_str())];
        if self.mode == Mode::Artwork {
            options.insert(0, (Dock::Objects, "Objects"));
        } else if self.dock == Dock::Objects {
            self.dock = Dock::Colors;
        }
        tab_row(ui, &mut self.dock, &options, accent);
        ui.add_space(6.0);
        match self.dock {
            Dock::Objects => self.objects_list(ui),
            Dock::Colors => {
                egui::ScrollArea::vertical().id_salt("colors").auto_shrink([false, false]).show(ui, |ui| self.colors_list(ui));
            }
            Dock::Check => self.check_list(ui),
        }
    }

    fn right_dock(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let accent = self.accent();
        let m = self.mode as usize;
        let options: Vec<(Tab, &str)> = tabs(self.mode).iter().map(|&t| (t, t.label())).collect();
        if !tabs(self.mode).contains(&self.tab[m]) {
            self.tab[m] = tabs(self.mode)[0];
        }
        tab_row(ui, &mut self.tab[m], &options, accent);
        ui.add_space(8.0);
        let tab = self.tab[m];
        egui::ScrollArea::vertical().id_salt(("page", m, tab as usize)).auto_shrink([false, false]).show(ui, |ui| {
            // Leave room for the scroll bar and never grow past the panel.
            ui.set_max_width(ui.available_width() - 10.0);
            self.side_panel(ui, canvas_rect, tab);
        });
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let has = self.design.is_some();
        let out = has || !self.pinned.is_empty();
        let art = self.mode == Mode::Artwork && self.art.is_some();
        let picked = art && !self.sel.is_empty();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if item(ui, true, "Open…", "O") {
                    self.pick_file();
                }
                let recent: Vec<PathBuf> = self.prefs.recent().into_iter().filter(|p| p.exists()).take(8).collect();
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button("Open recent", |ui| {
                        for p in recent {
                            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                                self.open_later = Some(p);
                                ui.close();
                            }
                        }
                    });
                });
                if item(ui, true, "New from shapes", "") {
                    self.picture = None;
                    self.use_artwork(Ok(artwork::blank()));
                }
                if item(ui, true, "Paste SVG code…", "Ctrl+V") {
                    self.set_mode(Mode::Artwork);
                    self.tab[Mode::Artwork as usize] = Tab::Source;
                    self.paste_open = true;
                }
                ui.separator();
                if item(ui, has, "Save as JEF…", "Ctrl+S") {
                    self.save_jef();
                }
                if item(ui, out, "Send to USB…", "Ctrl+U") {
                    self.open_usb();
                }
                if item(ui, true, "Test stitch-out…", "Ctrl+T") {
                    self.test.open = true;
                }
                if item(ui, has, "Save picture…", "Ctrl+Shift+S") {
                    self.save_picture();
                }
                ui.separator();
                if item(ui, true, "Quit", "Q") {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if item(ui, !self.undo.is_empty() || self.pending_since.is_some(), "Undo", "Ctrl+Z") {
                    self.undo_step(&ctx);
                }
                if item(ui, !self.redo.is_empty(), "Redo", "Ctrl+Shift+Z") {
                    self.redo_step();
                }
                ui.separator();
                for (text, keys, op, on) in [
                    ("Select all shapes", "Ctrl+A", Arrange::SelectAll, art),
                    ("Deselect", "Esc", Arrange::Deselect, picked),
                    ("Duplicate", "Ctrl+D", Arrange::Duplicate, picked),
                    ("Delete", "Del", Arrange::Delete, picked),
                    ("Hide", "Ctrl+H", Arrange::Hide, picked),
                    ("Show all shapes", "", Arrange::ShowAll, art),
                ] {
                    if item(ui, on, text, keys) {
                        self.arrange(op);
                    }
                }
            });
            ui.menu_button("View", |ui| {
                if item(ui, true, "Whole hoop", "F") {
                    self.fit_view(canvas_rect, false);
                }
                if item(ui, has, "Zoom to design", "D") {
                    self.fit_view(canvas_rect, true);
                }
                ui.separator();
                check(ui, &mut self.grid, "Template grid", "G");
                check(ui, &mut self.jumps, "Jump stitches", "J");
                check(ui, &mut self.textured, "Thread texture", "T");
                check(ui, &mut self.sizes, "Size label", "");
                check(ui, &mut self.density, "Thread density", "H");
                ui.separator();
                check(ui, &mut self.show_left, "Objects and colors panel", "");
                check(ui, &mut self.show_right, "Properties panel", "");
            });
            ui.menu_button("Design", |ui| {
                for (m, text, key) in [(Mode::Viewer, "Viewer", "V"), (Mode::Lettering, "Lettering", "L"), (Mode::Artwork, "Artwork", "A")] {
                    if ui.add(egui::Button::selectable(self.mode == m, text).shortcut_text(key)).clicked() {
                        self.set_mode(m);
                        ui.close();
                    }
                }
                ui.separator();
                if item(ui, has, "Rotate right", "R") {
                    self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
                }
                if item(ui, has, "Rotate left", "Shift+R") {
                    self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
                }
                if item(ui, has, "Mirror", "M") {
                    self.change(|s| s.at.mirror = !s.at.mirror);
                }
                if item(ui, has, "Center in hoop", "C") {
                    self.change(|s| {
                        s.at.dx = 0.0;
                        s.at.dy = 0.0;
                    });
                }
                ui.separator();
                ui.menu_button("Hoop", |ui| {
                    for (i, h) in HOOPS.iter().enumerate() {
                        let text = format!("{}  {} × {} mm", h.name, h.w, h.h);
                        if ui.add(egui::Button::selectable(self.setup.hoop == i, text).shortcut_text(if i < 4 { ["1", "2", "3", "4"][i] } else { "" })).clicked() {
                            self.change(|s| s.hoop = i);
                            self.fit_view(canvas_rect, false);
                            ui.close();
                        }
                    }
                });
                if item(ui, has, "Pin to hoop", "P") {
                    self.pin();
                }
                if item(ui, !self.pinned.is_empty(), "Clear pinned parts", "") {
                    self.pinned.clear();
                }
            });
            if self.mode == Mode::Artwork {
                ui.menu_button("Shape", |ui| {
                    ui.menu_button("Insert", |ui| {
                        for name in artwork::BUILT_IN {
                            if ui.button(name).clicked() {
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
                        ("Bring to front", "Shift+]", Arrange::Front),
                        ("Bring forward", "]", Arrange::Forward),
                        ("Send backward", "[", Arrange::Backward),
                        ("Send to back", "Shift+[", Arrange::Back),
                    ] {
                        if item(ui, picked, text, keys) {
                            self.arrange(op);
                        }
                    }
                    ui.separator();
                    ui.add_enabled_ui(picked, |ui| {
                        ui.menu_button("Align", |ui| {
                            for (text, op) in [
                                ("Left edges", Arrange::Left),
                                ("Centers across", Arrange::CenterX),
                                ("Right edges", Arrange::Right),
                                ("Top edges", Arrange::Top),
                                ("Middles up and down", Arrange::Middle),
                                ("Bottom edges", Arrange::Bottom),
                            ] {
                                if item(ui, true, text, "") {
                                    self.arrange(op);
                                }
                            }
                        });
                    });
                    if item(ui, self.sel.len() == 1, "Edit points", "") {
                        self.edit_points = !self.edit_points;
                        if let (Some(i), Some(a), Some(s)) = (self.primary(), &self.artwork, &mut self.art) {
                            s.shapes[i].bake(a);
                        }
                        self.tab[Mode::Artwork as usize] = Tab::Shape;
                    }
                });
            }
            ui.menu_button("Help", |ui| {
                if item(ui, true, "Keyboard shortcuts", "?") {
                    self.help = true;
                }
                if item(ui, true, "About Hoop", "") {
                    self.toast(format!("Hoop {} · embroidery for the Janome MC9900", env!("CARGO_PKG_VERSION")));
                }
            });
        });
        if let Some(name) = self.list_add.take() {
            self.add_built_in(name);
        }
    }

    fn tool_bar(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let accent = self.accent();
        let has = self.design.is_some();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if tool(ui, true, "Open", "Open a design, SVG or picture · O") {
                self.pick_file();
            }
            if tool(ui, has, "Save", "Save as JEF · Ctrl+S") {
                self.save_jef();
            }
            if tool(ui, has || !self.pinned.is_empty(), "To USB", "Write the JEF straight to the machine's USB stick · Ctrl+U") {
                self.open_usb();
            }
            if tool(ui, true, "Test", "Sew a small part first, or the calibration sheet · Ctrl+T") {
                self.test.open = !self.test.open;
            }
            ui.separator();
            if tool(ui, !self.undo.is_empty() || self.pending_since.is_some(), "Undo", "Ctrl+Z") {
                self.undo_step(&ctx);
            }
            if tool(ui, !self.redo.is_empty(), "Redo", "Ctrl+Shift+Z") {
                self.redo_step();
            }
            ui.separator();
            for (m, text, key) in [(Mode::Viewer, "Viewer", "V"), (Mode::Lettering, "Lettering", "L"), (Mode::Artwork, "Artwork", "A")] {
                let on = self.mode == m;
                let label = if on { RichText::new(text).color(contrast(accent)) } else { RichText::new(text) };
                let b = egui::Button::new(label).fill(if on { accent } else { Color32::TRANSPARENT }).frame_when_inactive(on).min_size(vec2(0.0, 26.0));
                if ui.add(b).on_hover_text(key).clicked() {
                    self.set_mode(m);
                }
            }
            ui.separator();
            if tool(ui, has, "Turn left", "Rotate 90° left · Shift+R") {
                self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
            }
            if tool(ui, has, "Turn right", "Rotate 90° right · R") {
                self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
            }
            if tool(ui, has, "Mirror", "M") {
                self.change(|s| s.at.mirror = !s.at.mirror);
            }
            if tool(ui, has, "Center", "C") {
                self.change(|s| {
                    s.at.dx = 0.0;
                    s.at.dy = 0.0;
                });
            }
            if tool(ui, has, "Pin", "Keep this design on the hoop and start the next part · P") {
                self.pin();
            }
            ui.separator();
            let h = HOOPS[self.setup.hoop];
            egui::ComboBox::from_id_salt("hoop-tool").selected_text(format!("{}  {}×{}", h.name, h.w, h.h)).width(150.0).show_ui(ui, |ui| {
                for (i, h) in HOOPS.iter().enumerate() {
                    if ui.selectable_label(self.setup.hoop == i, format!("{}  {} × {} mm", h.name, h.w, h.h)).clicked() {
                        self.change(|s| s.hoop = i);
                        self.fit_view(canvas_rect, false);
                    }
                }
            });
            ui.separator();
            if tool(ui, true, "Hoop", "Show the whole hoop · F") {
                self.fit_view(canvas_rect, false);
            }
            if tool(ui, has, "Design", "Zoom to the design · D") {
                self.fit_view(canvas_rect, true);
            }
            ui.separator();
            let label = if self.playing { "Pause" } else { "Play" };
            if tool(ui, has, label, "Play the stitch-out · Space") {
                self.toggle_play();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let text = |s: String| RichText::new(s).size(12.0).color(muted);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            match self.hover_at {
                Some((x, y)) => ui.label(text(format!("X {:>6.1}  Y {:>6.1} mm", x / 10.0, -y / 10.0)).monospace()),
                None => ui.label(text("X      –  Y      –   ".into()).monospace()),
            };
            if let Some(d) = &self.design {
                let (w, h) = self.setup.at.size(d.size().0, d.size().1);
                ui.label(text(format!("{:.1} × {:.1} mm", w / 10.0, h / 10.0)));
                ui.label(text(format!("{} stitches", d.pattern.count(Cmd::Stitch))));
                ui.label(text(format!("{} color{}", d.blocks.len(), if d.blocks.len() == 1 { "" } else { "s" })));
                let fits = matches!(hoops::fit(self.hoop_size(), d.size(), &self.setup.at), Fit::Inside(_));
                let (dot, word) = if fits { (OK, "fits") } else { (BAD, "doesn't fit") };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, dot);
                    ui.label(text(format!("{} {word}", HOOPS[self.setup.hoop].name)));
                });
            }
            if !self.pinned.is_empty() {
                ui.label(text(format!("{} pinned", self.pinned.len())));
            }
            if let Some(r) = &self.check {
                use crate::check::Level;
                let (p, w) = (r.count(Level::Problem), r.count(Level::Warning));
                let (color, label) = match (p, w) {
                    (0, 0) => (OK, "Check: ready to sew".to_string()),
                    (0, w) => (WARN, format!("Check: {w} warning{}", if w == 1 { "" } else { "s" })),
                    (p, _) => (BAD, format!("Check: {p} problem{}", if p == 1 { "" } else { "s" })),
                };
                if ui.add(egui::Label::new(RichText::new(label).size(12.0).color(color)).sense(Sense::click())).on_hover_text("Open the Check list").clicked() {
                    self.dock = Dock::Check;
                    self.show_left = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(text(format!("{:.0}%", self.view.scale * 100.0 / self.hoop_scale.max(1e-6))));
                let hint = match self.mode {
                    Mode::Artwork if self.design.is_some() => "Drag a shape to move it · drag empty fabric to pick several · Shift+drag moves the design · middle-drag pans",
                    _ => "Drag the design to move it · scroll to zoom · ? for keys",
                };
                ui.add(egui::Label::new(text(hint.into())).truncate());
            });
        });
    }
}
