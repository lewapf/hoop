use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, Key, KeyboardShortcut, Modifiers, RichText, Sense, Vec2, vec2};

use crate::canvas::{self, DesignStyle, Frame, Transform, View};
use crate::formats::{self, Block, Cmd, Pattern};
use crate::hoops::{self, Fit, HOOPS, Placement};
use crate::artwork::{self, ArtSettings};
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
    ("V / L / A", "Viewer / lettering / artwork"),
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
    Artwork,
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
    /// Lets background threads wake the window when their work is done.
    ctx: egui::Context,
    user_view: bool,
    mode: Mode,
    /// The other mode's design, kept while this one is shown.
    stash: [Option<Loaded>; 3],
    artwork: Option<artwork::Artwork>,
    art: Option<ArtSettings>,
    art_built: Option<ArtSettings>,
    art_slow: bool,
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
            ctx: cc.egui_ctx.clone(),
            user_view: false,
            mode: Mode::Viewer,
            stash: [None, None, None],
            artwork: None,
            art: None,
            art_built: None,
            art_slow: false,
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
        let ctx = cc.egui_ctx.clone();
        app.theme.watch(move || ctx.request_repaint());
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
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz")) {
            self.set_mode(Mode::Artwork);
            match artwork::load(path) {
                Ok(art) => {
                    self.art = Some(ArtSettings::for_artwork(&art));
                    self.art_built = None;
                    self.artwork = Some(art);
                    self.design = None;
                    self.error = None;
                    self.fit_pending = true;
                    self.user_view = false;
                }
                Err(e) => self.error = Some(e),
            }
            return;
        }
        self.set_mode(Mode::Viewer);
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
        let ctx = self.ctx.clone();
        // The portal dialog blocks until the user answers, so it runs off the UI thread.
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Open an embroidery design")
                .add_filter("Designs and artwork", &["jef", "dst", "exp", "pes", "pec", "svg", "svgz", "JEF", "DST", "EXP", "PES", "PEC", "SVG"])
                .pick_file();
            let _ = tx.send(file);
            ctx.request_repaint();
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
            self.pick_file();
        }
        if press(Modifiers::COMMAND, Key::S) {
            self.save_jef();
        }
        if press(Modifiers::NONE, Key::A) {
            self.set_mode(Mode::Artwork);
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

        ui.add_space(14.0);
        ui.label(RichText::new("Hoop").size(26.0).strong());
        ui.add_space(4.0);
        let mut mode = self.mode;
        segmented(ui, &mut mode, &[(Mode::Viewer, "Viewer  V"), (Mode::Lettering, "Lettering  L"), (Mode::Artwork, "Artwork  A")], accent);
        self.set_mode(mode);
        ui.add_space(10.0);

        if self.mode == Mode::Lettering {
            self.lettering_panel(ui);
        } else if self.mode == Mode::Artwork {
            self.artwork_panel(ui, canvas_rect);
        } else {
            card(ui, "Design", accent, |ui| {
                match &self.design {
                    Some(d) => {
                        ui.label(RichText::new(&d.name).strong().size(15.0));
                        let (w, h) = d.size();
                        ui.label(RichText::new(format!("{} · {:.1} × {:.1} mm", d.pattern.format, w / 10.0, h / 10.0)).color(muted));
                    }
                    None => {
                        ui.label(RichText::new("Open a JEF, DST, EXP, PES or PEC file, or drop one on the hoop.").color(muted));
                    }
                }
                ui.add_space(4.0);
                let w = (ui.available_width() - 8.0) / 2.0;
                ui.horizontal(|ui| {
                    if ui.add(primary("Open…", accent).min_size(vec2(w, 30.0))).on_hover_text("O").clicked() {
                        self.pick_file();
                    }
                    if ui.add_enabled(self.design.is_some(), egui::Button::new("Save as JEF…").min_size(vec2(w, 30.0))).on_hover_text("Ctrl+S").clicked() {
                        self.save_jef();
                    }
                });
            });
        }
        if let Some(st) = &self.status {
            ui.label(RichText::new(st).color(OK));
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(BAD));
        }

        if let Some(d) = &self.design {
            let fit = hoops::fit(self.hoop_size(), d.size(), &self.setup.at);
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
            let color = if ok { OK } else { BAD };
            egui::Frame::new().fill(color.gamma_multiply(0.12)).stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5))).corner_radius(10).inner_margin(12).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, color);
                    ui.label(RichText::new(title).color(color).strong());
                });
                ui.label(RichText::new(detail).color(muted));
            });
            ui.add_space(10.0);
        }

        card(ui, "Hoop", accent, |ui| {
            let design_size = self.design.as_ref().map(|d| d.size());
            let w = (ui.available_width() - 8.0) / 2.0;
            for row in HOOPS.chunks(2).enumerate() {
                ui.horizontal(|ui| {
                    for (j, h) in row.1.iter().enumerate() {
                        let i = row.0 * 2 + j;
                        let fits = design_size.is_none_or(|s| {
                            !matches!(hoops::fit((h.w, h.h), s, &Placement::default()), Fit::TooBig { fits_turned: false, .. })
                        });
                        if hoop_tile(ui, w, i, h, fits, self.setup.hoop == i, accent, muted).clicked() {
                            self.change(|s| s.hoop = i);
                            self.fit_view(canvas_rect, false);
                        }
                    }
                });
            }
        });

        card(ui, "Position", accent, |ui| {
            let w = (ui.available_width() - 18.0) / 4.0;
            ui.horizontal(|ui| {
                let b = |t: &str| egui::Button::new(t).min_size(vec2(w, 28.0));
                if ui.add(b("⟲ 90°")).on_hover_text("Rotate left · Shift+R").clicked() {
                    self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
                }
                if ui.add(b("⟳ 90°")).on_hover_text("Rotate right · R").clicked() {
                    self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
                }
                if ui.add(b("Mirror")).on_hover_text("M").clicked() {
                    self.change(|s| s.at.mirror = !s.at.mirror);
                }
                if ui.add(b("Center")).on_hover_text("C").clicked() {
                    self.change(|s| { s.at.dx = 0.0; s.at.dy = 0.0; });
                }
            });
            let at = self.setup.at;
            let signed = |v: f32| format!("{}{:.1}", if v > 0.0 { "+" } else if v < 0.0 { "−" } else { "" }, (v / 10.0).abs());
            ui.label(RichText::new(format!(
                "{}°{}  ·  X {}  Y {} mm",
                at.quarter_turns as u32 * 90,
                if at.mirror { " mirrored" } else { "" },
                signed(at.dx),
                signed(-at.dy)
            )).monospace().size(12.0).color(muted));
        });

        if self.design.is_some() {
            let focus = self.focus;
            let mut toggle = None;
            let d = self.design.as_mut().unwrap();
            card(ui, "Threads", accent, |ui| {
                for (k, b) in d.blocks.iter_mut().enumerate() {
                    let selected = focus == Some(k);
                    let fill = if selected { accent.gamma_multiply(0.18) } else { Color32::TRANSPARENT };
                    let r = egui::Frame::new().fill(fill).corner_radius(6).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if viewer {
                                ui.color_edit_button_srgb(&mut b.thread.color);
                            } else {
                                let (r, _) = ui.allocate_exact_size(vec2(28.0, 18.0), Sense::hover());
                                ui.painter().rect_filled(r, 9, rgb(b.thread.color));
                            }
                            ui.label(RichText::new(format!("{}", k + 1)).monospace().color(muted));
                            ui.label(&b.thread.name);
                            if !b.thread.code.is_empty() {
                                ui.label(RichText::new(&b.thread.code).monospace().color(muted));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(b.stitches.to_string()).monospace().color(muted));
                            });
                        });
                    });
                    if ui.interact(r.response.rect, ui.id().with(("thread", k)), Sense::click()).on_hover_text("Click to show this thread on its own").clicked() {
                        toggle = Some(k);
                    }
                }
                if !d.pattern.format.stores_colors() {
                    ui.label(RichText::new(format!("{} files don't store thread colors. Click a swatch to set the real ones.", d.pattern.format)).size(12.0).color(muted));
                }
            });
            if let Some(k) = toggle {
                self.focus = if self.focus == Some(k) { None } else { Some(k) };
            }

            let total = self.total();
            card(ui, "Stitch-out", accent, |ui| {
                ui.horizontal(|ui| {
                    let label = if self.playing { "Pause" } else if self.shown >= total { "Replay" } else { "Play" };
                    if ui.add(primary(label, accent).min_size(vec2(72.0, 28.0))).on_hover_text("Space").clicked() {
                        self.toggle_play();
                    }
                    ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(40.0);
                    let mut n = self.shown;
                    if ui.add(egui::Slider::new(&mut n, 0..=total).show_value(false)).changed() {
                        self.playing = false;
                        self.shown = n;
                    }
                });
                ui.horizontal(|ui| {
                    let thread = self.design.as_ref().and_then(|d| d.blocks.iter().position(|b| self.shown <= b.end)).unwrap_or(0) + 1;
                    let text = if self.shown >= total { format!("All {total} stitches") } else { format!("{} / {total} · thread {thread}", self.shown) };
                    ui.label(RichText::new(text).monospace().size(12.0).color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("rate").selected_text(PLAYBACK[self.play_rate].1).width(84.0).show_ui(ui, |ui| {
                            for (i, (_, name)) in PLAYBACK.iter().enumerate() {
                                ui.selectable_value(&mut self.play_rate, i, *name);
                            }
                        });
                    });
                });
            });

            let d = self.design.as_ref().unwrap();
            let p = &d.pattern;
            let stitches = p.count(Cmd::Stitch);
            let changes = d.blocks.len().saturating_sub(1);
            let minutes = stitches as f32 / SPEEDS[self.speed] as f32;
            let (w, h) = d.size();
            let time = format!(
                "≈ {} min{}",
                if minutes < 1.0 { "<1".into() } else { format!("{}", minutes.round()) },
                if changes > 0 { format!(" + {changes} change{}", if changes > 1 { "s" } else { "" }) } else { String::new() }
            );
            let trims = p.count(Cmd::Trim);
            let rows = [
                ("Size", format!("{:.1} × {:.1} mm", w / 10.0, h / 10.0)),
                ("Stitches", stitches.to_string()),
                ("Colors", d.blocks.len().to_string()),
                ("Jumps", if trims > 0 { format!("{} · {trims} trims", p.count(Cmd::Jump)) } else { p.count(Cmd::Jump).to_string() }),
                ("Sewing time", time),
            ];
            card(ui, "Details", accent, |ui| {
                for (k, v) in rows {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).color(muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(v).monospace()));
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Machine speed").color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("spm").selected_text(format!("{} spm", SPEEDS[self.speed])).width(96.0).show_ui(ui, |ui| {
                            for (i, s) in SPEEDS.iter().enumerate() {
                                ui.selectable_value(&mut self.speed, i, format!("{s} spm"));
                            }
                        });
                    });
                });
            });
        }

        card(ui, "Display", accent, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (name, c) in FABRICS {
                    let selected = self.fabric == c;
                    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                    if selected {
                        ui.painter().circle_stroke(rect.center(), 12.5, egui::Stroke::new(2.0, accent));
                    }
                    ui.painter().circle(rect.center(), 10.0, rgb(c), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
                    if resp.on_hover_text(name).clicked() {
                        self.fabric = c;
                    }
                }
                ui.color_edit_button_srgb(&mut self.fabric).on_hover_text("Other fabric color");
            });
            ui.checkbox(&mut self.textured, "Thread texture  T");
            ui.checkbox(&mut self.grid, "Template grid, 10 mm  G");
            ui.checkbox(&mut self.jumps, "Jump stitches  J");
        });
        ui.label(RichText::new("Press ? for every shortcut.").size(12.0).color(muted));
        ui.add_space(14.0);
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

        // Overlay controls in the canvas corners.
        let chip = |t: &str| {
            egui::Button::new(RichText::new(t.to_string()).color(Color32::WHITE).size(12.5))
                .fill(Color32::from_black_alpha(165))
                .stroke(egui::Stroke::NONE)
                .corner_radius(7)
        };
        let br = rect.right_bottom() - vec2(12.0, 12.0);
        if ui.put(egui::Rect::from_min_max(br - vec2(132.0, 30.0), br), chip("Whole hoop  F")).clicked() {
            self.fit_view(rect, false);
        }
        if self.design.is_some() && ui.put(egui::Rect::from_min_max(br - vec2(240.0, 30.0), br - vec2(140.0, 0.0)), chip("Design  D")).clicked() {
            self.fit_view(rect, true);
        }
        let hint = "Drag design to move · scroll to zoom · ? for keys";
        let galley = painter.layout_no_wrap(hint.into(), egui::FontId::proportional(12.0), Color32::WHITE);
        let at = rect.left_bottom() + vec2(12.0, -12.0 - galley.size().y - 12.0);
        let bg = egui::Rect::from_min_size(at, galley.size() + vec2(20.0, 12.0));
        if bg.right() < br.x - 252.0 {
            painter.rect_filled(bg, 7, Color32::from_black_alpha(140));
            painter.galley(at + vec2(10.0, 6.0), galley, Color32::WHITE);
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        self.stash[self.mode as usize] = self.design.take();
        self.design = self.stash[mode as usize].take();
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
        let s = &mut self.letter;
        card(ui, "Text", accent, |ui| {
            ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(2).desired_width(f32::INFINITY).font(egui::TextStyle::Heading));
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
        });
        card(ui, "Size", accent, |ui| {
            slider_row(ui, "Height", &mut s.height_mm, 5.0..=120.0, " mm", 1);
            slider_row(ui, "Spacing", &mut s.spacing_mm, -5.0..=15.0, " mm", 1);
            if s.text.lines().count() > 1 {
                slider_row(ui, "Lines", &mut s.line_spacing, 0.8..=2.5, "×", 2);
            }
            slider_row(ui, "Arc", &mut s.arc_deg, -180.0..=180.0, "°", 0)
                .on_hover_text("Bend the text into an arch (positive) or a smile (negative)");
        });
        fill_card(ui, s, muted, accent, true);
        edge_card(ui, s, accent, "Edge", true);
        let w = ui.available_width();
        ui.horizontal(|ui| {
            if ui.add(primary("Save as JEF…", accent).min_size(vec2(w - 88.0, 32.0))).on_hover_text("Ctrl+S").clicked() {
                self.save_jef();
            }
            if ui.add(egui::Button::new("Reset").min_size(vec2(80.0, 32.0))).clicked() {
                let font = self.letter.font;
                self.letter = Settings { font, ..Default::default() };
            }
        });
        ui.add_space(10.0);
    }

    fn regenerate_art(&mut self, ctx: &egui::Context) {
        let (Some(art), Some(s)) = (&self.artwork, &self.art) else { return };
        self.art_built = Some(s.clone());
        let started = std::time::Instant::now();
        let result = artwork::generate(art, s);
        self.art_slow = started.elapsed().as_millis() > 120;
        match result {
            Ok(pattern) => {
                let blocks = pattern.blocks();
                let bounds = pattern.bounds();
                let first = self.design.is_none();
                self.design = Some(Loaded { name: art.name.clone(), pattern, blocks, bounds });
                if first {
                    self.setup.hoop = hoops::smallest_fitting(self.design.as_ref().unwrap().size());
                    self.fit_pending = !self.user_view;
                }
                self.shown = self.total();
                self.playing = false;
                self.error = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{} — Hoop", art.name)));
            }
            Err(e) => {
                self.design = None;
                self.error = Some(e);
            }
        }
    }

    fn artwork_panel(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let muted = self.muted();
        let accent = self.accent();
        let has_art = self.artwork.is_some();
        card(ui, "Artwork", accent, |ui| {
            match &self.artwork {
                Some(a) => {
                    ui.label(RichText::new(&a.name).strong().size(15.0));
                    let shapes = a.shapes.len();
                    ui.label(RichText::new(format!("{shapes} shape{} · {} color{}", if shapes == 1 { "" } else { "s" }, a.layers.len(), if a.layers.len() == 1 { "" } else { "s" })).color(muted));
                    if a.skipped_images > 0 {
                        ui.label(RichText::new(format!("Skipped {} embedded picture{}. Only vector shapes can be stitched.", a.skipped_images, if a.skipped_images == 1 { "" } else { "s" })).size(12.0).color(muted));
                    }
                }
                None => {
                    ui.label(RichText::new("Open an SVG, or drop one on the hoop. Logos and clip art with flat colors work best.").color(muted));
                }
            }
            ui.add_space(4.0);
            let w = (ui.available_width() - 8.0) / 2.0;
            ui.horizontal(|ui| {
                if ui.add(primary("Open SVG…", accent).min_size(vec2(w, 30.0))).on_hover_text("O").clicked() {
                    self.pick_file();
                }
                if ui.add_enabled(self.design.is_some(), egui::Button::new("Save as JEF…").min_size(vec2(w, 30.0))).on_hover_text("Ctrl+S").clicked() {
                    self.save_jef();
                }
            });
        });
        if !has_art {
            return;
        }
        let hoop = self.hoop_size();
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
        card(ui, "Size", accent, |ui| {
            slider_row(ui, "Width", &mut s.width_mm, 10.0..=360.0, " mm", 1);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Height {:.1} mm", s.width_mm * art.aspect())).color(muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Fit hoop").on_hover_text("Largest size that fits the hoop with 5 mm to spare").clicked() {
                        let by_w = hoop.0 - 10.0;
                        let by_h = (hoop.1 - 10.0) / art.aspect().max(1e-3);
                        s.width_mm = by_w.min(by_h).floor().max(10.0);
                    }
                });
            });
        });

        card(ui, "Threads", accent, |ui| {
            let groups = artwork::thread_groups(art, s);
            for (g, (thread, members)) in groups.iter().enumerate() {
                let mut on = members.iter().all(|&i| s.layers[i].enabled);
                let mut t = *thread;
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, "").on_hover_text("Stitch these colors").changed() {
                        for &i in members {
                            s.layers[i].enabled = on;
                        }
                    }
                    // The SVG colors that use this thread, up to four.
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for &i in members.iter().take(4) {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 22.0), Sense::hover());
                        ui.painter().rect_filled(r.shrink2(vec2(0.0, 3.0)), 3, rgb(art.layers[i].color));
                    }
                    if members.len() > 4 {
                        ui.label(RichText::new(format!("+{}", members.len() - 4)).size(11.0).color(muted));
                    }
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new("→").color(muted));
                    thread_menu(ui, ("group", g), &mut t);
                    if members.iter().any(|&i| art.layers[i].background) {
                        ui.label(RichText::new("background").size(11.0).color(muted));
                    }
                });
                if t != *thread {
                    for &i in members {
                        s.layers[i].thread = t;
                    }
                }
            }
            let used = groups.iter().filter(|g| g.1.iter().any(|&i| s.layers[i].enabled)).count();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{used} thread{} in use", if used == 1 { "" } else { "s" })).color(muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for n in [2usize, 4, 6, 8] {
                        if n < used && ui.small_button(n.to_string()).on_hover_text(format!("Merge the closest threads until {n} are left")).clicked() {
                            artwork::reduce_threads(art, s, n);
                        }
                    }
                    if used > 3 {
                        ui.label(RichText::new("Reduce to").size(12.0).color(muted));
                    }
                });
            });
            ui.checkbox(&mut s.group_colors, "Group by thread (fewer thread changes)")
                .on_hover_text("Off: stitch in the drawing's own order, changing thread whenever it changes");
        });

        fill_card(ui, &mut s.style, muted, accent, false);
        card(ui, "Clean-up", accent, |ui| {
            ui.checkbox(&mut s.knockout, "Cut hidden parts").on_hover_text("Leave areas covered by shapes on top unstitched, so layers don't pile up");
            slider_row(ui, "Detail", &mut s.min_detail_mm, 0.0..=5.0, " mm", 1).on_hover_text("Shapes smaller than this many mm across are left out");
        });
        edge_card(ui, &mut s.style, accent, "Outline on fills", false);
        card(ui, "Lines", accent, |ui| {
            ui.label(RichText::new("How strokes in the SVG are stitched").size(12.0).color(muted));
            segmented(ui, &mut s.stroke, &[(EdgeKind::Satin, "Satin"), (EdgeKind::Running, "Run"), (EdgeKind::Triple, "Triple"), (EdgeKind::None, "None")], accent);
            if s.stroke == EdgeKind::Satin {
                ui.add_space(4.0);
                slider_row(ui, "Min", &mut s.stroke_min_mm, 0.5..=4.0, " mm", 1).on_hover_text("Thinnest satin line; thinner strokes are widened to this");
            }
        });
        if ui.button("Reset settings").clicked() {
            self.art = Some(ArtSettings::for_artwork(art));
        }
        let _ = canvas_rect;
        ui.add_space(10.0);
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
            Mode::Artwork => d.name.rsplit_once('.').map_or(d.name.as_str(), |(s, _)| s).to_string(),
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
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Save as JEF")
                .set_directory(dir)
                .set_file_name(name)
                .add_filter("Janome JEF", &["jef"])
                .save_file();
            let _ = tx.send(file);
            ctx.request_repaint();
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

        if let Some(rx) = &self.picker {
            match rx.try_recv() {
                Ok(file) => {
                    self.picker = None;
                    if let Some(path) = file {
                        self.open(ctx, &path);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
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
            // Without vsync this caps playback at about 60 frames a second.
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
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
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.saver = None,
            }
        }
        if self.mode == Mode::Lettering && self.letter_built.as_ref() != Some(&self.letter) {
            self.regenerate(ctx);
        }
        // Big artwork can take a moment to stitch, so while a slider is being
        // dragged it waits for the release.
        let dragging = ctx.input(|i| i.pointer.any_down());
        if self.mode == Mode::Artwork && self.art.is_some() && self.art_built != self.art && !(self.art_slow && dragging) {
            self.regenerate_art(ctx);
        }
        let canvas_rect = self.canvas_rect;
        self.keys(ctx, canvas_rect);

        let panel = egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin { left: 14, right: 6, top: 0, bottom: 0 });
        egui::Panel::right("controls").resizable(false).exact_size(370.0).frame(panel).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // Leave room for the scroll bar and never grow past the panel.
                ui.set_max_width(ui.available_width() - 10.0);
                self.side_panel(ui, canvas_rect);
            });
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

const OK: Color32 = Color32::from_rgb(0x55, 0xc2, 0x8c);
const BAD: Color32 = Color32::from_rgb(0xf0, 0x6b, 0x5f);

/// A rounded section with a small heading; every panel section uses one.
fn card(ui: &mut egui::Ui, title: &str, accent: Color32, add: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals();
    let fill = v.panel_fill.lerp_to_gamma(v.text_color(), 0.05);
    egui::Frame::new().fill(fill).corner_radius(10).inner_margin(12).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title.to_uppercase()).size(11.0).color(accent).strong());
        ui.add_space(2.0);
        add(ui);
    });
    ui.add_space(10.0);
}

fn primary(text: &str, accent: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(contrast(accent)).strong()).fill(accent)
}

/// Equal-width options in one row, the chosen one filled with the accent.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)], accent: Color32) {
    let track = ui.visuals().extreme_bg_color;
    egui::Frame::new().fill(track).corner_radius(8).inner_margin(3).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let w = (ui.available_width() - 4.0 * (options.len() - 1) as f32) / options.len() as f32;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (v, name) in options {
                let on = *value == *v;
                let text = RichText::new(*name).color(if on { contrast(accent) } else { ui.visuals().text_color() });
                let b = egui::Button::new(text)
                    .fill(if on { accent } else { Color32::TRANSPARENT })
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(6)
                    .min_size(vec2(w, 26.0));
                if ui.add(b).clicked() {
                    *value = *v;
                }
            }
        });
    });
}

/// Label, slider and an editable value, lined up the same in every row.
fn slider_row(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, decimals: usize) -> egui::Response {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label(label);
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 86.0).max(40.0);
        let r = ui.add(egui::Slider::new(v, range.clone()).show_value(false));
        let d = ui.add_sized([78.0, 22.0], egui::DragValue::new(v).range(range).suffix(suffix).fixed_decimals(decimals).speed(0.1));
        r | d
    })
    .inner
}

#[allow(clippy::too_many_arguments)]
fn hoop_tile(ui: &mut egui::Ui, w: f32, i: usize, h: &hoops::Hoop, fits: bool, selected: bool, accent: Color32, muted: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 58.0), Sense::click());
    let v = ui.visuals();
    let fill = if selected { accent.gamma_multiply(0.2) } else if resp.hovered() { v.widgets.hovered.weak_bg_fill } else { v.extreme_bg_color };
    let stroke = if selected { egui::Stroke::new(1.5, accent) } else { egui::Stroke::NONE };
    let p = ui.painter();
    p.rect(rect, 8, fill, stroke, egui::StrokeKind::Inside);
    let text = v.text_color();
    let x = rect.left() + 10.0;
    p.text(egui::pos2(x, rect.top() + 9.0), egui::Align2::LEFT_TOP, h.name, egui::FontId::proportional(15.0), text);
    p.text(egui::pos2(rect.right() - 10.0, rect.top() + 10.0), egui::Align2::RIGHT_TOP, (i + 1).to_string(), egui::FontId::monospace(11.0), muted);
    p.text(egui::pos2(x, rect.top() + 28.0), egui::Align2::LEFT_TOP, format!("{} × {} mm", h.w, h.h), egui::FontId::monospace(12.0), muted);
    let (note, c) = if fits { (h.note, muted) } else { ("Too small", BAD) };
    p.text(egui::pos2(x, rect.top() + 43.0), egui::Align2::LEFT_TOP, note, egui::FontId::proportional(11.0), c);
    resp.on_hover_text(format!("Hoop {} · key {}", h.name, i + 1))
}

/// A small button naming a Janome thread that opens the chart.
fn thread_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug + Copy, value: &mut i32) {
    let current = formats::jef_thread(*value, 0);
    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    ui.painter().circle(r.center(), 7.0, rgb(current.color), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
    ui.menu_button(format!("{} {}", current.name, current.code), |ui| {
        egui::Grid::new(("chart", ui.id().with(id))).spacing([4.0, 4.0]).show(ui, |ui| {
            for (n, (i, t)) in formats::jef_palette().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                let stroke = if i == *value { egui::Stroke::new(2.5, ui.visuals().selection.bg_fill) } else { egui::Stroke::new(1.0, Color32::from_black_alpha(80)) };
                ui.painter().rect(r, 5, rgb(t.color), stroke, egui::StrokeKind::Inside);
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
}

/// Fill style picker: tiles with a small drawing of each pattern.
fn fill_tiles(ui: &mut egui::Ui, value: &mut FillKind, accent: Color32) {
    use egui::{Pos2, Shape, Stroke, pos2};
    const STYLES: [(FillKind, &str); 8] = [
        (FillKind::Tatami, "Tatami"), (FillKind::Satin, "Satin"), (FillKind::Wave, "Wave"), (FillKind::Zigzag, "Zigzag"),
        (FillKind::Crosshatch, "Hatch"), (FillKind::Gradient, "Gradient"), (FillKind::Open, "Open"), (FillKind::None, "None"),
    ];
    let w = (ui.available_width() - 18.0) / 4.0;
    for row in STYLES.chunks(4) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for &(kind, name) in row {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 62.0), Sense::click());
                let v = ui.visuals();
                let on = *value == kind;
                let fill = if on { accent.gamma_multiply(0.2) } else if resp.hovered() { v.widgets.hovered.weak_bg_fill } else { v.extreme_bg_color };
                let p = ui.painter();
                p.rect(rect, 8, fill, if on { Stroke::new(1.5, accent) } else { Stroke::NONE }, egui::StrokeKind::Inside);
                let icon = egui::Rect::from_min_size(rect.min + vec2(8.0, 7.0), vec2(w - 16.0, 30.0));
                let ink = if on { accent } else { v.text_color().gamma_multiply(0.75) };
                let p = p.with_clip_rect(icon);
                let line = |a: Pos2, b: Pos2, width: f32| p.line_segment([a, b], Stroke::new(width, ink));
                let (l, r, t, bt) = (icon.left(), icon.right(), icon.top(), icon.bottom());
                match kind {
                    FillKind::Tatami | FillKind::Satin => {
                        let step = if kind == FillKind::Satin { 2.5 } else { 4.0 };
                        let mut x = l - 40.0;
                        let mut k = 0;
                        while x < r {
                            if kind == FillKind::Tatami {
                                // Short stitches with staggered needle points.
                                let mut y = t - 10.0 + (k % 3) as f32 * 3.0;
                                while y < bt {
                                    line(pos2(x + (y - t), y), pos2(x + (y - t) + 6.0, y + 6.0), 1.2);
                                    y += 8.0;
                                }
                            } else {
                                line(pos2(x, t), pos2(x + 30.0, bt), 1.2);
                            }
                            x += step;
                            k += 1;
                        }
                    }
                    FillKind::Wave | FillKind::Zigzag | FillKind::Open | FillKind::Gradient => {
                        let mut y = t + 2.0;
                        let mut gap = 4.0;
                        while y < bt + 4.0 {
                            let pts: Vec<Pos2> = (0..=40)
                                .map(|i| {
                                    let x = l + (r - l) * i as f32 / 40.0;
                                    let u = (x - l) / 16.0;
                                    let dy = match kind {
                                        FillKind::Wave => 2.5 * (u * std::f32::consts::TAU).sin(),
                                        FillKind::Zigzag => 2.5 * (4.0 * (u - (u + 0.5).floor()).abs() - 1.0),
                                        _ => 0.0,
                                    };
                                    pos2(x, y + dy)
                                })
                                .collect();
                            p.add(Shape::line(pts, Stroke::new(1.2, ink)));
                            y += match kind {
                                FillKind::Open => 7.0,
                                FillKind::Gradient => {
                                    gap += 1.2;
                                    gap - 1.2
                                }
                                _ => 4.0,
                            };
                        }
                    }
                    FillKind::Crosshatch => {
                        let mut x = l - 40.0;
                        while x < r {
                            line(pos2(x, t), pos2(x + 30.0, bt), 1.0);
                            line(pos2(x + 30.0, t), pos2(x, bt), 1.0);
                            x += 7.0;
                        }
                    }
                    FillKind::None => {
                        line(pos2(l + 6.0, bt - 4.0), pos2(r - 6.0, t + 4.0), 1.2);
                    }
                }
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() - 13.0),
                    egui::Align2::CENTER_CENTER,
                    name,
                    egui::FontId::proportional(12.0),
                    if on { accent } else { v.text_color() },
                );
                if resp.clicked() {
                    *value = kind;
                }
            }
        });
    }
}

/// The chosen Janome thread as a full-width button that opens the chart.
fn jef_picker(ui: &mut egui::Ui, id: &str, value: &mut i32) {
    let current = formats::jef_thread(*value, 0);
    let row = vec2(ui.available_width(), 26.0);
    ui.allocate_ui_with_layout(row, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label("Thread");
        });
        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        ui.painter().circle(r.center(), 9.0, rgb(current.color), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
        ui.menu_button(format!("{}  {}", current.name, current.code), |ui| {
            egui::Grid::new(("chart", id)).spacing([4.0, 4.0]).show(ui, |ui| {
                for (n, (i, t)) in formats::jef_palette().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    let stroke = if i == *value { egui::Stroke::new(2.5, ui.visuals().selection.bg_fill) } else { egui::Stroke::new(1.0, Color32::from_black_alpha(80)) };
                    ui.painter().rect(r, 5, rgb(t.color), stroke, egui::StrokeKind::Inside);
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

/// Fill style tiles and the settings of the chosen style.
fn fill_card(ui: &mut egui::Ui, s: &mut Settings, muted: Color32, accent: Color32, with_thread: bool) {
    card(ui, "Fill", accent, |ui| {
        let before = s.fill;
        fill_tiles(ui, &mut s.fill, accent);
        if s.fill != before {
            s.density_mm = s.fill.default_rows_mm();
            // An outline under an open fill would show, so it starts off there.
            s.underlay = !matches!(s.fill, FillKind::Open | FillKind::Crosshatch);
        }
        if s.fill != FillKind::None {
            ui.add_space(4.0);
            slider_row(ui, "Angle", &mut s.fill_angle, 0.0..=180.0, "°", 0);
            slider_row(ui, "Rows", &mut s.density_mm, 0.25..=3.0, " mm", 2).on_hover_text("Distance between fill rows");
            if s.fill == FillKind::Gradient {
                slider_row(ui, "Fade", &mut s.fade_mm, 0.5..=6.0, " mm", 1).on_hover_text("Row spacing at the light end");
            }
            if matches!(s.fill, FillKind::Wave | FillKind::Zigzag) {
                slider_row(ui, "Swing", &mut s.wave_amp_mm, 0.5..=8.0, " mm", 1).on_hover_text("How far the rows swing up and down");
                slider_row(ui, "Length", &mut s.wave_len_mm, 3.0..=40.0, " mm", 1).on_hover_text("Distance from one wave to the next");
            }
            if s.fill != FillKind::Satin {
                slider_row(ui, "Stitch", &mut s.stitch_mm, 2.0..=6.0, " mm", 1).on_hover_text("Stitch length");
            }
            let note = match s.fill {
                FillKind::Satin => "Satin looks best on strokes under 8 mm wide.",
                FillKind::Wave | FillKind::Zigzag => "The pattern shows in how the thread catches the light.",
                FillKind::Crosshatch | FillKind::Open => "Open fills let the fabric show through. Nice on felt and canvas.",
                FillKind::Gradient => "Rows thin out across each letter. Turn the angle to set the direction.",
                _ => "",
            };
            if !note.is_empty() {
                ui.label(RichText::new(note).size(12.0).color(muted));
            }
            ui.checkbox(&mut s.underlay, "Underlay around each letter");
            if with_thread {
                jef_picker(ui, "fill", &mut s.fill_thread);
            }
        }
    });
}

/// Edge style for filled shapes.
fn edge_card(ui: &mut egui::Ui, s: &mut Settings, accent: Color32, title: &str, with_thread: bool) {
    card(ui, title, accent, |ui| {
        segmented(ui, &mut s.edge, &[(EdgeKind::Satin, "Satin"), (EdgeKind::Running, "Run"), (EdgeKind::Triple, "Triple"), (EdgeKind::None, "None")], accent);
        if s.edge == EdgeKind::Satin {
            ui.add_space(4.0);
            slider_row(ui, "Width", &mut s.edge_width_mm, 0.8..=5.0, " mm", 1);
        }
        if with_thread && s.edge != EdgeKind::None {
            jef_picker(ui, "edge", &mut s.edge_thread);
        }
    });
}
