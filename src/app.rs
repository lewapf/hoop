use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, Key, KeyboardShortcut, Modifiers, RichText, Sense, Vec2, vec2};

use crate::canvas::{self, DesignStyle, Frame, Transform, View};
use crate::formats::{self, Block, Cmd, Pattern};
use crate::hoops::{self, Fit, HOOPS, Placement};
use crate::lettering::{self, EdgeKind, FillKind, FontSource, Settings};
use crate::theme::Theme;

const FABRICS: [(&str, [u8; 3]); 8] = [
    ("Natural linen", [0xd6, 0xc9, 0xb0]), ("White cotton", [0xf6, 0xf5, 0xf0]),
    ("Heather gray", [0x9c, 0x9b, 0x9d]), ("Denim", [0x3e, 0x57, 0x78]), ("Navy", [0x1e, 0x2a, 0x44]),
    ("Black", [0x1c, 0x1b, 0x1d]), ("Brick red", [0x9e, 0x2a, 0x2b]), ("Sage", [0x9a, 0xab, 0x8d]),
];
const SPEEDS: [u32; 4] = [400, 600, 800, 860];
const PLAYBACK: [(f32, &str); 4] = [(1.0, "Real time"), (10.0, "10×"), (30.0, "30×"), (120.0, "120×")];

const SHORTCUTS: &[(&str, &str)] = &[
    ("O", "Open a file"),
    ("L / V", "Lettering / viewer"),
    ("Ctrl+S", "Save as JEF"),
    ("Space", "Play or pause the stitch-out"),
    ("Home / End", "Back to the first stitch / show everything"),
    ("[ / ]", "Previous / next thread"),
    (", / .", "Step one stitch back / forward"),
    ("Arrows", "Move the design 1 mm (Shift: 10 mm)"),
    ("R / Shift+R", "Rotate 90° right / left"),
    ("M", "Mirror"),
    ("C", "Center in the hoop"),
    ("1 – 4", "Pick a hoop"),
    ("F / D", "Show the whole hoop / zoom to the design"),
    ("G / J / T", "Grid / jump stitches / thread texture"),
    ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo a move"),
    ("?", "Show these shortcuts"),
    ("Q", "Quit"),
];

struct Loaded {
    name: String,
    pattern: Pattern,
    blocks: Vec<Block>,
    bounds: (f32, f32, f32, f32),
}

impl Loaded {
    fn size(&self) -> (f32, f32) {
        (self.bounds.2 - self.bounds.0, self.bounds.3 - self.bounds.1)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Viewer,
    Lettering,
}

#[derive(Clone, Copy, PartialEq)]
struct Setup {
    hoop: usize,
    at: Placement,
}

enum Drag {
    Move { start: (f32, f32), from: Placement },
    Pan,
}

pub struct HoopApp {
    theme: Theme,
    design: Option<Loaded>,
    error: Option<String>,
    setup: Setup,
    undo: Vec<Setup>,
    redo: Vec<Setup>,
    view: View,
    fit_pending: bool,
    drag: Option<Drag>,
    shown: usize,
    playing: bool,
    play_acc: f32,
    play_rate: usize,
    speed: usize,
    focus: Option<usize>,
    fabric: [u8; 3],
    grid: bool,
    jumps: bool,
    textured: bool,
    help: bool,
    picker: Option<Receiver<Option<PathBuf>>>,
    weave: Option<egui::TextureHandle>,
    canvas_rect: egui::Rect,
    user_view: bool,
    mode: Mode,
    /// The other mode's design, kept while this one is shown.
    stash: Option<Loaded>,
    letter: Settings,
    letter_built: Option<Settings>,
    fonts: Vec<FontSource>,
    font_scan: Option<Receiver<Vec<FontSource>>>,
    font_filter: String,
    saver: Option<(Receiver<Option<PathBuf>>, Vec<u8>)>,
    status: Option<String>,
}

impl HoopApp {
    pub fn new(cc: &eframe::CreationContext, file: Option<PathBuf>) -> Self {
        let mut app = HoopApp {
            theme: Theme::omarchy(),
            design: None,
            error: None,
            setup: Setup { hoop: 0, at: Placement::default() },
            undo: Vec::new(),
            redo: Vec::new(),
            view: View::default(),
            fit_pending: true,
            drag: None,
            shown: 0,
            playing: false,
            play_acc: 0.0,
            play_rate: 2,
            speed: 2,
            focus: None,
            fabric: FABRICS[0].1,
            grid: true,
            jumps: false,
            textured: true,
            help: false,
            picker: None,
            weave: None,
            canvas_rect: egui::Rect::NOTHING,
            user_view: false,
            mode: Mode::Viewer,
            stash: None,
            letter: Settings::default(),
            letter_built: None,
            fonts: lettering::bundled_fonts(),
            font_scan: None,
            font_filter: String::new(),
            saver: None,
            status: None,
        };
        let (tx, rx) = channel();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(lettering::system_fonts());
            ctx.request_repaint();
        });
        app.font_scan = Some(rx);
        load_ui_font(&cc.egui_ctx);
        app.apply_theme(&cc.egui_ctx);
        if let Some(path) = file {
            app.open(&cc.egui_ctx, &path);
        }
        app
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let p = self.theme.palette;
        let (bg, fg, accent) = (rgb(p.background), rgb(p.foreground), rgb(p.accent));
        let dark = canvas::luminance(p.background) < 0.5;
        let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = bg;
        v.window_fill = bg;
        v.extreme_bg_color = bg.lerp_to_gamma(fg, 0.06);
        v.faint_bg_color = bg.lerp_to_gamma(fg, 0.04);
        v.override_text_color = Some(fg);
        v.selection.bg_fill = accent;
        v.selection.stroke.color = contrast(accent);
        v.hyperlink_color = accent;
        v.widgets.noninteractive.bg_stroke.color = bg.lerp_to_gamma(fg, 0.15);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.bg_fill = bg.lerp_to_gamma(fg, 0.1);
            w.weak_bg_fill = bg.lerp_to_gamma(fg, 0.1);
            w.fg_stroke.color = fg;
        }
        v.widgets.hovered.bg_stroke.color = accent;
        v.widgets.active.bg_stroke.color = accent;
        ctx.set_visuals(v);
    }

    fn accent(&self) -> Color32 {
        rgb(self.theme.palette.accent)
    }

    fn muted(&self) -> Color32 {
        let p = self.theme.palette;
        rgb(p.foreground).lerp_to_gamma(rgb(p.background), 0.4)
    }

    fn open(&mut self, ctx: &egui::Context, path: &Path) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let result = std::fs::read(path)
            .map_err(|e| format!("Couldn't open {name}: {e}."))
            .and_then(|bytes| formats::read(&name, &bytes).map_err(|e| format!("{name}: {e}")));
        match result {
            Ok(pattern) => {
                let blocks = pattern.blocks();
                let bounds = pattern.bounds();
                let loaded = Loaded { name: name.clone(), pattern, blocks, bounds };
                self.setup = Setup { hoop: hoops::smallest_fitting(loaded.size()), at: Placement::default() };
                self.undo.clear();
                self.redo.clear();
                self.shown = loaded.pattern.points.len();
                self.playing = false;
                self.focus = None;
                self.design = Some(loaded);
                self.error = None;
                self.fit_pending = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{name} — Hoop")));
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn pick_file(&mut self) {
        if self.picker.is_some() {
            return;
        }
        let (tx, rx) = channel();
        // The portal dialog blocks until the user answers, so it runs off the UI thread.
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Open an embroidery design")
                .add_filter("Embroidery designs", &["jef", "dst", "exp", "pes", "pec", "JEF", "DST", "EXP", "PES", "PEC"])
                .pick_file();
            let _ = tx.send(file);
        });
        self.picker = Some(rx);
    }

    fn change(&mut self, f: impl FnOnce(&mut Setup)) {
        let before = self.setup;
        f(&mut self.setup);
        if self.setup != before {
            self.undo.push(before);
            self.redo.clear();
        }
    }

    fn hoop_size(&self) -> (f32, f32) {
        let h = HOOPS[self.setup.hoop];
        (h.w, h.h)
    }

    fn fit_view(&mut self, rect: egui::Rect, design: bool) {
        let (w, h, cx, cy) = match (&self.design, design) {
            (Some(d), true) => {
                let (dw, dh) = self.setup.at.size(d.size().0, d.size().1);
                (dw * 1.15 + 40.0, dh * 1.15 + 40.0, self.setup.at.dx, self.setup.at.dy)
            }
            _ => {
                let (hw, hh) = self.hoop_size();
                let frame = 2.0 * (canvas::OPENING_MARGIN + canvas::RING);
                (hw * 10.0 + frame + 500.0, hh * 10.0 + frame + 80.0, 0.0, 0.0)
            }
        };
        let scale = (rect.width() / w).min(rect.height() / h);
        self.view = View { scale, pan: vec2(-cx, -cy) * scale };
        // Showing the whole hoop hands framing back to the window size.
        self.user_view = design;
    }

    fn total(&self) -> usize {
        self.design.as_ref().map_or(0, |d| d.pattern.points.len())
    }

    fn toggle_play(&mut self) {
        if self.design.is_none() {
            return;
        }
        if !self.playing && self.shown >= self.total() {
            self.shown = 0;
        }
        self.playing = !self.playing;
        self.play_acc = 0.0;
    }

    fn jump_thread(&mut self, forward: bool) {
        let Some(d) = &self.design else { return };
        self.playing = false;
        let ends: Vec<usize> = std::iter::once(0).chain(d.blocks.iter().map(|b| b.end)).collect();
        self.shown = if forward {
            ends.iter().copied().find(|&e| e > self.shown).unwrap_or(self.total())
        } else {
            ends.iter().rev().copied().find(|&e| e < self.shown).unwrap_or(0)
        };
    }

    fn keys(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let press = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        let typed_question = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "?")));
        if typed_question {
            self.help = !self.help;
        }
        if press(Modifiers::NONE, Key::Escape) {
            self.help = false;
        }
        if press(Modifiers::NONE, Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if press(Modifiers::NONE, Key::O) || press(Modifiers::COMMAND, Key::O) {
            self.set_mode(Mode::Viewer);
            self.pick_file();
        }
        if press(Modifiers::COMMAND, Key::S) {
            self.save_jef();
        }
        if press(Modifiers::NONE, Key::L) {
            self.set_mode(Mode::Lettering);
        }
        if press(Modifiers::NONE, Key::V) {
            self.set_mode(Mode::Viewer);
        }
        if press(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) {
            if let Some(s) = self.redo.pop() {
                self.undo.push(self.setup);
                self.setup = s;
            }
        }
        if press(Modifiers::COMMAND, Key::Z) {
            if let Some(s) = self.undo.pop() {
                self.redo.push(self.setup);
                self.setup = s;
            }
        }
        if press(Modifiers::NONE, Key::Space) {
            self.toggle_play();
        }
        if press(Modifiers::NONE, Key::Home) {
            self.playing = false;
            self.shown = 0;
        }
        if press(Modifiers::NONE, Key::End) {
            self.playing = false;
            self.shown = self.total();
        }
        if press(Modifiers::NONE, Key::OpenBracket) {
            self.jump_thread(false);
        }
        if press(Modifiers::NONE, Key::CloseBracket) {
            self.jump_thread(true);
        }
        if press(Modifiers::NONE, Key::Comma) {
            self.playing = false;
            self.shown = self.shown.saturating_sub(1);
        }
        if press(Modifiers::NONE, Key::Period) {
            self.playing = false;
            self.shown = (self.shown + 1).min(self.total());
        }
        for (key, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            for (m, step) in [(Modifiers::NONE, 10.0), (Modifiers::SHIFT, 100.0)] {
                if press(m, key) && self.design.is_some() {
                    self.change(|s| {
                        s.at.dx += dx * step;
                        s.at.dy += dy * step;
                    });
                }
            }
        }
        if press(Modifiers::SHIFT, Key::R) {
            self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
        }
        if press(Modifiers::NONE, Key::R) {
            self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
        }
        if press(Modifiers::NONE, Key::M) {
            self.change(|s| s.at.mirror = !s.at.mirror);
        }
        if press(Modifiers::NONE, Key::C) {
            self.change(|s| {
                s.at.dx = 0.0;
                s.at.dy = 0.0;
            });
        }
        for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
            if press(Modifiers::NONE, key) {
                self.change(|s| s.hoop = i);
                self.fit_view(canvas_rect, false);
            }
        }
        if press(Modifiers::NONE, Key::F) {
            self.fit_view(canvas_rect, false);
        }
        if press(Modifiers::NONE, Key::D) {
            self.fit_view(canvas_rect, true);
        }
        if press(Modifiers::NONE, Key::G) {
            self.grid = !self.grid;
        }
        if press(Modifiers::NONE, Key::J) {
            self.jumps = !self.jumps;
        }
        if press(Modifiers::NONE, Key::T) {
            self.textured = !self.textured;
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let muted = self.muted();
        let accent = self.accent();
        let viewer = self.mode == Mode::Viewer;
        let heading = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(10.0);
            ui.label(RichText::new(t.to_uppercase()).size(11.0).color(accent).strong());
            ui.add_space(2.0);
        };

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Hoop").size(24.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (mode, name, key) in [(Mode::Lettering, "Lettering", "L"), (Mode::Viewer, "Viewer", "V")] {
                    let on = self.mode == mode;
                    let text = RichText::new(name).color(if on { contrast(accent) } else { ui.visuals().text_color() });
                    let b = egui::Button::new(text).fill(if on { accent } else { Color32::TRANSPARENT }).corner_radius(12);
                    if ui.add(b).on_hover_text(key).clicked() {
                        self.set_mode(mode);
                    }
                }
            });
        });
        ui.add_space(6.0);
        if self.mode == Mode::Lettering {
            self.lettering_panel(ui);
        } else {
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Open…").color(contrast(accent))).fill(accent)).on_hover_text("O").clicked() {
                    self.pick_file();
                }
                if self.design.is_some() && ui.button("Save as JEF…").on_hover_text("Ctrl+S").clicked() {
                    self.save_jef();
                }
            });
        }
        if let Some(st) = &self.status {
            ui.label(RichText::new(st).color(Color32::from_rgb(0x55, 0xc2, 0x8c)));
        }
        if self.mode == Mode::Lettering {
        } else { match &self.design {
            Some(d) => {
                ui.label(RichText::new(&d.name).monospace());
            }
            None => {
                ui.label(RichText::new("No design open. Press O or drop a JEF, DST, EXP, PES or PEC file here.").color(muted));
            }
        } }
        if let Some(e) = &self.error {
            ui.colored_label(Color32::from_rgb(0xf0, 0x6b, 0x5f), e);
        }

        if let Some(d) = &self.design {
            ui.add_space(8.0);
            let size = d.size();
            let fit = hoops::fit(self.hoop_size(), size, &self.setup.at);
            let (ok, title, detail) = match fit {
                Fit::Inside(m) => (true, format!("Fits the {}", HOOPS[self.setup.hoop].name), format!("Closest edge is {m:.1} mm away.")),
                Fit::TooBig { over_w, over_h, fits_turned } => {
                    let mut parts = Vec::new();
                    if over_w > 0.0 { parts.push(format!("{over_w:.1} mm too wide")); }
                    if over_h > 0.0 { parts.push(format!("{over_h:.1} mm too tall")); }
                    let hint = if fits_turned { " Rotate it 90° (R) and it fits." } else { " Pick a bigger hoop." };
                    (false, "Doesn't fit this hoop".into(), format!("{}.{hint}", parts.join(", ")))
                }
                Fit::OffEdge { side, by } => (false, "Off the sewing area".into(), format!("Sticks out {by:.1} mm on the {side}. Press C to center.")),
            };
            let color = if ok { Color32::from_rgb(0x55, 0xc2, 0x8c) } else { Color32::from_rgb(0xf0, 0x6b, 0x5f) };
            egui::Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(6).inner_margin(10).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(title).color(color).strong());
                ui.label(RichText::new(detail).color(muted));
            });
        }

        heading(ui, "Hoop size");
        let design_size = self.design.as_ref().map(|d| d.size());
        egui::Grid::new("hoops").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, h) in HOOPS.iter().enumerate() {
                let fits = design_size.is_none_or(|s| {
                    !matches!(hoops::fit((h.w, h.h), s, &Placement::default()), Fit::TooBig { fits_turned: false, .. })
                });
                let text = format!("{}  {}\n{} × {} mm\n{}", i + 1, h.name, h.w, h.h, if fits { h.note } else { "Too small" });
                let selected = self.setup.hoop == i;
                let mut button = egui::Button::new(RichText::new(text).size(12.5)).min_size(vec2(148.0, 54.0));
                if selected {
                    button = button.fill(accent.gamma_multiply(0.25)).stroke(egui::Stroke::new(1.0, accent));
                }
                if ui.add(button).clicked() {
                    self.change(|s| s.hoop = i);
                    self.fit_view(canvas_rect, false);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });

        heading(ui, "Position");
        ui.horizontal_wrapped(|ui| {
            if ui.button("⟲ 90°").on_hover_text("Shift+R").clicked() {
                self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
            }
            if ui.button("⟳ 90°").on_hover_text("R").clicked() {
                self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
            }
            if ui.button("Mirror").on_hover_text("M").clicked() {
                self.change(|s| s.at.mirror = !s.at.mirror);
            }
            if ui.button("Center").on_hover_text("C").clicked() {
                self.change(|s| { s.at.dx = 0.0; s.at.dy = 0.0; });
            }
        });
        let at = self.setup.at;
        let signed = |v: f32| format!("{}{:.1}", if v > 0.0 { "+" } else if v < 0.0 { "−" } else { "" }, (v / 10.0).abs());
        ui.label(RichText::new(format!(
            "Rotation {}°{} · X {} Y {} mm",
            at.quarter_turns as u32 * 90,
            if at.mirror { ", mirrored" } else { "" },
            signed(at.dx),
            signed(-at.dy)
        )).monospace().size(11.5).color(muted));

        if let Some(d) = &mut self.design {
            heading(ui, "Thread order");
            let mut toggle = None;
            for (k, b) in d.blocks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if viewer {
                        ui.color_edit_button_srgb(&mut b.thread.color);
                    } else {
                        let (r, _) = ui.allocate_exact_size(vec2(40.0, 18.0), Sense::hover());
                        ui.painter().rect_filled(r, 4, rgb(b.thread.color));
                    }
                    let selected = self.focus == Some(k);
                    let code = if b.thread.code.is_empty() { String::new() } else { format!(" {}", b.thread.code) };
                    let label = RichText::new(format!("{}. {}{}", k + 1, b.thread.name, code));
                    if ui.selectable_label(selected, label).on_hover_text("Show this thread on its own").clicked() {
                        toggle = Some(k);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{}", b.stitches)).monospace().color(muted));
                    });
                });
            }
            if let Some(k) = toggle {
                self.focus = if self.focus == Some(k) { None } else { Some(k) };
            }
            if !d.pattern.format.stores_colors() {
                ui.label(RichText::new(format!("{} files don't store thread colors, so these are placeholders.", d.pattern.format)).size(12.0).color(muted));
            }
        }

        if self.design.is_some() {
            heading(ui, "Stitch-out");
            let total = self.total();
            ui.horizontal(|ui| {
                let label = if self.playing { "Pause" } else if self.shown >= total { "Replay" } else { "Play" };
                if ui.add(egui::Button::new(label).min_size(vec2(64.0, 0.0))).on_hover_text("Space").clicked() {
                    self.toggle_play();
                }
                ui.spacing_mut().slider_width = ui.available_width() - 8.0;
                let mut n = self.shown;
                if ui.add(egui::Slider::new(&mut n, 0..=total).show_value(false)).changed() {
                    self.playing = false;
                    self.shown = n;
                }
            });
            ui.horizontal(|ui| {
                let thread = self.design.as_ref().and_then(|d| d.blocks.iter().position(|b| self.shown <= b.end)).unwrap_or(0) + 1;
                let text = if self.shown >= total { format!("All {total} stitches") } else { format!("{} / {total} · thread {thread}", self.shown) };
                ui.label(RichText::new(text).monospace().size(11.5).color(muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("rate").selected_text(PLAYBACK[self.play_rate].1).width(90.0).show_ui(ui, |ui| {
                        for (i, (_, name)) in PLAYBACK.iter().enumerate() {
                            ui.selectable_value(&mut self.play_rate, i, *name);
                        }
                    });
                });
            });
        }

        if let Some(d) = &self.design {
            heading(ui, "Details");
            let p = &d.pattern;
            let stitches = p.count(Cmd::Stitch);
            let changes = d.blocks.len().saturating_sub(1);
            let minutes = stitches as f32 / SPEEDS[self.speed] as f32;
            let (w, h) = d.size();
            let time = format!(
                "≈ {} min{}",
                if minutes < 1.0 { "<1".into() } else { format!("{}", minutes.round()) },
                if changes > 0 { format!(" + {changes} thread change{}", if changes > 1 { "s" } else { "" }) } else { String::new() }
            );
            let trims = p.count(Cmd::Trim);
            let rows = [
                ("Size", format!("{:.1} × {:.1} mm", w / 10.0, h / 10.0)),
                ("Stitches", stitches.to_string()),
                ("Colors", d.blocks.len().to_string()),
                ("Jumps", if trims > 0 { format!("{} · {trims} trims", p.count(Cmd::Jump)) } else { p.count(Cmd::Jump).to_string() }),
                ("Sewing time", time),
                ("Format", p.format.to_string()),
            ];
            egui::Grid::new("details").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(RichText::new(k).color(muted));
                    ui.label(RichText::new(v).monospace());
                    ui.end_row();
                }
                ui.label(RichText::new("Machine speed").color(muted));
                egui::ComboBox::from_id_salt("spm").selected_text(format!("{} spm", SPEEDS[self.speed])).show_ui(ui, |ui| {
                    for (i, s) in SPEEDS.iter().enumerate() {
                        ui.selectable_value(&mut self.speed, i, format!("{s} spm"));
                    }
                });
                ui.end_row();
            });
        }

        heading(ui, "Display");
        ui.horizontal_wrapped(|ui| {
            for (name, c) in FABRICS {
                let selected = self.fabric == c;
                let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                ui.painter().circle(rect.center(), 11.0, rgb(c), egui::Stroke::new(if selected { 2.5 } else { 1.0 }, if selected { accent } else { muted }));
                if resp.on_hover_text(name).clicked() {
                    self.fabric = c;
                }
            }
            ui.color_edit_button_srgb(&mut self.fabric).on_hover_text("Other fabric color");
        });
        ui.checkbox(&mut self.textured, "Thread texture (T)");
        ui.checkbox(&mut self.grid, "Template grid, 10 mm (G)");
        ui.checkbox(&mut self.jumps, "Jump stitches (J)");
        ui.add_space(10.0);
        ui.label(RichText::new("Press ? for every shortcut.").size(12.0).color(muted));
        ui.add_space(12.0);
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // Keep the hoop framed as the window resizes until the user zooms or pans.
        if rect.size() != self.canvas_rect.size() && !self.user_view {
            self.fit_pending = true;
        }
        self.canvas_rect = rect;
        if self.fit_pending && rect.width() > 10.0 {
            self.fit_view(rect, false);
            self.fit_pending = false;
        }
        let painter = ui.painter_at(rect);
        let frame = Frame { painter: &painter, rect, view: self.view };

        // Interaction: drag the design to move it, drag elsewhere to pan, scroll to zoom.
        if response.drag_started() {
            let over = response.interact_pointer_pos().map(|p| frame.to_world(p));
            self.drag = match (over, &self.design) {
                (Some(w), Some(d)) => {
                    let (dw, dh) = self.setup.at.size(d.size().0, d.size().1);
                    let at = self.setup.at;
                    if (w.0 - at.dx).abs() <= dw / 2.0 + 20.0 && (w.1 - at.dy).abs() <= dh / 2.0 + 20.0 {
                        Some(Drag::Move { start: w, from: at })
                    } else {
                        Some(Drag::Pan)
                    }
                }
                _ => Some(Drag::Pan),
            };
        }
        if response.dragged() {
            match &self.drag {
                Some(Drag::Move { start, from }) => {
                    if let Some(p) = response.interact_pointer_pos() {
                        let w = frame.to_world(p);
                        // Snap to 0.5 mm, the step the machine's jog keys use.
                        self.setup.at.dx = ((from.dx + w.0 - start.0) / 5.0).round() * 5.0;
                        self.setup.at.dy = ((from.dy + w.1 - start.1) / 5.0).round() * 5.0;
                    }
                }
                Some(Drag::Pan) => {
                    self.view.pan += response.drag_delta();
                    self.user_view = true;
                }
                None => {}
            }
        }
        if response.drag_stopped() {
            if let Some(Drag::Move { from, .. }) = self.drag.take() {
                if from != self.setup.at {
                    let mut before = self.setup;
                    before.at = from;
                    self.undo.push(before);
                    self.redo.clear();
                }
            }
        }
        if response.double_clicked() {
            self.fit_view(rect, false);
        }
        if response.hovered() {
            let (scroll, zoom, pointer) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.pointer.hover_pos()));
            let factor = zoom * (scroll * 0.0025).exp();
            if (factor - 1.0).abs() > 1e-4 {
                if let Some(p) = pointer {
                    let old = self.view.scale;
                    let new = (old * factor).clamp(0.05, 40.0);
                    let anchor = p - rect.center();
                    self.view.pan = anchor - (anchor - self.view.pan) * (new / old);
                    self.view.scale = new;
                    self.user_view = true;
                }
            }
        }

        let frame = Frame { painter: &painter, rect, view: self.view };
        let weave = self.weave.get_or_insert_with(|| {
            ui.ctx().load_texture("weave", canvas::weave_image(), egui::TextureOptions {
                wrap_mode: egui::TextureWrapMode::Repeat,
                ..egui::TextureOptions::NEAREST
            })
        });
        canvas::draw_fabric(&frame, self.fabric, weave);
        let hoop = HOOPS[self.setup.hoop];
        canvas::draw_hoop(&frame, hoop.w * 10.0, hoop.h * 10.0, &format!("{}  {} × {} mm", hoop.name, hoop.w, hoop.h), self.fabric, self.grid);

        if let Some(d) = &self.design {
            let t = Transform::new(d.bounds, &self.setup.at);
            let style = DesignStyle { textured: self.textured, show_jumps: self.jumps, focus: self.focus, fabric: self.fabric };
            canvas::draw_design(&frame, &d.pattern, &d.blocks, &t, self.shown, &style);
            let size = self.setup.at.size(d.size().0, d.size().1);
            let off = !matches!(hoops::fit(self.hoop_size(), d.size(), &self.setup.at), Fit::Inside(_));
            if off {
                canvas::draw_bounds(&frame, size, &self.setup.at, Color32::from_rgb(0xe0, 0x36, 0x2c));
            } else if matches!(self.drag, Some(Drag::Move { .. })) {
                canvas::draw_bounds(&frame, size, &self.setup.at, self.accent());
            }
        } else {
            painter.text(
                rect.center_bottom() - vec2(0.0, 28.0),
                egui::Align2::CENTER_CENTER,
                "Press O to open a design, or drop a file here",
                egui::FontId::proportional(15.0),
                Color32::from_white_alpha(230),
            );
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        std::mem::swap(&mut self.design, &mut self.stash);
        self.mode = mode;
        self.playing = false;
        self.focus = None;
        self.error = None;
        self.status = None;
        self.shown = self.total();
    }

    fn regenerate(&mut self, ctx: &egui::Context) {
        self.letter_built = Some(self.letter.clone());
        let font = &self.fonts[self.letter.font.min(self.fonts.len() - 1)];
        match lettering::generate(font, &self.letter) {
            Ok(pattern) => {
                let blocks = pattern.blocks();
                let bounds = pattern.bounds();
                let first = self.design.is_none();
                let name = self.letter.text.lines().next().unwrap_or("").trim().to_string();
                self.design = Some(Loaded { name, pattern, blocks, bounds });
                if first {
                    let size = self.design.as_ref().unwrap().size();
                    self.setup.hoop = hoops::smallest_fitting(size);
                }
                self.shown = self.total();
                self.playing = false;
                self.error = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title("Lettering — Hoop".into()));
            }
            Err(e) => {
                self.design = None;
                self.error = Some(e);
            }
        }
    }

    fn lettering_panel(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        let heading = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(10.0);
            ui.label(RichText::new(t.to_uppercase()).size(11.0).color(accent).strong());
            ui.add_space(2.0);
        };
        let s = &mut self.letter;
        heading(ui, "Text");
        ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(2).desired_width(f32::INFINITY).font(egui::TextStyle::Heading));

        heading(ui, "Font");
        let current = self.fonts.get(s.font).map_or("", |f| f.name.as_str()).to_string();
        egui::ComboBox::from_id_salt("font").selected_text(current).width(ui.available_width()).height(380.0).show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.font_filter).hint_text("Search fonts"));
            let filter = self.font_filter.to_lowercase();
            let mut was_featured = true;
            for (i, f) in self.fonts.iter().enumerate() {
                if !filter.is_empty() && !f.name.to_lowercase().contains(&filter) {
                    continue;
                }
                if was_featured && !f.featured {
                    ui.separator();
                    was_featured = false;
                }
                ui.selectable_value(&mut s.font, i, &f.name);
            }
            if self.font_scan.is_some() {
                ui.label(RichText::new("Looking for installed fonts…").color(muted));
            }
        });
        let slider = |ui: &mut egui::Ui, v: &mut f32, r: std::ops::RangeInclusive<f32>, text: &str, suffix: &str| {
            ui.add(egui::Slider::new(v, r).text(text).suffix(suffix).fixed_decimals(1));
        };
        slider(ui, &mut s.height_mm, 5.0..=120.0, "Height", " mm");
        slider(ui, &mut s.spacing_mm, -5.0..=15.0, "Letter spacing", " mm");
        if s.text.lines().count() > 1 {
            slider(ui, &mut s.line_spacing, 0.8..=2.5, "Line spacing", "×");
        }
        ui.add(egui::Slider::new(&mut s.arc_deg, -180.0..=180.0).text("Arc").suffix("°").fixed_decimals(0))
            .on_hover_text("Bend the text into an arch (positive) or a smile (negative)");

        heading(ui, "Fill");
        ui.horizontal(|ui| {
            for (k, name) in [(FillKind::Tatami, "Tatami"), (FillKind::Satin, "Satin"), (FillKind::None, "None")] {
                ui.selectable_value(&mut s.fill, k, name);
            }
        });
        if s.fill != FillKind::None {
            ui.add(egui::Slider::new(&mut s.fill_angle, 0.0..=180.0).text("Angle").suffix("°").fixed_decimals(0));
            slider(ui, &mut s.density_mm, 0.25..=1.0, "Row spacing", " mm");
            if s.fill == FillKind::Tatami {
                slider(ui, &mut s.stitch_mm, 2.0..=6.0, "Stitch length", " mm");
            } else {
                ui.label(RichText::new("Satin looks best on strokes under 8 mm wide. Wider rows get split.").size(12.0).color(muted));
            }
            ui.checkbox(&mut s.underlay, "Underlay (runs around each letter first)");
            jef_picker(ui, "Fill thread", &mut s.fill_thread);
        }

        heading(ui, "Edge");
        ui.horizontal(|ui| {
            for (k, name) in [(EdgeKind::Satin, "Satin"), (EdgeKind::Running, "Run"), (EdgeKind::Triple, "Triple run"), (EdgeKind::None, "None")] {
                ui.selectable_value(&mut s.edge, k, name);
            }
        });
        if s.edge == EdgeKind::Satin {
            slider(ui, &mut s.edge_width_mm, 0.8..=5.0, "Width", " mm");
        }
        if s.edge != EdgeKind::None {
            jef_picker(ui, "Edge thread", &mut s.edge_thread);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("Save as JEF…").color(contrast(accent))).fill(accent)).on_hover_text("Ctrl+S").clicked() {
                self.save_jef();
            }
            if ui.button("Reset").clicked() {
                let font = self.letter.font;
                self.letter = Settings { font, ..Default::default() };
            }
        });
    }

    fn save_jef(&mut self) {
        let Some(d) = &self.design else { return };
        if self.saver.is_some() {
            return;
        }
        // Rotation and mirroring are baked in; the offset isn't, because the
        // machine positions designs from the hoop center itself.
        let at = Placement { dx: 0.0, dy: 0.0, ..self.setup.at };
        let t = Transform::new(d.bounds, &at);
        let points: Vec<(f32, f32)> = d.pattern.points.iter().map(|&p| t.apply(p)).collect();
        let palette: Vec<(i32, formats::Thread)> = formats::jef_palette().collect();
        let threads: Vec<i32> = d
            .blocks
            .iter()
            .map(|b| palette.iter().find(|(_, t)| *t == b.thread).map_or_else(|| formats::nearest_jef(b.thread.color), |(i, _)| *i))
            .collect();
        let bytes = formats::write_jef(&points, &d.pattern.cmds, &threads);
        let stem: String = match self.mode {
            Mode::Lettering => d.name.chars().filter(|c| c.is_alphanumeric() || *c == ' ').take(24).collect::<String>().trim().replace(' ', "_"),
            Mode::Viewer => {
                let stem = d.name.rsplit_once('.').map_or(d.name.as_str(), |(s, _)| s).to_string();
                if d.pattern.format == formats::Format::Jef { format!("{stem}-hoop") } else { stem }
            }
        };
        let name = format!("{}.jef", if stem.is_empty() { "lettering" } else { &stem });
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let dir = [home.join("Documents/Embroidery Designs"), home.join("Documents"), home]
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_default();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Save as JEF")
                .set_directory(dir)
                .set_file_name(name)
                .add_filter("Janome JEF", &["jef"])
                .save_file();
            let _ = tx.send(file);
        });
        self.saver = Some((rx, bytes));
    }

    fn write_file(&mut self, mut path: PathBuf, bytes: &[u8]) {
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("jef")) {
            path.set_extension("jef");
        }
        // Write beside the target and rename, so a failed save never leaves half a file.
        let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
        let result = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path));
        match result {
            Ok(()) => {
                self.status = Some(format!("Saved {}", path.display()));
                self.error = None;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.error = Some(format!("Couldn't save {}: {e}.", path.display()));
            }
        }
    }

    fn help_overlay(&mut self, ctx: &egui::Context) {
        if !self.help {
            return;
        }
        let muted = self.muted();
        egui::Window::new("Shortcuts")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Grid::new("keys").num_columns(2).spacing([18.0, 5.0]).show(ui, |ui| {
                    for (k, what) in SHORTCUTS {
                        ui.label(RichText::new(*k).monospace().strong());
                        ui.label(*what);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Drag the design to move it, drag the fabric to pan, scroll to zoom, double-click to show the whole hoop.").size(12.0).color(muted));
                ui.label(RichText::new("Press ? or Esc to close.").size(12.0).color(muted));
            });
    }
}

impl eframe::App for HoopApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        if self.theme.refresh() {
            self.apply_theme(ctx);
        }
        // Pick up theme switches even when the window is idle.
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        if let Some(rx) = &self.picker {
            match rx.try_recv() {
                Ok(file) => {
                    self.picker = None;
                    if let Some(path) = file {
                        self.open(ctx, &path);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(_) => self.picker = None,
            }
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().next().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.open(ctx, &path);
        }

        if self.playing {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            self.play_acc += dt * SPEEDS[self.speed] as f32 / 60.0 * PLAYBACK[self.play_rate].0;
            let step = self.play_acc.floor();
            self.play_acc -= step;
            self.shown = (self.shown + step as usize).min(self.total());
            if self.shown >= self.total() {
                self.playing = false;
            }
            ctx.request_repaint();
        }

        if let Some(rx) = &self.font_scan {
            if let Ok(found) = rx.try_recv() {
                self.fonts.extend(found);
                self.font_scan = None;
            }
        }
        if let Some((rx, _)) = &self.saver {
            match rx.try_recv() {
                Ok(target) => {
                    let (_, bytes) = self.saver.take().unwrap();
                    if let Some(path) = target {
                        self.write_file(path, &bytes);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(_) => self.saver = None,
            }
        }
        if self.mode == Mode::Lettering && self.letter_built.as_ref() != Some(&self.letter) {
            self.regenerate(ctx);
        }
        let canvas_rect = self.canvas_rect;
        self.keys(ctx, canvas_rect);

        let panel = egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin::symmetric(16, 0));
        egui::Panel::right("controls").resizable(false).exact_size(340.0).frame(panel).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.side_panel(ui, canvas_rect));
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas(ui));
        self.help_overlay(ctx);
    }
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Text color that reads on top of `c`.
fn contrast(c: Color32) -> Color32 {
    if canvas::luminance([c.r(), c.g(), c.b()]) > 0.55 { Color32::from_rgb(0x14, 0x12, 0x13) } else { Color32::WHITE }
}

/// A button showing the chosen Janome thread that opens the whole chart.
fn jef_picker(ui: &mut egui::Ui, label: &str, value: &mut i32) {
    let current = formats::jef_thread(*value, 0);
    ui.horizontal(|ui| {
        ui.label(label);
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 8.0, rgb(current.color));
        ui.menu_button(format!("{} {}", current.name, current.code), |ui| {
            egui::Grid::new(label).spacing([4.0, 4.0]).show(ui, |ui| {
                for (n, (i, t)) in formats::jef_palette().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    let stroke = if i == *value { egui::Stroke::new(2.0, ui.visuals().selection.bg_fill) } else { egui::Stroke::new(1.0, Color32::from_black_alpha(80)) };
                    ui.painter().rect(r, 4, rgb(t.color), stroke, egui::StrokeKind::Inside);
                    if resp.on_hover_text(format!("{} {}", t.name, t.code)).clicked() {
                        *value = i;
                        ui.close();
                    }
                    if n % 9 == 8 {
                        ui.end_row();
                    }
                }
            });
        });
    });
}

/// Uses the Omarchy monospace font for the interface, like Omarchy's own apps.
fn load_ui_font(ctx: &egui::Context) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            use egui::{FontId, TextStyle};
            s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.5));
            s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
            s.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
            s.spacing.item_spacing = vec2(8.0, 6.0);
            s.spacing.button_padding = vec2(10.0, 5.0);
        });
    }
    let run = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let Some(family) = run("omarchy", &["font", "current"]) else { return };
    let Some(file) = run("fc-match", &["-f", "%{file}", &family]) else { return };
    let Ok(bytes) = std::fs::read(&file) else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("omarchy".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().insert(0, "omarchy".into());
    }
    ctx.set_fonts(fonts);
}
