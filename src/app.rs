use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, Key, KeyboardShortcut, Modifiers, RichText, Sense, Vec2, vec2};

use crate::canvas::{self, DesignStyle, Frame, Transform, View};
use crate::export::{self, Out};
use crate::formats::{self, Block, Cmd, Pattern};
use crate::hoops::{self, Fit, HOOPS, Placement};
use crate::artwork::{self, ArtSettings, Look};

mod checks;
mod library;
mod shapes;
mod shell;
mod team;
mod testpiece;
mod threadwin;

use shell::{Dock, Tab};
use crate::lettering::{self, Align, EdgeKind, FillKind, FontSource, PatchShape, Settings};
use crate::prefs::Prefs;
use crate::trace::{self, TraceSettings};
use crate::usb;
use crate::theme::Theme;

const FABRICS: [(&str, [u8; 3]); 8] = [
    (n!("Natural linen"), [0xd6, 0xc9, 0xb0]), (n!("White cotton"), [0xf6, 0xf5, 0xf0]),
    (n!("Heather gray"), [0x9c, 0x9b, 0x9d]), (n!("Denim"), [0x3e, 0x57, 0x78]), (n!("Navy"), [0x1e, 0x2a, 0x44]),
    (n!("Black"), [0x1c, 0x1b, 0x1d]), (n!("Brick red"), [0x9e, 0x2a, 0x2b]), (n!("Sage"), [0x9a, 0xab, 0x8d]),
];
const SPEEDS: [u32; 4] = [400, 600, 800, 860];
const PLAYBACK: [(f32, &str); 4] = [(1.0, n!("Real time")), (10.0, "10×"), (30.0, "30×"), (120.0, "120×")];

const SHORTCUTS: &[(&str, &str)] = &[
    (n!("O"), n!("Open a file")),
    ("V / L / A", n!("Viewer / lettering / artwork")),
    (n!("Ctrl+S"), n!("Save as JEF")),
    (n!("Ctrl+U"), n!("Send to the USB stick for the machine")),
    (n!("Ctrl+T"), n!("Test stitch-out: sew a small part, or the calibration sheet")),
    (n!("Ctrl+Shift+S"), n!("Save a picture of the preview")),
    (n!("Ctrl+P"), n!("Save a worksheet PDF with an actual-size template")),
    (n!("Ctrl+V"), n!("Paste SVG code as artwork")),
    (n!("Space"), n!("Play or pause the stitch-out")),
    (n!("Home / End"), n!("Back to the first stitch / show everything")),
    ("[ / ]", n!("Previous / next thread")),
    (", / .", n!("Step one stitch back / forward")),
    (n!("Arrows"), n!("Move the design 1 mm (Shift: 10 mm)")),
    (n!("R / Shift+R"), n!("Rotate 90° right / left")),
    (n!("M"), n!("Mirror")),
    (n!("P"), n!("Pin the design to the hoop to combine it with the next")),
    (n!("C"), n!("Center in the hoop")),
    ("1 – 4", n!("Pick a hoop")),
    ("F / D", n!("Show the whole hoop / zoom to the design")),
    ("G / J / T", n!("Grid / jump stitches / thread texture")),
    (n!("H"), n!("Thread density view (from the Check list)")),
    (n!("Click shape"), n!("Artwork: select a shape to move, resize or recolor it")),
    (n!("Arrows · [ / ]"), n!("Selected shape: move it / send it down or up")),
    (n!("Ctrl+D · Del · Esc"), n!("Selected shape: duplicate / delete / deselect")),
    (n!("Ctrl+Z / Ctrl+Shift+Z"), n!("Undo / redo any change")),
    ("?", n!("Show these shortcuts")),
    (n!("Q"), n!("Quit")),
];

#[derive(Default)]
struct UsbState {
    open: bool,
    drives: Vec<usb::Drive>,
    chosen: usize,
    name: String,
    /// A send or eject running in the background, and what it reports.
    job: Option<Receiver<Result<String, String>>>,
    /// The drive last written to, offered for ejecting.
    sent_to: Option<usb::Drive>,
    message: Option<Result<String, String>>,
    /// What to send instead of the design, such as a test piece.
    bytes: Option<Vec<u8>>,
}

/// A finished part fixed on the hoop, in hoop coordinates (0.1 mm, hoop
/// center at the origin), with rotation and position already applied.
struct Pinned {
    name: String,
    pattern: Pattern,
    blocks: Vec<Block>,
    bounds: (f32, f32, f32, f32),
}

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

/// Saving a picture of the canvas takes a frame drawn without overlays,
/// then a screenshot of it.
#[derive(Clone, Copy, PartialEq)]
enum Capture {
    Idle,
    Requested,
    Waiting(egui::Rect, std::time::Instant),
}

#[derive(Clone, Copy, PartialEq)]
enum FitFix {
    Rotate,
    Hoop(usize),
    Scale,
    Center,
}

/// Everything undo covers: hoop and placement plus the lettering and
/// artwork settings.
#[derive(Clone, PartialEq)]
struct Snapshot {
    setup: Setup,
    letter: Settings,
    art: Option<ArtSettings>,
}

#[derive(Clone, Copy, PartialEq)]
struct Setup {
    hoop: usize,
    at: Placement,
}

enum Drag {
    Move { start: (f32, f32), from: Placement },
    Pan,
    /// Moving the picked artwork shapes: where the drag started and each
    /// shape's offset then (SVG user units), the box of the moving shapes,
    /// and the x and y lines they snap to.
    Shape { start: (f32, f32), from: Vec<(usize, (f32, f32))>, moving: (f32, f32, f32, f32), snap: (Vec<f32>, Vec<f32>) },
    /// Picking shapes with a rubber band, from this screen point; `add`
    /// keeps the shapes already picked.
    Band { start: egui::Pos2, add: bool },
    /// Moving one point of the selected shape: line and point index.
    Node { line: usize, point: usize },
    /// Resizing the selected shape by a corner: size and pointer distance
    /// from the shape's middle when the drag started.
    Scale { from: f32, start: f32 },
    /// Turning the selected shape: its turn and the pointer's angle at the start.
    Turn { from: f32, start: f32 },
}

pub struct HoopApp {
    theme: Theme,
    design: Option<Loaded>,
    error: Option<String>,
    setup: Setup,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The state the last undo step was taken from.
    committed: Snapshot,
    /// When the current, not yet committed change started.
    pending_since: Option<std::time::Instant>,
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
    sizes: bool,
    help: bool,
    picker: Option<Receiver<Option<PathBuf>>>,
    weave: Option<egui::TextureHandle>,
    canvas_rect: egui::Rect,
    /// Lets background threads wake the window when their work is done.
    ctx: egui::Context,
    user_view: bool,
    paste_open: bool,
    paste_text: String,
    mode: Mode,
    /// The other mode's design, kept while this one is shown.
    stash: [Option<Loaded>; 3],
    artwork: Option<artwork::Artwork>,
    art: Option<ArtSettings>,
    art_built: Option<ArtSettings>,
    art_slow: bool,
    /// The artwork shape picked in the Shapes list or on the hoop.
    sel: Vec<usize>,
    /// Snap guides shown while shapes are dragged: vertical or not, and
    /// where, in user units.
    guides: Vec<(bool, f32)>,
    /// A row of the Objects list being dragged to restack it.
    list_drag: Option<usize>,
    /// A built-in shape asked for from a panel, added once it is drawn.
    list_add: Option<&'static str>,
    /// Pointer position on the hoop, in 0.1 mm, for the status bar.
    hover_at: Option<(f32, f32)>,
    /// The zoom that shows the whole hoop; the status bar's 100 %.
    hoop_scale: f32,
    /// The panel tab shown in each mode, and the left dock's tab.
    tab: [Tab; 3],
    dock: Dock,
    show_left: bool,
    show_right: bool,
    /// The design check and the stitches it was made for.
    check: Option<crate::check::Report>,
    check_key: u64,
    density: bool,
    /// A spot pointed out from the Check list, in design coordinates.
    flash: Option<((f32, f32), std::time::Instant)>,
    /// Saving or sending waits here while the user decides about problems.
    preflight: Option<checks::Output>,
    preflight_passed: bool,
    edit_points: bool,
    /// Set by undo and redo so the next stitch-out doesn't shift the design
    /// to keep shapes in place (the restored placement already does).
    keep_anchor: bool,
    letter: Settings,
    letter_built: Option<Settings>,
    fonts: Vec<FontSource>,
    font_scan: Option<Receiver<Vec<FontSource>>>,
    font_filter: String,
    /// A save dialog in progress: its answer, the bytes, and the extension.
    saver: Option<(Receiver<Option<PathBuf>>, Vec<u8>, &'static str)>,
    capture: Capture,
    pinned: Vec<Pinned>,
    usb: UsbState,
    test: testpiece::TestSew,
    /// What the calibration sheet showed works; tunes defaults and the check.
    tuning: crate::testsew::Tuning,
    /// The picture behind traced artwork, kept so it can be traced again.
    picture: Option<trace::Picture>,
    trace_set: TraceSettings,
    pending_retrace: bool,
    status: Option<(String, std::time::Instant)>,
    prefs: Prefs,
    /// Font name from the last session, waiting for the font list to load.
    want_font: Option<String>,
    /// A recent file picked in the panel, opened once the panel is drawn.
    open_later: Option<PathBuf>,
    library: library::Library,
    threads: threadwin::ThreadCharts,
    team: team::Team,
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
            committed: Snapshot { setup: Setup { hoop: 0, at: Placement::default() }, letter: Settings::default(), art: None },
            pending_since: None,
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
            sizes: true,
            help: false,
            picker: None,
            weave: None,
            canvas_rect: egui::Rect::NOTHING,
            ctx: cc.egui_ctx.clone(),
            user_view: false,
            paste_open: false,
            paste_text: String::new(),
            mode: Mode::Viewer,
            stash: [None, None, None],
            artwork: None,
            art: None,
            art_built: None,
            sel: Vec::new(),
            guides: Vec::new(),
            list_drag: None,
            list_add: None,
            hover_at: None,
            hoop_scale: 1.0,
            tab: [Tab::Design, Tab::Text, Tab::Source],
            dock: Dock::Colors,
            show_left: true,
            show_right: true,
            check: None,
            check_key: 0,
            density: false,
            flash: None,
            preflight: None,
            preflight_passed: false,
            edit_points: false,
            keep_anchor: false,
            art_slow: false,
            letter: Settings::default(),
            letter_built: None,
            fonts: lettering::bundled_fonts(),
            font_scan: None,
            font_filter: String::new(),
            saver: None,
            capture: Capture::Idle,
            pinned: Vec::new(),
            usb: UsbState::default(),
            test: testpiece::TestSew::default(),
            tuning: Default::default(),
            picture: None,
            trace_set: TraceSettings::default(),
            pending_retrace: false,
            status: None,
            prefs: Prefs::load(),
            want_font: None,
            open_later: None,
            library: Default::default(),
            threads: Default::default(),
            team: Default::default(),
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
        app.restore();
        app.restore_chart();
        app.reset_history();
        load_ui_font(&cc.egui_ctx);
        app.apply_theme(&cc.egui_ctx);
        if let Some(path) = file {
            app.open(&cc.egui_ctx, &path);
        }
        app.debug_start();
        app
    }

    /// For screenshots in tests: HOOP_MODE=viewer|lettering|artwork and
    /// HOOP_TAB=<tab label> pick what the window opens on.
    fn debug_start(&mut self) {
        if let Ok(m) = std::env::var("HOOP_MODE") {
            match m.as_str() {
                "lettering" => self.set_mode(Mode::Lettering),
                "artwork" => self.set_mode(Mode::Artwork),
                _ => self.set_mode(Mode::Viewer),
            }
        }
        match std::env::var("HOOP_DOCK").as_deref() {
            Ok("library") => self.dock = Dock::Library,
            Ok("colors") => self.dock = Dock::Colors,
            Ok("check") => self.dock = Dock::Check,
            _ => {}
        }
        match std::env::var("HOOP_WINDOW").as_deref() {
            Ok("threads") => self.threads.open = true,
            Ok("test") => self.test.open = true,
            Ok("help") => self.help = true,
            _ => {}
        }
        if let Ok(t) = std::env::var("HOOP_TAB") {
            if let Some(tab) = shell::tab_named(self.mode, &t) {
                self.tab[self.mode as usize] = tab;
            }
        }
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
        // Squarer corners, closer to a desktop toolkit.
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = egui::CornerRadius::same(3);
        }
        v.window_corner_radius = egui::CornerRadius::same(6);
        v.menu_corner_radius = egui::CornerRadius::same(4);
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
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif") {
            self.set_mode(Mode::Artwork);
            match trace::load(path) {
                Ok(pic) => {
                    self.picture = Some(pic);
                    self.trace_set = TraceSettings::default();
                    self.retrace(true);
                    self.prefs.add_recent(path);
                }
                Err(e) => self.error = Some(e),
            }
            return;
        }
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz")) {
            self.picture = None;
            if self.use_artwork(artwork::load(path)) {
                self.prefs.add_recent(path);
            }
            return;
        }
        self.set_mode(Mode::Viewer);
        let result = std::fs::read(path)
            .map_err(|e| t!("Couldn't open {name}: {e}.", name = name, e = e))
            .and_then(|bytes| formats::read(&name, &bytes).map_err(|e| format!("{name}: {e}")));
        match result {
            Ok(pattern) => {
                let blocks = pattern.blocks();
                let bounds = pattern.bounds();
                let loaded = Loaded { name: name.clone(), pattern, blocks, bounds };
                if self.pinned.is_empty() {
                    self.setup = Setup { hoop: hoops::smallest_fitting(loaded.size()), at: Placement::default() };
                } else {
                    // Keep the spot below the pinned parts; only reset turning.
                    self.setup.at.quarter_turns = 0;
                    self.setup.at.mirror = false;
                }
                self.reset_history();
                self.shown = loaded.pattern.points.len();
                self.playing = false;
                self.focus = None;
                self.design = Some(loaded);
                self.error = None;
                self.fit_pending = true;
                self.prefs.add_recent(path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(t!("{name} — Hoop", name = name)));
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
                .set_title(t!("Open an embroidery design"))
                .add_filter("Designs, artwork and pictures", &["jef", "dst", "exp", "pes", "pec", "svg", "svgz", "png", "jpg", "jpeg", "webp", "bmp", "gif", t!("JEF"), t!("DST"), t!("EXP"), t!("PES"), t!("PEC"), t!("SVG"), t!("PNG"), t!("JPG"), t!("JPEG")])
                .pick_file();
            let _ = tx.send(file);
            ctx.request_repaint();
        });
        self.picker = Some(rx);
    }

    fn change(&mut self, f: impl FnOnce(&mut Setup)) {
        f(&mut self.setup);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { setup: self.setup, letter: self.letter.clone(), art: self.art.clone() }
    }

    fn restore_snapshot(&mut self, s: Snapshot) {
        self.setup = s.setup;
        self.letter = s.letter.clone();
        if s.art.as_ref().map(|a| a.layers.len()) == self.art.as_ref().map(|a| a.layers.len()) {
            self.keep_anchor = s.art != self.art;
            self.art = s.art.clone();
            let n = self.art.as_ref().map_or(0, |a| a.shapes.len());
            self.sel.retain(|&i| i < n);
        }
        self.committed = self.snapshot();
        self.pending_since = None;
    }

    /// Starts a fresh history, for a newly opened design.
    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.committed = self.snapshot();
        self.pending_since = None;
    }

    /// Turns settled changes into undo steps. A change counts once the
    /// mouse is up and nothing changed for half a second, so dragging a
    /// slider or typing a word is one step.
    fn track_history(&mut self, ctx: &egui::Context, force: bool) {
        let now = self.snapshot();
        if now == self.committed {
            self.pending_since = None;
            return;
        }
        let since = *self.pending_since.get_or_insert_with(std::time::Instant::now);
        let settled = since.elapsed().as_millis() > 500 && !ctx.input(|i| i.pointer.any_down());
        if force || settled {
            self.undo.push(std::mem::replace(&mut self.committed, now));
            self.redo.clear();
            self.pending_since = None;
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(550));
        }
    }

    fn undo_step(&mut self, ctx: &egui::Context) {
        self.track_history(ctx, true);
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore_snapshot(s);
            self.toast(t!("Undone").into());
        }
    }

    fn redo_step(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore_snapshot(s);
            self.toast(t!("Redone").into());
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
        if !design {
            self.hoop_scale = scale;
        }
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
        if self.test.picking && press(Modifiers::NONE, Key::Escape) {
            self.test.picking = false;
        }
        if !self.help {
            self.shape_keys(ctx);
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
        if press(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            self.save_picture();
        }
        if press(Modifiers::COMMAND, Key::P) && self.design.is_some() {
            self.save_worksheet();
        }
        if press(Modifiers::COMMAND, Key::U) {
            self.open_usb();
        }
        if press(Modifiers::COMMAND, Key::T) {
            self.test.open = !self.test.open;
        }
        if press(Modifiers::COMMAND, Key::S) {
            self.save_jef();
        }
        if press(Modifiers::NONE, Key::P) {
            self.pin();
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
        if press(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || press(Modifiers::COMMAND, Key::Y) {
            self.redo_step();
        }
        if press(Modifiers::COMMAND, Key::Z) {
            self.undo_step(ctx);
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
        if press(Modifiers::NONE, Key::H) {
            self.density = !self.density;
        }
        if press(Modifiers::NONE, Key::J) {
            self.jumps = !self.jumps;
        }
        if press(Modifiers::NONE, Key::T) {
            self.textured = !self.textured;
        }
    }

    /// The right dock's page for the chosen tab.
    fn side_panel(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect, tab: Tab) {
        let muted = self.muted();
        let accent = self.accent();
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(BAD));
        }

        if self.mode == Mode::Lettering {
            self.lettering_panel(ui, tab);
        } else if self.mode == Mode::Artwork {
            self.artwork_panel(ui, canvas_rect, tab);
        } else if tab == Tab::Design {
            card(ui, t!("Design"), accent, |ui| {
                match &self.design {
                    Some(d) => {
                        ui.label(RichText::new(&d.name).strong().size(15.0));
                        let (w, h) = d.size();
                        ui.label(RichText::new(t!("{format} · {v1:.1} × {v2:.1} mm", format = d.pattern.format, v1 = w / 10.0, v2 = h / 10.0)).color(muted));
                    }
                    None => {
                        ui.label(RichText::new(t!("Open a JEF, DST, EXP, PES or PEC file, or drop one on the hoop.")).color(muted));
                    }
                }
                let recent: Vec<PathBuf> = self.prefs.recent().into_iter().filter(|p| p.exists()).take(5).collect();
                if !recent.is_empty() {
                    egui::CollapsingHeader::new(RichText::new(t!("Recent")).color(muted)).default_open(self.design.is_none()).show(ui, |ui| {
                        for p in recent {
                            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            if ui.add(egui::Button::new(&name).frame(false)).on_hover_text(p.display().to_string()).clicked() {
                                self.open_later = Some(p);
                            }
                        }
                    });
                }
                ui.add_space(4.0);
                let w = (ui.available_width() - 8.0) / 2.0;
                ui.horizontal(|ui| {
                    if ui.add(primary(t!("Open…"), accent).min_size(vec2(w, 30.0))).on_hover_text("O").clicked() {
                        self.pick_file();
                    }
                    if ui.add_enabled(self.design.is_some(), egui::Button::new(t!("Save as JEF…")).min_size(vec2(w, 30.0))).on_hover_text("Ctrl+S").clicked() {
                        self.save_jef();
                    }
                });
            });
        }
        if tab == Tab::Hoop && let Some(d) = &self.design {
            let fit = match (self.pinned.is_empty(), self.hoop_bounds()) {
                (false, Some(b)) => {
                    let at = Placement { dx: (b.0 + b.2) / 2.0, dy: (b.1 + b.3) / 2.0, ..Default::default() };
                    hoops::fit(self.hoop_size(), (b.2 - b.0, b.3 - b.1), &at)
                }
                _ => hoops::fit(self.hoop_size(), d.size(), &self.setup.at),
            };
            let (ok, title, detail) = match fit {
                Fit::Inside(m) => (true, t!("Fits the {v0}", v0 = HOOPS[self.setup.hoop].name), t!("Closest edge is {m:.1} mm away.", m = m)),
                Fit::TooBig { over_w, over_h, fits_turned } => {
                    let mut parts = Vec::new();
                    if over_w > 0.0 { parts.push(t!("{over_w:.1} mm too wide", over_w = over_w)); }
                    if over_h > 0.0 { parts.push(t!("{over_h:.1} mm too tall", over_h = over_h)); }
                    let hint = if fits_turned { t!(" Rotate it 90° (R) and it fits.") } else { t!(" Pick a bigger hoop.") };
                    (false, t!("Doesn't fit this hoop").into(), format!("{}.{hint}", parts.join(", ")))
                }
                Fit::OffEdge { side, by } => (false, t!("Off the sewing area").into(), t!("Sticks out {by:.1} mm on the {side}. Press C to center.", by = by, side = side)),
            };
            let color = if ok { OK } else { BAD };
            // One-click ways out of a bad fit.
            let mut actions: Vec<(String, FitFix)> = Vec::new();
            let size = d.size();
            match fit {
                Fit::TooBig { fits_turned, .. } => {
                    if fits_turned {
                        actions.push((t!("Rotate 90°").into(), FitFix::Rotate));
                    }
                    let unturned = Placement::default();
                    if let Some(i) = (0..3).find(|&i| matches!(hoops::fit((HOOPS[i].w, HOOPS[i].h), size, &unturned), Fit::Inside(_))) {
                        actions.push((t!("Use {hoop}", hoop = HOOPS[i].name), FitFix::Hoop(i)));
                    }
                    if self.mode != Mode::Viewer {
                        actions.push((t!("Shrink to fit").into(), FitFix::Scale));
                    }
                }
                Fit::OffEdge { .. } => actions.push(("Center".into(), FitFix::Center)),
                Fit::Inside(m) if self.mode != Mode::Viewer && m > 15.0 => actions.push((t!("Grow to fill hoop").into(), FitFix::Scale)),
                Fit::Inside(_) => {}
            }
            let mut chosen = None;
            egui::Frame::new().fill(color.gamma_multiply(0.12)).stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5))).corner_radius(10).inner_margin(12).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, color);
                    ui.label(RichText::new(title).color(color).strong());
                });
                ui.label(RichText::new(detail).color(muted));
                if !actions.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (label, fix) in &actions {
                            if ui.button(label).clicked() {
                                chosen = Some(*fix);
                            }
                        }
                    });
                }
            });
            ui.add_space(10.0);
            if let Some(fix) = chosen {
                self.apply_fix(fix, canvas_rect);
            }
        }

        if tab == Tab::Hoop && (self.design.is_some() || !self.pinned.is_empty()) {
            let mut remove = None;
            let mut clear = false;
            let mut pin = false;
            card(ui, t!("Combine"), accent, |ui| {
                if self.pinned.is_empty() {
                    ui.label(RichText::new(t!("Pin this design to the hoop, then make or open the next part, like a name under a logo. Save as JEF saves all of them as one file.")).size(12.0).color(muted));
                }
                for (i, p) in self.pinned.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).monospace().color(muted));
                        ui.label(&p.name);
                        ui.label(RichText::new(t!("{v0:.0} × {v1:.0} mm", v0 = (p.bounds.2 - p.bounds.0) / 10.0, v1 = (p.bounds.3 - p.bounds.1) / 10.0)).size(12.0).color(muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(t!("Remove")).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.design.is_some(), egui::Button::new(t!("Pin to hoop"))).on_hover_text(t!("P · keep this design where it is and start the next part")).clicked() {
                        pin = true;
                    }
                    if !self.pinned.is_empty() && ui.button(t!("Clear pinned")).clicked() {
                        clear = true;
                    }
                });
            });
            if let Some(i) = remove {
                self.pinned.remove(i);
            }
            if clear {
                self.pinned.clear();
            }
            if pin {
                self.pin();
            }
        }

        if tab == Tab::Hoop {
            card(ui, t!("Hoop"), accent, |ui| {
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

            card(ui, t!("Position"), accent, |ui| {
                let w = (ui.available_width() - 18.0) / 4.0;
                ui.horizontal(|ui| {
                    let b = |t: &str| egui::Button::new(t).min_size(vec2(w, 28.0));
                    if ui.add(b(t!("Left 90°"))).on_hover_text(t!("Rotate left · Shift+R")).clicked() {
                        self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
                    }
                    if ui.add(b(t!("Right 90°"))).on_hover_text(t!("Rotate right · R")).clicked() {
                        self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
                    }
                    if ui.add(b(t!("Mirror"))).on_hover_text("M").clicked() {
                        self.change(|s| s.at.mirror = !s.at.mirror);
                    }
                    if ui.add(b(t!("Center"))).on_hover_text("C").clicked() {
                        self.change(|s| { s.at.dx = 0.0; s.at.dy = 0.0; });
                    }
                });
                let at = self.setup.at;
                let signed = |v: f32| format!("{}{:.1}", if v > 0.0 { "+" } else if v < 0.0 { "−" } else { "" }, (v / 10.0).abs());
                ui.label(RichText::new(t!("{v0}°{v1}  ·  X {v2}  Y {v3} mm", v0 = at.quarter_turns as u32 * 90, v1 = if at.mirror { " mirrored" } else { "" }, v2 = signed(at.dx), v3 = signed(-at.dy))).monospace().size(12.0).color(muted));
            });
        }

        if tab == Tab::Stitch && self.design.is_some() {
            let total = self.total();
            card(ui, t!("Stitch-out"), accent, |ui| {
                ui.horizontal(|ui| {
                    let label = if self.playing { t!("Pause") } else if self.shown >= total { t!("Replay") } else { t!("Play") };
                    if ui.add(primary(label, accent).min_size(vec2(72.0, 28.0))).on_hover_text(t!("Space")).clicked() {
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
                    let text = if self.shown >= total { t!("All {total} stitches", total = total) } else { t!("{shown} / {total} · thread {thread}", shown = self.shown, total = total, thread = thread) };
                    ui.label(RichText::new(text).monospace().size(12.0).color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("rate").selected_text(crate::i18n::tr(PLAYBACK[self.play_rate].1)).width(84.0).show_ui(ui, |ui| {
                            for (i, (_, name)) in PLAYBACK.iter().enumerate() {
                                ui.selectable_value(&mut self.play_rate, i, crate::i18n::tr(name));
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
            let mins = if minutes < 1.0 { "<1".to_string() } else { format!("{}", minutes.round()) };
            let time = if changes > 0 {
                tn!(changes, "≈ {mins} min + {n} change", "≈ {mins} min + {n} changes", mins = mins)
            } else {
                t!("≈ {mins} min", mins = mins)
            };
            let trims = p.count(Cmd::Trim);
            let rows = [
                ("Size", t!("{v0:.1} × {v1:.1} mm", v0 = w / 10.0, v1 = h / 10.0)),
                (t!("Stitches"), stitches.to_string()),
                (t!("Colors"), d.blocks.len().to_string()),
                ("Jumps", if trims > 0 { t!("{v0} · {trims} trims", v0 = p.count(Cmd::Jump), trims = trims) } else { p.count(Cmd::Jump).to_string() }),
                (t!("Sewing time"), time),
            ];
            let warnings = p.warnings();
            card(ui, t!("Details"), accent, |ui| {
                for w in &warnings {
                    egui::Frame::new().fill(WARN.gamma_multiply(0.12)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(w).color(WARN).size(12.5));
                    });
                }
                for (k, v) in rows {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).color(muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(v).monospace()));
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("Machine speed")).color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("spm").selected_text(t!("{spm} spm", spm = SPEEDS[self.speed])).width(96.0).show_ui(ui, |ui| {
                            for (i, s) in SPEEDS.iter().enumerate() {
                                ui.selectable_value(&mut self.speed, i, t!("{spm} spm", spm = s));
                            }
                        });
                    });
                });
                if ui.button(t!("Save worksheet…")).on_hover_text(t!("A PDF with the thread sequence and an actual-size template · Ctrl+P")).clicked() {
                    self.save_worksheet();
                }
            });
        }

        if tab == Tab::View {
        card(ui, t!("Display"), accent, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (name, c) in FABRICS {
                    let selected = self.fabric == c;
                    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                    if selected {
                        ui.painter().circle_stroke(rect.center(), 12.5, egui::Stroke::new(2.0, accent));
                    }
                    ui.painter().circle(rect.center(), 10.0, rgb(c), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
                    if resp.on_hover_text(crate::i18n::tr(name)).clicked() {
                        self.fabric = c;
                    }
                }
                ui.color_edit_button_srgb(&mut self.fabric).on_hover_text(t!("Other fabric color"));
            });
            ui.checkbox(&mut self.textured, t!("Thread texture  T"));
            ui.checkbox(&mut self.grid, t!("Template grid, 10 mm  G"));
            ui.checkbox(&mut self.jumps, t!("Jump stitches  J"));
            ui.checkbox(&mut self.sizes, t!("Size label under the design"));
            if ui.add_enabled(self.design.is_some(), egui::Button::new(t!("Save picture…"))).on_hover_text(t!("A PNG of the hoop preview, to share · Ctrl+Shift+S")).clicked() {
                self.save_picture();
            }
        });
        }
        ui.label(RichText::new(t!("Press ? for every shortcut.")).size(12.0).color(muted));
        ui.add_space(14.0);
    }

    /// The Colors list in the left dock: each thread of the design in sewing
    /// order. Clicking one shows it on its own.
    fn colors_list(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        let viewer = self.mode == Mode::Viewer;
        let ui = &mut *ui;
        let Some(_) = &self.design else {
            ui.label(RichText::new(t!("Thread colors of the design show here, in sewing order.")).size(12.0).color(muted));
            return;
        };
        {
            let focus = self.focus;
            let mut toggle = None;
            let matched: Vec<Option<formats::Thread>> = self.design.as_ref().unwrap().blocks.iter().map(|b| self.chart_thread(&b.thread)).collect();
            let chart_name = self.chart().name.clone();
            let d = self.design.as_mut().unwrap();
            let meters: Vec<f32> = d.blocks.iter().map(|b| d.pattern.thread_meters(b)).collect();
            let mut copied = false;
            {
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
                            // Two lines so long thread names fit a narrow dock.
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.add(egui::Label::new(format!("{}. {}", k + 1, b.thread.name)).truncate());
                                let code = if b.thread.code.is_empty() { String::new() } else { format!("{} · ", b.thread.code) };
                                ui.label(RichText::new(t!("{code}{v0:.1} m · {stitches} st", code = code, v0 = meters[k], stitches = b.stitches)).size(11.5).color(muted))
                                    .on_hover_text(t!("{stitches} stitches, about {v1:.1} m of thread", stitches = b.stitches, v1 = meters[k]));
                                if let Some(t) = &matched[k] {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                        ui.painter().circle_filled(r.center(), 5.0, rgb(t.color));
                                        ui.add(egui::Label::new(RichText::new(format!("{} {}", t.code, t.name)).size(11.5)).truncate())
                                            .on_hover_text(format!("{chart_name}: {} {}", t.code, t.name));
                                    });
                                }
                            });
                        });
                    });
                    if ui.interact(r.response.rect, ui.id().with(("thread", k)), Sense::click()).on_hover_text(t!("Click to show this thread on its own")).clicked() {
                        toggle = Some(k);
                    }
                }
                if !d.pattern.format.stores_colors() {
                    ui.label(RichText::new(t!("{format} files don't store thread colors. Click a swatch to set the real ones.", format = d.pattern.format)).size(12.0).color(muted));
                }
                if ui.small_button(t!("Copy thread list")).on_hover_text(t!("For shopping or notes")).clicked() {
                    let list: Vec<String> = d
                        .blocks
                        .iter()
                        .enumerate()
                        .map(|(k, b)| {
                            let alt = matched[k].as_ref().map_or(String::new(), |t| format!(" ({chart_name} {} {})", t.code, t.name));
                            t!("{v0}. {name} {code}{alt} — {stitches} stitches, about {v4:.1} m", v0 = k + 1, name = b.thread.name, code = b.thread.code, alt = alt, stitches = b.stitches, v4 = meters[k])
                        })
                        .collect();
                    ui.ctx().copy_text(format!("{}\n{}", d.name, list.join("\n")));
                    copied = true;
                }
            }
            if copied {
                self.toast(t!("Thread list copied").into());
            }
            if ui.small_button(t!("Thread chart: {chart_name}…", chart_name = chart_name)).on_hover_text(t!("Pick a brand and mark the threads you have")).clicked() {
                self.threads.open = true;
            }
            if let Some(k) = toggle {
                self.focus = if self.focus == Some(k) { None } else { Some(k) };
            }

        }
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

        // Drawing the test area takes over the pointer until it is done.
        let picking = self.test_area_input(ui, &response, &frame);
        // Artwork shapes: pick, drag, resize, turn, edit points, rubber band.
        let handled_double = !picking && self.shape_input(ui, &response, &frame);
        let hover_shape = if picking { None } else { self.shape_hover(ui, &response, &frame) };

        // Interaction: drag the design to move it, drag elsewhere to pan, scroll to zoom.
        if response.drag_started() && self.drag.is_none() && !picking {
            let over = response.interact_pointer_pos().map(|p| frame.to_world(p));
            let middle = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
            self.drag = match (over, &self.design) {
                _ if middle => Some(Drag::Pan),
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
                _ => {}
            }
        }
        if response.drag_stopped() {
            self.drag = None;
        }
        if response.double_clicked() && !handled_double {
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
        canvas::draw_hoop(&frame, hoop.w * 10.0, hoop.h * 10.0, &t!("{name}  {w} × {h} mm", name = hoop.name, w = hoop.w, h = hoop.h), self.fabric, self.grid);

        let pinned_style = DesignStyle { textured: self.textured, show_jumps: false, focus: None, fabric: self.fabric };
        let identity = Transform::new((0.0, 0.0, 0.0, 0.0), &Placement::default());
        for p in &self.pinned {
            canvas::draw_design(&frame, &p.pattern, &p.blocks, &identity, p.pattern.points.len(), &pinned_style);
        }
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
            if self.sizes && !self.playing {
                // Finished size just under the design, like a dimension on a drawing.
                let at = self.setup.at;
                let below = frame.to_screen(at.dx, at.dy + size.1 / 2.0) + vec2(0.0, 14.0);
                let text = t!("{v0:.1} × {v1:.1} mm", v0 = size.0 / 10.0, v1 = size.1 / 10.0);
                let ink = if canvas::luminance(self.fabric) < 0.45 { Color32::from_white_alpha(200) } else { Color32::from_black_alpha(170) };
                painter.text(below, egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(12.0), ink);
            }
        } else {
            painter.text(
                rect.center_bottom() - vec2(0.0, 28.0),
                egui::Align2::CENTER_CENTER,
                t!("Press O to open a design, or drop a file here"),
                egui::FontId::proportional(15.0),
                Color32::from_white_alpha(230),
            );
        }

        if self.capture == Capture::Requested {
            // This frame has no overlays; grab it once it is on screen.
            self.capture = Capture::Waiting(rect, std::time::Instant::now());
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            return;
        }
        if let Some((text, at)) = &self.status {
            let age = at.elapsed().as_secs_f32();
            if age < 4.0 {
                let alpha = ((4.0 - age) / 0.5).min(1.0);
                let galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(14.0), Color32::WHITE.gamma_multiply(alpha));
                let pos = rect.center_top() + vec2(-galley.size().x / 2.0, 16.0);
                let bg = egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0));
                painter.rect_filled(bg, 8, Color32::from_black_alpha((190.0 * alpha) as u8));
                painter.galley(pos, galley, Color32::WHITE);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(if age > 3.5 { 30 } else { 500 }));
            }
        }

        self.check_overlay(&painter, &frame);
        self.shape_overlay(&painter, &frame, hover_shape);
        self.test_overlay(&painter, &frame);
        self.hover_at = response.hover_pos().map(|p| frame.to_world(p));

    }

    fn toast(&mut self, text: String) {
        self.status = Some((text, std::time::Instant::now()));
        self.ctx.request_repaint();
    }

    fn apply_fix(&mut self, fix: FitFix, canvas_rect: egui::Rect) {
        let Some(d) = &self.design else { return };
        let (hw, hh) = self.hoop_size();
        let (dw, dh) = self.setup.at.size(d.size().0, d.size().1);
        // Leave 5 mm on every side.
        let factor = ((hw * 10.0 - 100.0) / dw).min((hh * 10.0 - 100.0) / dh).max(0.05);
        match fix {
            FitFix::Rotate => self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4),
            FitFix::Hoop(i) => {
                self.change(|s| s.hoop = i);
                self.fit_view(canvas_rect, false);
            }
            FitFix::Center => self.change(|s| {
                s.at.dx = 0.0;
                s.at.dy = 0.0;
            }),
            FitFix::Scale => {
                self.change(|s| {
                    s.at.dx = 0.0;
                    s.at.dy = 0.0;
                });
                match self.mode {
                    Mode::Lettering => self.letter.height_mm = (self.letter.height_mm * factor * 10.0).floor() / 10.0,
                    Mode::Artwork => {
                        if let Some(a) = &mut self.art {
                            a.width_mm = (a.width_mm * factor).floor();
                        }
                    }
                    Mode::Viewer => {}
                }
            }
        }
    }

    /// Settings from the last session.
    fn restore(&mut self) {
        crate::i18n::set_language(self.prefs.get("language").unwrap_or("system"));
        let p = &self.prefs;
        if let Some(c) = p.get("fabric").and_then(|v| u32::from_str_radix(v, 16).ok()) {
            self.fabric = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        }
        for (key, flag) in [("grid", &mut self.grid), ("jumps", &mut self.jumps), ("texture", &mut self.textured), ("sizes", &mut self.sizes)] {
            if let Some(v) = p.num(key) {
                *flag = v;
            }
        }
        if let Some(v) = p.num::<usize>("speed").filter(|v| *v < SPEEDS.len()) {
            self.speed = v;
        }
        if let Some(v) = p.num::<usize>("playback").filter(|v| *v < PLAYBACK.len()) {
            self.play_rate = v;
        }
        self.letter = p.lettering();
        self.tuning = p.tuning();
        self.want_font = p.get("letter.font").map(str::to_string);
        self.resolve_font();
    }

    /// Picks the remembered font once it shows up in the font list.
    fn resolve_font(&mut self) {
        if let Some(name) = &self.want_font {
            if let Some(i) = self.fonts.iter().position(|f| &f.name == name) {
                self.letter.font = i;
                self.want_font = None;
            }
        }
    }

    fn store(&mut self) {
        let f = self.fabric;
        self.prefs.set("fabric", format!("{:02x}{:02x}{:02x}", f[0], f[1], f[2]));
        self.prefs.set("grid", self.grid);
        self.prefs.set("jumps", self.jumps);
        self.prefs.set("texture", self.textured);
        self.prefs.set("sizes", self.sizes);
        self.prefs.set("speed", self.speed);
        self.prefs.set("playback", self.play_rate);
        let font = self.fonts.get(self.letter.font).map_or("", |f| f.name.as_str()).to_string();
        self.prefs.store_lettering(&self.letter, &font);
        if let Err(e) = self.prefs.save() {
            eprintln!("hoop: couldn't save settings: {e}");
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
                if first && self.pinned.is_empty() {
                    let size = self.design.as_ref().unwrap().size();
                    self.setup.hoop = hoops::smallest_fitting(size);
                }
                self.shown = self.total();
                self.playing = false;
                self.error = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(t!("Lettering — Hoop").into()));
            }
            Err(e) => {
                self.design = None;
                self.error = Some(e);
            }
        }
    }

    fn lettering_panel(&mut self, ui: &mut egui::Ui, tab: Tab) {
        let muted = self.muted();
        let accent = self.accent();
        let mut tip_later = None;
        let s = &mut self.letter;
        if tab == Tab::Text {
        card(ui, t!("Text"), accent, |ui| {
            ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(2).desired_width(f32::INFINITY).font(egui::TextStyle::Heading));
            let current = self.fonts.get(s.font).map_or("", |f| f.name.as_str()).to_string();
            egui::ComboBox::from_id_salt("font").selected_text(current).width(ui.available_width()).height(380.0).show_ui(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.font_filter).hint_text(t!("Search fonts")));
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
                    ui.label(RichText::new(t!("Looking for installed fonts…")).color(muted));
                }
            });
        });
        card(ui, t!("Quick styles"), accent, |ui| {
            if let Some(i) = preset_chips(ui, &lettering::PRESETS.map(|p| (p.0, p.1))) {
                (lettering::PRESETS[i].2)(s);
            }
        });
        }
        if tab == Tab::Style {
        card(ui, t!("Shape"), accent, |ui| {
            slider_row(ui, t!("Height"), &mut s.height_mm, 5.0..=120.0, " mm", 1).on_hover_text(t!("Height of capital letters"));
            slider_row(ui, t!("Spacing"), &mut s.spacing_mm, -5.0..=15.0, " mm", 1).on_hover_text(t!("Extra space between letters"));
            ui.horizontal(|ui| {
                ui.label(t!("Layout"));
                egui::ComboBox::from_id_salt("text-shape").selected_text(crate::i18n::tr(s.shape.name())).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                    for sh in lettering::TextShape::ALL {
                        ui.selectable_value(&mut s.shape, sh, crate::i18n::tr(sh.name()));
                    }
                });
            });
            match s.shape {
                lettering::TextShape::Arc => {
                    slider_row(ui, t!("Arc"), &mut s.arc_deg, -360.0..=360.0, "°", 0)
                        .on_hover_text(t!("Bend the text into an arch (positive) or a smile (negative). 360° closes the circle."));
                }
                lettering::TextShape::Circle => {
                    slider_row(ui, t!("Circle"), &mut s.circle_mm, 20.0..=180.0, " mm", 0).on_hover_text(t!("Inside diameter of the ring of text"));
                    ui.label(RichText::new(t!("Line 1 runs round the top, line 2 round the bottom, more lines sit in the middle.")).size(12.0).color(muted));
                }
                lettering::TextShape::Monogram => {
                    ui.label(RichText::new(t!("Type three initials; the middle one is sewn larger.")).size(12.0).color(muted));
                }
                sh if sh.bends() => {
                    slider_row(ui, t!("Amount"), &mut s.envelope_mm, -40.0..=40.0, " mm", 1).on_hover_text(t!("How far the envelope bends the text"));
                }
                _ => {}
            }
            slider_row(ui, t!("Slant"), &mut s.slant_deg, -30.0..=30.0, "°", 0).on_hover_text(t!("Lean the letters like italics"));
            if s.text.lines().count() > 1 {
                slider_row(ui, t!("Lines"), &mut s.line_spacing, 0.8..=2.5, "×", 2).on_hover_text(t!("Distance between lines, in letter heights"));
                segmented(ui, &mut s.align, &[(Align::Left, t!("Left")), (Align::Center, t!("Center")), (Align::Right, t!("Right"))], accent);
            }
        });
        if let Some(tip) = fabric_card(ui, s, muted) {
            tip_later = Some(tip);
        }
        fill_card(ui, s, muted, accent, true);
        edge_card(ui, s, accent, t!("Edge"), true);
        }
        if tab == Tab::Patch {
        card(ui, t!("Patch"), accent, |ui| {
            ui.label(RichText::new(t!("A background shape for badges and name patches")).size(12.0).color(muted));
            segmented(
                ui,
                &mut s.patch,
                &[(PatchShape::None, t!("Off")), (PatchShape::Rect, t!("Box")), (PatchShape::Rounded, t!("Round")), (PatchShape::Oval, t!("Oval")), (PatchShape::Circle, t!("Circle"))],
                accent,
            );
            if s.patch != PatchShape::None {
                ui.add_space(4.0);
                slider_row(ui, t!("Padding"), &mut s.patch_pad_mm, 1.0..=25.0, " mm", 1).on_hover_text(t!("Space between the text and the patch edge"));
                slider_row(ui, t!("Border"), &mut s.patch_border_mm, 0.0..=6.0, " mm", 1).on_hover_text(t!("Satin border width; 0 for none"));
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(t!("Background")).color(muted));
                    for (k, app, name, about) in [
                        (FillKind::Tatami, false, "Filled", t!("Stitched solid")),
                        (FillKind::Open, false, "Light", t!("An open fill, the fabric shows through")),
                        (FillKind::None, false, "Fabric", t!("No background stitching")),
                        (FillKind::None, true, "Appliqué", t!("A piece of fabric sewn on: placement line, stop, tack-down, stop, then the cover stitch")),
                    ] {
                        let on = s.patch_fill == k && s.patch_applique == app;
                        if ui.selectable_label(on, name).on_hover_text(about).clicked() {
                            s.patch_fill = k;
                            s.patch_applique = app;
                        }
                    }
                });
                if s.patch_applique {
                    ui.label(RichText::new(t!("The machine stops twice: lay the fabric over the placement line, then trim it close to the tack-down.")).size(12.0).color(muted));
                    jef_picker_labeled(ui, "patch-bg", t!("Guide"), &mut s.patch_thread);
                } else if s.patch_fill != FillKind::None {
                    jef_picker_labeled(ui, "patch-bg", t!("Fill"), &mut s.patch_thread);
                }
                if s.patch_border_mm > 0.0 {
                    jef_picker_labeled(ui, "patch-border", t!("Border"), &mut s.patch_border_thread);
                }
            }
        });
        }
        if let Some(tip) = tip_later {
            self.toast(tip.to_string());
        }
        if tab == Tab::Text {
            self.team_card(ui);
        }
        if tab == Tab::Text && ui.button(t!("Reset lettering")).on_hover_text(t!("Back to the default style, keeping the font")).clicked() {
            let font = self.letter.font;
            self.letter = Settings { font, ..Default::default() };
        }
        ui.add_space(10.0);
    }

    /// Shows freshly read artwork in the Artwork tab, or the reason it failed.
    fn use_artwork(&mut self, result: Result<artwork::Artwork, String>) -> bool {
        self.set_mode(Mode::Artwork);
        match result {
            Ok(art) => {
                let mut s = ArtSettings::for_artwork(&art);
                self.tuning.apply(&mut s.style);
                self.art = Some(s);
                self.reset_history();
                self.art_built = None;
                self.sel.clear();
                self.edit_points = false;
                self.dock = Dock::Objects;
                self.artwork = Some(art);
                self.design = None;
                self.error = None;
                self.fit_pending = true;
                self.user_view = false;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Traces the picture again, keeping the stitch settings unless `fresh`.
    fn retrace(&mut self, fresh: bool) {
        let Some(pic) = &self.picture else { return };
        let old = self.art.clone();
        let result = trace::trace(pic, &self.trace_set);
        if self.use_artwork(result) {
            if let (Some(new), Some(old)) = (&mut self.art, old.filter(|_| !fresh)) {
                new.width_mm = old.width_mm;
                new.style = old.style;
                new.knockout = old.knockout;
                new.min_detail_mm = old.min_detail_mm;
                new.stroke = old.stroke;
            } else if let Some(new) = &mut self.art {
                new.width_mm = 90.0;
            }
        }
    }

    fn paste_svg(&mut self, text: &str) -> bool {
        self.picture = None;
        self.use_artwork(artwork::parse(t!("Pasted SVG"), text.trim().as_bytes()))
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
                // Keep the drawing's frame still on the hoop when one shape
                // moves or changes, instead of re-centering the whole design.
                if let Some(old) = self.design.as_ref().filter(|_| !std::mem::take(&mut self.keep_anchor)) {
                    let a = Transform::new(old.bounds, &self.setup.at).apply((0.0, 0.0));
                    let b = Transform::new(bounds, &self.setup.at).apply((0.0, 0.0));
                    self.setup.at.dx += a.0 - b.0;
                    self.setup.at.dy += a.1 - b.1;
                }
                self.keep_anchor = false;
                self.design = Some(Loaded { name: art.name.clone(), pattern, blocks, bounds });
                if first && self.pinned.is_empty() {
                    self.setup.hoop = hoops::smallest_fitting(self.design.as_ref().unwrap().size());
                    self.fit_pending = !self.user_view;
                }
                self.shown = self.total();
                self.playing = false;
                self.error = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(t!("{name} — Hoop", name = art.name)));
            }
            Err(e) => {
                self.design = None;
                self.error = Some(e);
            }
        }
    }

    fn artwork_panel(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect, tab: Tab) {
        let muted = self.muted();
        let accent = self.accent();
        let has_art = self.artwork.is_some();
        if tab == Tab::Shape && has_art {
            self.shape_tab(ui);
            return;
        }
        if tab == Tab::Source || !has_art {
        card(ui, t!("Artwork"), accent, |ui| {
            match &self.artwork {
                Some(a) => {
                    ui.label(RichText::new(&a.name).strong().size(15.0));
                    let shapes = a.shapes.len();
                    ui.label(RichText::new(format!("{} · {}", tn!(shapes, "{n} shape", "{n} shapes"), tn!(a.layers.len(), "{n} color", "{n} colors"))).color(muted));
                    if a.skipped_images > 0 {
                        ui.label(RichText::new(tn!(a.skipped_images, "Skipped {n} embedded picture. Only vector shapes can be stitched.", "Skipped {n} embedded pictures. Only vector shapes can be stitched.")).size(12.0).color(muted));
                    }
                }
                None => {
                    ui.label(RichText::new(t!("Open an SVG or a picture (PNG, JPG), or drop one on the hoop. Logos and clip art with flat colors work best.")).color(muted));
                    if ui.button(t!("Start from shapes")).on_hover_text(t!("An empty design to build from hearts, stars, circles and more")).clicked() {
                        self.picture = None;
                        self.use_artwork(Ok(artwork::blank()));
                    }
                }
            }
            ui.add_space(4.0);
            let w = (ui.available_width() - 12.0) / 3.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if ui.add(primary(t!("Open…"), accent).min_size(vec2(w, 30.0))).on_hover_text("O").clicked() {
                    self.pick_file();
                }
                if ui.add(egui::Button::new(t!("Paste SVG")).selected(self.paste_open).min_size(vec2(w, 30.0)))
                    .on_hover_text(t!("Paste SVG code, or press Ctrl+V anywhere with SVG code copied"))
                    .clicked()
                {
                    self.paste_open = !self.paste_open;
                }
                if ui.add_enabled(self.design.is_some(), egui::Button::new(t!("Save JEF…")).min_size(vec2(w, 30.0))).on_hover_text("Ctrl+S").clicked() {
                    self.save_jef();
                }
            });
            if self.paste_open {
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt("svg-text").max_height(220.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.paste_text)
                            .code_editor()
                            .desired_rows(8)
                            .desired_width(f32::INFINITY)
                            .hint_text("<svg xmlns=\"http://www.w3.org/2000/svg\" …>\n  …\n</svg>"),
                    );
                });
                ui.horizontal(|ui| {
                    let ready = !self.paste_text.trim().is_empty();
                    if ui.add_enabled(ready, primary(t!("Use this SVG"), accent)).clicked() {
                        let text = self.paste_text.clone();
                        if self.paste_svg(&text) {
                            self.paste_open = false;
                        }
                    }
                    if ui.button(t!("Clear")).clicked() {
                        self.paste_text.clear();
                    }
                });
            }
        });
        }
        if !has_art {
            return;
        }
        if tab == Tab::Source && self.picture.is_some() {
            let mut t = self.trace_set;
            card(ui, t!("Trace"), accent, |ui| {
                ui.label(RichText::new(t!("Pictures are reduced to a few flat colors and outlined. Logos and clip art work best; photos don't stitch well.")).size(12.0).color(muted));
                int_row(ui, t!("Colors"), &mut t.colors, 2..=10, t!("How many colors the picture is reduced to, background included"));
                int_row(ui, t!("Smooth"), &mut t.smooth, 0..=3, t!("Rounds off pixel corners in the outlines"));
                int_row(ui, t!("Clean"), &mut t.clean, 0..=3, t!("Removes specks and noise before tracing. Raise it for scanned or blurry pictures."));
                ui.checkbox(&mut t.drop_background, t!("Leave out the background color"));
            });
            if t != self.trace_set && !ui.ctx().input(|i| i.pointer.any_down()) {
                self.trace_set = t;
                self.retrace(false);
            } else if t != self.trace_set {
                self.trace_set = t;
                self.pending_retrace = true;
            }
            if self.pending_retrace && !ui.ctx().input(|i| i.pointer.any_down()) {
                self.pending_retrace = false;
                self.retrace(false);
            }
        }
        let hoop = self.hoop_size();
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
        if tab == Tab::Source {
        card(ui, t!("Size"), accent, |ui| {
            slider_row(ui, t!("Width"), &mut s.width_mm, 10.0..=360.0, " mm", 1);
            ui.horizontal(|ui| {
                ui.label(RichText::new(t!("Height {v0:.1} mm", v0 = s.width_mm * art.aspect())).color(muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("Fit hoop")).on_hover_text(t!("Largest size that fits the hoop with 5 mm to spare")).clicked() {
                        let by_w = hoop.0 - 10.0;
                        let by_h = (hoop.1 - 10.0) / art.aspect().max(1e-3);
                        s.width_mm = by_w.min(by_h).floor().max(10.0);
                    }
                });
            });
        });

        }
        if tab == Tab::Threads {
        card(ui, t!("Threads"), accent, |ui| {
            // Colors no shape uses any more are left out.
            let used_layers: Vec<bool> = (0..art.layers.len()).map(|l| s.shapes.iter().any(|e| e.layer == l && !e.hidden)).collect();
            let mut groups = artwork::thread_groups(art, s);
            groups.retain(|g| g.1.iter().any(|&i| used_layers[i]));
            for (g, (thread, members)) in groups.iter().enumerate() {
                let mut on = members.iter().all(|&i| s.layers[i].enabled);
                let mut t = *thread;
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, "").on_hover_text(t!("Stitch these colors")).changed() {
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
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(RichText::new(t!("Stitch as")).size(12.0).color(muted));
                let mut look = s.layers[members[0]].look;
                let name = match look {
                    Look::Default => t!("Auto").to_string(),
                    Look::Fill(k) => crate::i18n::tr(crate::lettering::fill_name(k)).to_string(),
                    Look::Outline => t!("Outline").into(),
                    Look::Applique => t!("Appliqué").into(),
                };
                egui::ComboBox::from_id_salt(("look", g)).selected_text(name).width(78.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut look, Look::Default, t!("Auto")).on_hover_text(t!("Use the Fill card below"));
                    for k in [FillKind::Tatami, FillKind::Satin, FillKind::Wave, FillKind::Open] {
                        ui.selectable_value(&mut look, Look::Fill(k), crate::i18n::tr(crate::lettering::fill_name(k)).to_string());
                    }
                    ui.selectable_value(&mut look, Look::Outline, t!("Outline")).on_hover_text(t!("Only the shape's outline"));
                    ui.selectable_value(&mut look, Look::Applique, t!("Appliqué"))
                        .on_hover_text(t!("Sew a piece of fabric on: placement line, stop, tack-down, stop, satin cover"));
                });
                for &i in members {
                    s.layers[i].look = look;
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
                ui.label(RichText::new(tn!(used, "{n} thread in use", "{n} threads in use")).color(muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for n in [2usize, 4, 6, 8] {
                        if n < used && ui.small_button(n.to_string()).on_hover_text(t!("Merge the closest threads until {n} are left", n = n)).clicked() {
                            artwork::reduce_threads(art, s, n);
                        }
                    }
                    if used > 3 {
                        ui.label(RichText::new(t!("Reduce to")).size(12.0).color(muted));
                    }
                });
            });
            ui.checkbox(&mut s.group_colors, t!("Group by thread (fewer thread changes)"))
                .on_hover_text(t!("Off: stitch in the drawing's own order, changing thread whenever it changes"));
        });

        }
        if tab != Tab::Style {
            return;
        }
        card(ui, t!("Quick styles"), accent, |ui| {
            if let Some(i) = preset_chips(ui, &artwork::PRESETS.map(|p| (p.0, p.1))) {
                (artwork::PRESETS[i].2)(s);
            }
        });
        if let Some(tip) = fabric_card(ui, &mut s.style, muted) {
            self.status = Some((tip.to_string(), std::time::Instant::now()));
        }
        fill_card(ui, &mut s.style, muted, accent, false);
        card(ui, t!("Clean-up"), accent, |ui| {
            ui.checkbox(&mut s.knockout, t!("Cut hidden parts")).on_hover_text(t!("Leave areas covered by shapes on top unstitched, so layers don't pile up"));
            slider_row(ui, t!("Detail"), &mut s.min_detail_mm, 0.0..=5.0, " mm", 1).on_hover_text(t!("Shapes smaller than this many mm across are left out"));
        });
        edge_card(ui, &mut s.style, accent, t!("Outline on fills"), false);
        card(ui, t!("Lines"), accent, |ui| {
            ui.label(RichText::new(t!("How strokes in the SVG are stitched")).size(12.0).color(muted));
            segmented(ui, &mut s.stroke, &[(EdgeKind::Satin, t!("Satin")), (EdgeKind::Running, t!("Run")), (EdgeKind::Triple, t!("Triple")), (EdgeKind::None, t!("None"))], accent);
            if s.stroke == EdgeKind::Satin {
                ui.add_space(4.0);
                slider_row(ui, t!("Min"), &mut s.stroke_min_mm, 0.5..=4.0, " mm", 1).on_hover_text(t!("Thinnest satin line; thinner strokes are widened to this"));
            }
        });
        if ui.button(t!("Reset settings")).on_hover_text(t!("Also puts every shape back as it was in the drawing")).clicked() {
            self.art = Some(ArtSettings::for_artwork(art));
            self.sel.clear();
        }
        let _ = (canvas_rect, muted);
        ui.add_space(10.0);
    }

    /// Fixes the current design on the hoop as part of a combined design.
    fn pin(&mut self) {
        let Some(d) = &self.design else { return };
        let t = Transform::new(d.bounds, &self.setup.at);
        let points: Vec<(f32, f32)> = d.pattern.points.iter().map(|&p| t.apply(p)).collect();
        let mut pattern = d.pattern.clone();
        pattern.points = points;
        pattern.threads = d.blocks.iter().map(|b| b.thread.clone()).collect();
        let blocks = pattern.blocks();
        let bounds = pattern.bounds();
        let height = bounds.3 - bounds.1;
        self.pinned.push(Pinned { name: d.name.clone(), pattern, blocks, bounds });
        // Start the next part just below, so the two don't sit on top of each other.
        self.change(|s| s.at.dy += height + 50.0);
        self.toast(t!("Pinned to the hoop. Make or open the next part.").into());
    }

    /// World-space bounds of everything on the hoop, pinned parts included.
    fn hoop_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(f32, f32, f32, f32)> = None;
        let mut add = |r: (f32, f32, f32, f32)| {
            b = Some(match b {
                None => r,
                Some(o) => (o.0.min(r.0), o.1.min(r.1), o.2.max(r.2), o.3.max(r.3)),
            });
        };
        for p in &self.pinned {
            add(p.bounds);
        }
        if let Some(d) = &self.design {
            let (w, h) = self.setup.at.size(d.size().0, d.size().1);
            let at = self.setup.at;
            add((at.dx - w / 2.0, at.dy - h / 2.0, at.dx + w / 2.0, at.dy + h / 2.0));
        }
        b
    }

    fn save_jef(&mut self) {
        self.export(Out::Jef);
    }

    /// Saves what's on the hoop in `out`'s format.
    pub(super) fn export(&mut self, out: Out) {
        if !self.checked(checks::Output::Save(out)) {
            return;
        }
        let Some((points, cmds, threads)) = self.hoop_stitches() else { return };
        let stem = if self.pinned.is_empty() { self.design_stem() } else { format!("{}-combined", self.design_stem()) };
        let bytes = export::write(out, &stem, &points, &cmds, &threads);
        let title = if out == Out::Jef { t!("Save as JEF") } else { t!("Export") };
        self.ask_save(title, format!("{stem}.{}", out.ext()), out.filter(), out.ext(), bytes);
    }

    /// Saves a PDF worksheet: facts, thread sequence and an actual-size template.
    pub(super) fn save_worksheet(&mut self) {
        let Some((points, cmds, threads)) = self.hoop_stitches() else { return };
        let hoop = HOOPS[self.setup.hoop];
        let stem = self.design_stem();
        let name = self.design.as_ref().map_or(stem.clone(), |d| d.name.clone());
        let threads: Vec<formats::Thread> = threads.iter().map(|t| self.chart_thread(t).unwrap_or_else(|| t.clone())).collect();
        let chart = self.chart().name.clone();
        let bytes = crate::worksheet::pdf(&crate::worksheet::Sheet {
            chart: &chart,
            name: &name,
            hoop: &t!("{name} {w} × {h} mm", name = hoop.name, w = hoop.w, h = hoop.h),
            points: &points,
            cmds: &cmds,
            threads: &threads,
            spm: SPEEDS[self.speed],
            date: &formats::today(),
        });
        self.ask_save(t!("Save worksheet"), format!("{stem}-worksheet.pdf"), "PDF", "pdf", bytes);
    }

    /// The JEF for what's on the hoop: the design, or every pinned part with it.
    fn jef_bytes(&self) -> Option<Vec<u8>> {
        let (points, cmds, threads) = self.hoop_stitches()?;
        Some(formats::write_jef(&points, &cmds, &export::jef_numbers(&threads)))
    }

    /// What's on the hoop as points centered on the hoop, commands and one
    /// thread per color block.
    fn hoop_stitches(&self) -> Option<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>)> {
        if !self.pinned.is_empty() {
            return self.combined_stitches();
        }
        let d = self.design.as_ref()?;
        // Rotation and mirroring are baked in; the offset isn't, because the
        // machine positions designs from the hoop center itself.
        let at = Placement { dx: 0.0, dy: 0.0, ..self.setup.at };
        let t = Transform::new(d.bounds, &at);
        let points: Vec<(f32, f32)> = d.pattern.points.iter().map(|&p| t.apply(p)).collect();
        Some((points, d.pattern.cmds.clone(), d.blocks.iter().map(|b| b.thread.clone()).collect()))
    }

    /// The pinned parts and the current design as one design, keeping their
    /// places relative to each other and centering the whole on the hoop.
    fn combined_stitches(&self) -> Option<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>)> {
        let mut points: Vec<(f32, f32)> = Vec::new();
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut threads: Vec<formats::Thread> = Vec::new();
        let mut parts: Vec<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>)> =
            self.pinned.iter().map(|p| (p.pattern.points.clone(), p.pattern.cmds.clone(), p.blocks.iter().map(|b| b.thread.clone()).collect())).collect();
        if let Some(d) = &self.design {
            let t = Transform::new(d.bounds, &self.setup.at);
            parts.push((d.pattern.points.iter().map(|&p| t.apply(p)).collect(), d.pattern.cmds.clone(), d.blocks.iter().map(|b| b.thread.clone()).collect()));
        }
        for (pts, cs, ts) in parts {
            if ts.is_empty() || pts.is_empty() {
                continue;
            }
            // A thread change between parts unless the next part starts in the
            // thread the last one ended with.
            if let Some(last) = threads.last() {
                if *last != ts[0] {
                    let at = *points.last().unwrap();
                    points.push(at);
                    cmds.push(Cmd::ColorChange);
                    threads.push(ts[0].clone());
                }
                threads.extend(ts.into_iter().skip(1));
            } else {
                threads.extend(ts);
            }
            // Each part starts by moving to its first point.
            points.push(pts[0]);
            cmds.push(Cmd::Jump);
            points.extend(pts.into_iter().skip(1));
            cmds.extend(cs.into_iter().skip(1));
        }
        let b = self.hoop_bounds()?;
        let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
        let centered: Vec<(f32, f32)> = points.iter().map(|&(x, y)| (x - cx, y - cy)).collect();
        Some((centered, cmds, threads))
    }

    fn open_usb(&mut self) {
        if self.design.is_none() && self.pinned.is_empty() {
            return;
        }
        if !self.checked(checks::Output::Usb) {
            return;
        }
        self.show_usb(&self.design_stem(), None);
    }

    /// Opens the Send to USB window for the design, or for `bytes` instead.
    fn show_usb(&mut self, name: &str, bytes: Option<Vec<u8>>) {
        self.usb.open = true;
        self.usb.drives = usb::drives();
        self.usb.chosen = 0;
        self.usb.message = None;
        self.usb.name = usb::short_name(name);
        self.usb.bytes = bytes;
    }

    fn usb_window(&mut self, ctx: &egui::Context) {
        if !self.usb.open {
            return;
        }
        if let Some(rx) = &self.usb.job {
            if let Ok(result) = rx.try_recv() {
                self.usb.job = None;
                if let Ok(msg) = &result {
                    self.toast(msg.clone());
                }
                self.usb.message = Some(result);
                self.usb.drives = usb::drives();
            }
        }
        let accent = self.accent();
        let muted = self.muted();
        let mut open = true;
        let mut send = false;
        let mut eject = None;
        egui::Window::new(t!("Send to USB")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(360.0);
            let busy = self.usb.job.is_some();
            if self.usb.drives.is_empty() {
                ui.label(t!("Plug in the USB stick for the machine."));
                ui.label(RichText::new(t!("Hoop looks for removable FAT, exFAT and NTFS drives.")).size(12.0).color(muted));
            } else {
                for (i, d) in self.usb.drives.iter().enumerate() {
                    let state = if d.mount.is_some() { "ready" } else { t!("will be opened") };
                    ui.radio_value(&mut self.usb.chosen, i, format!("{} · {state}", d.title()));
                }
            }
            ui.horizontal(|ui| {
                ui.label(t!("Name"));
                let r = ui.add(egui::TextEdit::singleline(&mut self.usb.name).desired_width(110.0).char_limit(8));
                if r.changed() {
                    self.usb.name = usb::short_name(&self.usb.name);
                }
                ui.label(RichText::new(".jef").monospace().color(muted));
            });
            ui.label(RichText::new(t!("Saved into EMB/Embf, where the MC9900 looks for designs. Up to 8 letters shows best on the machine.")).size(12.0).color(muted));
            if let Some(msg) = &self.usb.message {
                match msg {
                    Ok(m) => ui.label(RichText::new(m).color(OK)),
                    Err(e) => ui.label(RichText::new(e).color(BAD)),
                };
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let can_send = !busy && !self.usb.drives.is_empty();
                if ui.add_enabled(can_send, primary(if busy { t!("Working…") } else { t!("Send") }, accent)).clicked() {
                    send = true;
                }
                if ui.add_enabled(!busy, egui::Button::new(t!("Look again"))).clicked() {
                    self.usb.drives = usb::drives();
                    self.usb.chosen = 0;
                }
                if let Some(d) = &self.usb.sent_to {
                    if ui.add_enabled(!busy, egui::Button::new("Eject")).on_hover_text(t!("Close the drive so it can be pulled out safely")).clicked() {
                        eject = Some(d.clone());
                    }
                }
            });
        });
        if !open {
            self.usb.open = false;
        }
        if send {
            if let (Some(drive), Some(bytes)) = (self.usb.drives.get(self.usb.chosen).cloned(), self.usb.bytes.clone().or_else(|| self.jef_bytes())) {
                let file = format!("{}.jef", usb::short_name(&self.usb.name));
                let (tx, rx) = channel();
                let ctx = ctx.clone();
                self.usb.sent_to = Some(drive.clone());
                std::thread::spawn(move || {
                    let r = usb::send(&drive, &file, &bytes).map(|p| t!("Sent {file} to {v1}", file = file, v1 = p.parent().map(|d| d.display().to_string()).unwrap_or_default()));
                    let _ = tx.send(r);
                    ctx.request_repaint();
                });
                self.usb.job = Some(rx);
                self.usb.message = None;
            }
        }
        if let Some(drive) = eject {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = usb::eject(&drive).map(|_| t!("{v0} is ejected. You can pull it out.", v0 = drive.title()));
                let _ = tx.send(r);
                ctx.request_repaint();
            });
            self.usb.sent_to = None;
            self.usb.job = Some(rx);
        }
    }

    /// A file name for the shown design, without extension.
    fn design_stem(&self) -> String {
        let Some(d) = &self.design else { return "design".into() };
        let stem: String = match self.mode {
            Mode::Lettering => d.name.chars().filter(|c| c.is_alphanumeric() || *c == ' ').take(24).collect::<String>().trim().replace(' ', "_"),
            Mode::Artwork => d.name.rsplit_once('.').map_or(d.name.as_str(), |(s, _)| s).to_string(),
            Mode::Viewer => {
                let stem = d.name.rsplit_once('.').map_or(d.name.as_str(), |(s, _)| s).to_string();
                if d.pattern.format == formats::Format::Jef { format!("{stem}-hoop") } else { stem }
            }
        };
        if stem.is_empty() { "lettering".into() } else { stem }
    }

    fn ask_save(&mut self, title: &'static str, name: String, filter: &'static str, ext: &'static str, bytes: Vec<u8>) {
        if self.saver.is_some() {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let dir = [home.join("Documents/Embroidery Designs"), home.join("Documents"), home]
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_default();
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new().set_title(title).set_directory(dir).set_file_name(name).add_filter(filter, &[ext]).save_file();
            let _ = tx.send(file);
            ctx.request_repaint();
        });
        self.saver = Some((rx, bytes, ext));
    }

    fn save_picture(&mut self) {
        if self.design.is_some() && self.capture == Capture::Idle && self.saver.is_none() {
            self.capture = Capture::Requested;
            self.ctx.request_repaint();
        }
    }

    /// Crops the screenshot to the canvas and asks where to save it as PNG.
    fn finish_picture(&mut self, image: &egui::ColorImage, rect: egui::Rect, ppp: f32) {
        let [iw, ih] = image.size;
        let x0 = ((rect.min.x * ppp).round() as usize).min(iw);
        let y0 = ((rect.min.y * ppp).round() as usize).min(ih);
        let x1 = ((rect.max.x * ppp).round() as usize).min(iw);
        let y1 = ((rect.max.y * ppp).round() as usize).min(ih);
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if w == 0 || h == 0 {
            return;
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in y0..y1 {
            for p in &image.pixels[y * iw + x0..y * iw + x1] {
                rgba.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
            }
        }
        let mut png_bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let written = enc.write_header().and_then(|mut wr| wr.write_image_data(&rgba));
        match written {
            Ok(()) => {
                let name = format!("{}.png", self.design_stem());
                self.ask_save(t!("Save picture"), name, t!("PNG picture"), "png", png_bytes);
            }
            Err(e) => self.error = Some(t!("Couldn't make the picture: {e}.", e = e)),
        }
    }

    fn write_file(&mut self, mut path: PathBuf, bytes: &[u8], ext: &str) {
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            path.set_extension(ext);
        }
        // Write beside the target and rename, so a failed save never leaves half a file.
        let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
        let result = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path));
        match result {
            Ok(()) => {
                self.toast(t!("Saved {path}", path = path.display()));
                self.error = None;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.error = Some(t!("Couldn't save {v0}: {e}.", v0 = path.display(), e = e));
            }
        }
    }

    fn help_overlay(&mut self, ctx: &egui::Context) {
        if !self.help {
            return;
        }
        let muted = self.muted();
        egui::Window::new(t!("Shortcuts"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Grid::new("keys").num_columns(2).spacing([18.0, 5.0]).show(ui, |ui| {
                    for (k, what) in SHORTCUTS {
                        ui.label(RichText::new(crate::i18n::tr(k)).monospace().strong());
                        ui.label(crate::i18n::tr(what));
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new(t!("Drag the design to move it, drag the fabric to pan, scroll to zoom, double-click to show the whole hoop.")).size(12.0).color(muted));
                ui.label(RichText::new(t!("Press ? or Esc to close.")).size(12.0).color(muted));
            });
    }
}

impl eframe::App for HoopApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.store();
    }

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
        // Ctrl+V with SVG code on the clipboard opens it as artwork, unless a
        // text field has the focus and takes the paste itself.
        if !ctx.egui_wants_keyboard_input() {
            let pasted = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) if t.contains("<svg") => Some(t.clone()),
                    _ => None,
                })
            });
            if let Some(text) = pasted {
                self.paste_text = text.clone();
                self.paste_svg(&text);
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
                self.resolve_font();
            }
        }
        if let Capture::Waiting(_, since) = self.capture {
            if since.elapsed().as_secs() >= 3 {
                self.capture = Capture::Idle;
                self.error = Some(t!("The picture couldn't be taken. Try again.").into());
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if let Capture::Waiting(rect, _) = self.capture {
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                self.capture = Capture::Idle;
                let ppp = ctx.pixels_per_point();
                self.finish_picture(&image, rect, ppp);
            }
        }
        self.poll_team();
        if let Some((rx, _, _)) = &self.saver {
            match rx.try_recv() {
                Ok(target) => {
                    let (_, bytes, ext) = self.saver.take().unwrap();
                    if let Some(path) = target {
                        self.write_file(path, &bytes, ext);
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
        if let Some(p) = self.open_later.take() {
            self.open(ctx, &p);
        }
        let canvas_rect = self.canvas_rect;
        self.keys(ctx, canvas_rect);

        self.refresh_check();
        self.refresh_test_piece();
        self.layout(ui, canvas_rect);
        self.preflight_window(ctx);
        self.track_history(ctx, false);
        self.help_overlay(ctx);
        self.test_window(ctx);
        self.usb_window(ctx);
        self.threads_window(ctx);
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
const WARN: Color32 = Color32::from_rgb(0xe2, 0xa6, 0x4a);

/// A rounded section with a small heading; every panel section uses one.
fn card(ui: &mut egui::Ui, title: &str, accent: Color32, add: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals();
    // A Qt-style group box: a thin frame with its title on top.
    let fill = v.panel_fill.lerp_to_gamma(v.text_color(), 0.025);
    let stroke = egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color);
    ui.label(RichText::new(title).size(12.5).strong().color(v.text_color()));
    ui.add_space(-2.0);
    egui::Frame::new().fill(fill).stroke(stroke).corner_radius(4).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    let _ = accent;
    ui.add_space(8.0);
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

/// Like `slider_row`, for whole numbers.
fn int_row<T: egui::emath::Numeric>(ui: &mut egui::Ui, label: &str, v: &mut T, range: std::ops::RangeInclusive<T>, hover: &str) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label(label);
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 86.0).max(40.0);
        ui.add(egui::Slider::new(v, range.clone()).show_value(false)).on_hover_text(hover);
        ui.add_sized([78.0, 22.0], egui::DragValue::new(v).range(range).speed(0.05));
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
    p.text(egui::pos2(x, rect.top() + 28.0), egui::Align2::LEFT_TOP, t!("{w} × {h} mm", w = h.w, h = h.h), egui::FontId::monospace(12.0), muted);
    let (note, c) = if fits { (crate::i18n::tr(h.note), muted) } else { (t!("Too small"), BAD) };
    p.text(egui::pos2(x, rect.top() + 43.0), egui::Align2::LEFT_TOP, note, egui::FontId::proportional(11.0), c);
    resp.on_hover_text(t!("Hoop {name} · key {v1}", name = h.name, v1 = i + 1))
}

/// A wrapping row of named style buttons; returns the one clicked.
fn preset_chips(ui: &mut egui::Ui, presets: &[(&'static str, &'static str)]) -> Option<usize> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (i, (name, about)) in presets.iter().enumerate() {
            if ui.add(egui::Button::new(crate::i18n::tr(name)).corner_radius(12)).on_hover_text(crate::i18n::tr(about)).clicked() {
                picked = Some(i);
            }
        }
    });
    picked
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
    const STYLES: [(FillKind, &str); 12] = [
        (FillKind::Auto, n!("Auto")), (FillKind::Column, n!("Column")), (FillKind::Tatami, n!("Tatami")),
        (FillKind::Satin, n!("Satin")), (FillKind::Contour, n!("Contour")), (FillKind::Wave, n!("Wave")),
        (FillKind::Zigzag, n!("Zigzag")), (FillKind::Crosshatch, n!("Hatch")), (FillKind::Gradient, n!("Gradient")),
        (FillKind::Open, n!("Open")), (FillKind::None, n!("None")), (FillKind::None, ""),
    ];
    let w = (ui.available_width() - 12.0) / 3.0;
    for row in STYLES.chunks(3) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for &(kind, name) in row {
                if name.is_empty() {
                    continue;
                }
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
                    FillKind::Column | FillKind::Auto => {
                        // A curved band sewn with stitches square to it; Auto
                        // adds a patch of tatami for the wide parts.
                        let right = if kind == FillKind::Auto { l + (r - l) * 0.55 } else { r };
                        let c = pos2((l + right) / 2.0, bt + 26.0);
                        let (r0, r1) = (30.0, 44.0);
                        let span = ((right - l) / 2.0 / r1).min(1.2);
                        let n = 26;
                        for k in 0..=n {
                            let a = -std::f32::consts::FRAC_PI_2 - span + 2.0 * span * k as f32 / n as f32;
                            let (cs, sn) = (a.cos(), a.sin());
                            line(pos2(c.x + cs * r0, c.y + sn * r0), pos2(c.x + cs * r1, c.y + sn * r1), 1.1);
                        }
                        if kind == FillKind::Auto {
                            let mut x = right + 4.0;
                            let mut k = 0;
                            while x < r {
                                let mut y = t - 10.0 + (k % 3) as f32 * 3.0;
                                while y < bt {
                                    line(pos2(x, y), pos2(x + 4.0, y + 4.0), 1.1);
                                    y += 7.0;
                                }
                                x += 4.0;
                                k += 1;
                            }
                        }
                    }
                    FillKind::Contour => {
                        let c = icon.center();
                        for k in 0..6 {
                            let inset = k as f32 * 3.5;
                            let rr = egui::Rect::from_center_size(c, icon.size() - vec2(2.0 * inset, 2.0 * inset));
                            if rr.width() > 2.0 && rr.height() > 2.0 {
                                p.rect_stroke(rr, 8.0 - inset.min(7.0), Stroke::new(1.1, ink), egui::StrokeKind::Middle);
                            }
                        }
                    }
                }
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() - 13.0),
                    egui::Align2::CENTER_CENTER,
                    crate::i18n::tr(name),
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
    jef_picker_labeled(ui, id, t!("Thread"), value);
}

fn jef_picker_labeled(ui: &mut egui::Ui, id: &str, label: &str, value: &mut i32) {
    let current = formats::jef_thread(*value, 0);
    let row = vec2(ui.available_width(), 26.0);
    ui.allocate_ui_with_layout(row, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label(label);
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

/// Fabrics and how they change the stitching: (name, extra row spacing in
/// mm, pull compensation, stitch length, advice shown when picked).
const FABRICS_SEW: [(&str, f32, f32, f32, &str); 4] = [
    (n!("Woven"), 0.0, 0.2, 3.5, n!("Woven cotton: tear-away stabilizer is enough.")),
    (n!("Knit"), 0.05, 0.35, 3.0, n!("T-shirts and knits: use cut-away stabilizer and hoop without stretching.")),
    (n!("Denim"), 0.0, 0.15, 4.0, n!("Denim and canvas: tear-away stabilizer, a size 90 needle.")),
    (n!("Towel"), -0.05, 0.3, 3.0, n!("Towels and fleece: lay water-soluble topping on top so stitches don't sink into the pile.")),
];

/// Quick fabric presets for the fill. Returns the advice for a picked one.
fn fabric_card(ui: &mut egui::Ui, s: &mut Settings, muted: Color32) -> Option<&'static str> {
    let mut tip = None;
    let accent = ui.visuals().selection.bg_fill;
    card(ui, t!("Fabric"), accent, |ui| {
        ui.label(RichText::new(t!("Sets row spacing, pull and stitch length for the material.")).size(12.0).color(muted));
        let base = s.fill.default_rows_mm();
        ui.horizontal(|ui| {
            for (name, rows, pull, len, advice) in FABRICS_SEW {
                let on = (s.density_mm - (base + rows)).abs() < 0.001 && (s.pull_mm - pull).abs() < 0.001 && (s.stitch_mm - len).abs() < 0.001;
                if ui.selectable_label(on, crate::i18n::tr(name)).on_hover_text(crate::i18n::tr(advice)).clicked() {
                    s.density_mm = base + rows;
                    s.pull_mm = pull;
                    s.stitch_mm = len;
                    s.underlay = true;
                    tip = Some(crate::i18n::tr(advice));
                }
            }
        });
    });
    tip
}

/// Fill style tiles and the settings of the chosen style.
fn fill_card(ui: &mut egui::Ui, s: &mut Settings, muted: Color32, accent: Color32, with_thread: bool) {
    card(ui, t!("Fill"), accent, |ui| {
        let before = s.fill;
        fill_tiles(ui, &mut s.fill, accent);
        if s.fill != before {
            s.density_mm = s.fill.default_rows_mm();
            // An outline under an open fill would show, so it starts off there.
            s.underlay = !matches!(s.fill, FillKind::Open | FillKind::Crosshatch);
        }
        if s.fill == FillKind::None {
            return;
        }
        ui.add_space(4.0);
        if with_thread {
            jef_picker(ui, "fill", &mut s.fill_thread);
        }
        if !matches!(s.fill, FillKind::Contour | FillKind::Column) {
            slider_row(ui, t!("Angle"), &mut s.fill_angle, 0.0..=180.0, "°", 0).on_hover_text(t!("Direction of the stitch rows"));
        }
        if matches!(s.fill, FillKind::Crosshatch | FillKind::Open | FillKind::Gradient) {
            slider_row(ui, t!("Rows"), &mut s.density_mm, 0.25..=3.0, " mm", 2).on_hover_text(t!("Distance between fill rows"));
        }
        if s.fill == FillKind::Gradient {
            slider_row(ui, t!("Fade"), &mut s.fade_mm, 0.5..=6.0, " mm", 1).on_hover_text(t!("Row spacing at the light end"));
        }
        if matches!(s.fill, FillKind::Wave | FillKind::Zigzag) {
            slider_row(ui, t!("Swing"), &mut s.wave_amp_mm, 0.5..=8.0, " mm", 1).on_hover_text(t!("How far the rows swing up and down"));
            slider_row(ui, t!("Length"), &mut s.wave_len_mm, 3.0..=40.0, " mm", 1).on_hover_text(t!("Distance from one wave to the next"));
        }
        let note = match s.fill {
            FillKind::Satin => t!("Satin looks best on strokes under 8 mm wide."),
            FillKind::Wave | FillKind::Zigzag => t!("The pattern shows in how the thread catches the light."),
            FillKind::Crosshatch | FillKind::Open => t!("Open fills let the fabric show through. Nice on felt and canvas."),
            FillKind::Gradient => t!("Rows thin out across each shape. Turn the angle to set the direction."),
            FillKind::Contour => t!("Rings follow the outline inward. Lovely on round shapes and bold letters."),
            FillKind::Column => t!("Satin that turns with the shape, like hand-digitized lettering and borders. For parts up to about 7 mm wide."),
            FillKind::Auto => t!("Narrow parts become turning satin columns, wide parts tatami at this angle."),
            _ => "",
        };
        if !note.is_empty() {
            ui.label(RichText::new(note).size(12.0).color(muted));
        }
        egui::CollapsingHeader::new(RichText::new(t!("More settings")).color(muted)).id_salt(("fill-more", with_thread)).show(ui, |ui| {
            if !matches!(s.fill, FillKind::Crosshatch | FillKind::Open | FillKind::Gradient) {
                slider_row(ui, t!("Rows"), &mut s.density_mm, 0.25..=3.0, " mm", 2).on_hover_text(t!("Distance between fill rows. Smaller is denser."));
            }
            if s.fill != FillKind::Satin {
                slider_row(ui, t!("Stitch"), &mut s.stitch_mm, 2.0..=6.0, " mm", 1).on_hover_text(t!("Stitch length along each row"));
            }
            slider_row(ui, t!("Pull"), &mut s.pull_mm, 0.0..=1.0, " mm", 2)
                .on_hover_text(t!("Stitches pull the fabric in, so rows are made this much longer at each end"));
            ui.checkbox(&mut s.underlay, t!("Underlay outline")).on_hover_text(t!("A running stitch around each shape first, to hold the fabric"));
        });
    });
}
fn edge_card(ui: &mut egui::Ui, s: &mut Settings, accent: Color32, title: &str, with_thread: bool) {
    card(ui, title, accent, |ui| {
        segmented(ui, &mut s.edge, &[(EdgeKind::Satin, t!("Satin")), (EdgeKind::Running, t!("Run")), (EdgeKind::Triple, t!("Triple")), (EdgeKind::None, t!("None"))], accent);
        if s.edge == EdgeKind::Satin {
            ui.add_space(4.0);
            slider_row(ui, t!("Width"), &mut s.edge_width_mm, 0.8..=5.0, " mm", 1);
        }
        if with_thread && s.edge != EdgeKind::None {
            jef_picker(ui, "edge", &mut s.edge_thread);
        }
    });
}
