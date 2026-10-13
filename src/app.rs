use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, Key, KeyboardShortcut, Modifiers, RichText, Sense, Vec2, vec2};

use crate::artwork::{self, ArtSettings, Look};
use crate::canvas::{self, DesignStyle, Frame, Transform, View};
use crate::export::{self, Out};
use crate::formats::{self, Block, Cmd, Pattern};
use crate::hoops::{self, Fit, Placement};

mod artwork_panel;
mod canvas_view;
mod checks;
mod demo;
mod draw;
mod history;
mod hoopings;
mod icons;
mod lettering_panel;
mod library;
mod manual;
mod optimizewin;
mod output;
mod pages;
mod settings;
mod shapes;
mod shell;
mod stitchedit;
mod style_cards;
mod team;
mod testpiece;
mod threadwin;
mod tools;
mod usbwin;
mod widgets;
mod wizard;

use crate::lettering::{self, Align, EdgeKind, FillKind, FontSource, PatchShape, Settings};
use crate::prefs::Prefs;
use crate::theme::Theme;
use crate::trace::{self, TraceSettings};
use crate::usb;
use shell::{Dock, Tab};
use style_cards::*;
use widgets::*;

const FABRICS: [(&str, [u8; 3]); 8] = [
    (n!("Natural linen"), [0xd6, 0xc9, 0xb0]),
    (n!("White cotton"), [0xf6, 0xf5, 0xf0]),
    (n!("Heather gray"), [0x9c, 0x9b, 0x9d]),
    (n!("Denim"), [0x3e, 0x57, 0x78]),
    (n!("Navy"), [0x1e, 0x2a, 0x44]),
    (n!("Black"), [0x1c, 0x1b, 0x1d]),
    (n!("Brick red"), [0x9e, 0x2a, 0x2b]),
    (n!("Sage"), [0x9a, 0xab, 0x8d]),
];
const SPEEDS: [u32; 4] = [400, 600, 800, 860];
const PLAYBACK: [(f32, &str); 4] = [(1.0, n!("Real time")), (10.0, "10×"), (30.0, "30×"), (120.0, "120×")];

const SHORTCUTS: &[(&str, &str)] = &[
    (n!("O"), n!("Open a file")),
    ("V / L / A", n!("Viewer / lettering / artwork")),
    (n!("Ctrl+S"), n!("Save in the machine's format")),
    (n!("Ctrl+U"), n!("Send to the USB stick for the machine")),
    (n!("Ctrl+T"), n!("Test stitch-out: sew a small part, or the calibration sheet")),
    (n!("Ctrl+Shift+S"), n!("Save a picture of the preview")),
    (n!("Ctrl+P"), n!("Save a worksheet PDF with an actual-size template")),
    (n!("Ctrl+,"), n!("Settings: language, folders, machine, threads")),
    (n!("N"), n!("New project wizard")),
    (n!("F1"), n!("User manual")),
    (n!("F9 / F10"), n!("Hide or show the left / right panel")),
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
    Split,
}

/// Everything undo covers: hoop and placement plus the lettering and
/// artwork settings.
/// A save dialog in progress: its answer, the bytes, and the extension.
type SaveJob = (Receiver<Option<PathBuf>>, Vec<u8>, &'static str);

#[derive(Clone, PartialEq)]
struct Snapshot {
    setup: Setup,
    letter: Settings,
    art: Option<ArtSettings>,
    /// An opened stitch file as edited.
    stitches: Option<stitchedit::Shared>,
}

#[derive(Clone, Copy, PartialEq)]
struct Setup {
    hoop: usize,
    at: Placement,
}

enum Drag {
    Move {
        start: (f32, f32),
        from: Placement,
    },
    Pan,
    /// Moving the picked artwork shapes: where the drag started and each
    /// shape's offset then (SVG user units), the box of the moving shapes,
    /// and the x and y lines they snap to.
    Shape {
        start: (f32, f32),
        from: Vec<(usize, (f32, f32))>,
        moving: (f32, f32, f32, f32),
        snap: (Vec<f32>, Vec<f32>),
    },
    /// Picking shapes with a rubber band, from this screen point; `add`
    /// keeps the shapes already picked.
    Band {
        start: egui::Pos2,
        add: bool,
    },
    /// Moving one point of the selected shape: line and point index.
    Node {
        line: usize,
        point: usize,
    },
    /// Resizing the selected shape by a corner: size and pointer distance
    /// from the shape's middle when the drag started.
    Scale {
        from: f32,
        start: f32,
    },
    /// Turning the selected shape: its turn and the pointer's angle at the start.
    Turn {
        from: f32,
        start: f32,
    },
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
    saver: Option<SaveJob>,
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
    tools: tools::Tools,
    hoopings: hoopings::Hoopings,
    stitch_edit: stitchedit::StitchEdit,
    backdrop: Option<draw::Backdrop>,
    pen: draw::Pen,
    settings: settings::SettingsWindow,
    wizard: wizard::Wizard,
    manual: manual::Manual,
    demo: Option<demo::Demo>,
    optimize: optimizewin::OptimizeWin,
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
            committed: Snapshot { setup: Setup { hoop: 0, at: Placement::default() }, letter: Settings::default(), art: None, stitches: None },
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
            tools: Default::default(),
            hoopings: Default::default(),
            stitch_edit: Default::default(),
            backdrop: None,
            pen: Default::default(),
            settings: Default::default(),
            wizard: Default::default(),
            manual: Default::default(),
            demo: None,
            optimize: Default::default(),
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
        load_ui_font(&cc.egui_ctx, app.prefs.get("ui_font") == Some("theme"));
        app.apply_theme(&cc.egui_ctx);
        let demo = file.as_deref() == Some(std::path::Path::new("--demo"));
        if let Some(path) = file.filter(|_| !demo) {
            app.open(&cc.egui_ctx, &path);
        }
        if demo {
            app.start_demo();
        }
        app.debug_start(&cc.egui_ctx);
        app
    }

    /// For screenshots in tests: HOOP_MODE=viewer|lettering|artwork and
    /// HOOP_TAB=<tab label> pick what the window opens on.
    fn debug_start(&mut self, ctx: &egui::Context) {
        if let Ok(p) = std::env::var("HOOP_BACKDROP") {
            self.load_backdrop(ctx, std::path::Path::new(&p));
            self.pen.on = true;
        }
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
        if std::env::var("HOOP_DENSITY").is_ok() {
            self.density = true;
        }
        if let Some(i) = std::env::var("HOOP_SELECT").ok().and_then(|v| v.parse::<usize>().ok()) {
            self.sel = vec![i];
        }
        match std::env::var("HOOP_WINDOW").as_deref() {
            Ok("threads") => self.threads.open = true,
            Ok("test") => self.test.open = true,
            Ok("help") => self.help = true,
            Ok("settings") => self.settings.open = true,
            Ok("manual") => self.manual.open = true,
            Ok("optimize") => self.optimize.open = true,
            Ok("wizard") => {
                self.open_wizard();
                if let Ok(m) = std::env::var("HOOP_WIZARD_MAKE") {
                    self.wizard_debug(ctx, &m);
                }
            }
            Ok("split") => {
                self.open_hoopings();
                if let Some(h) = std::env::var("HOOP_SPLIT_HOOP").ok().and_then(|v| v.parse().ok()) {
                    self.hoopings.hoop = h;
                }
            }
            _ => {}
        }
        if let Ok(t) = std::env::var("HOOP_TAB")
            && let Some(tab) = shell::tab_named(self.mode, &t)
        {
            self.tab[self.mode as usize] = tab;
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
                .add_filter(
                    "Designs, artwork and pictures",
                    &[
                        "jef",
                        "dst",
                        "exp",
                        "pes",
                        "pec",
                        "svg",
                        "svgz",
                        "png",
                        "jpg",
                        "jpeg",
                        "webp",
                        "bmp",
                        "gif",
                        t!("JEF"),
                        t!("DST"),
                        t!("EXP"),
                        t!("PES"),
                        t!("PEC"),
                        t!("SVG"),
                        t!("PNG"),
                        t!("JPG"),
                        t!("JPEG"),
                    ],
                )
                .pick_file();
            let _ = tx.send(file);
            ctx.request_repaint();
        });
        self.picker = Some(rx);
    }

    fn hoop_size(&self) -> (f32, f32) {
        let h = hoops::get(self.setup.hoop);
        (h.w, h.h)
    }

    fn toast(&mut self, text: String) {
        self.status = Some((text, std::time::Instant::now()));
        self.ctx.request_repaint();
    }

    /// Settings from the last session.
    fn restore(&mut self) {
        crate::i18n::set_language(self.prefs.get("language").unwrap_or("system"));
        if let Some(i) = self.prefs.get("machine").and_then(|k| hoops::MACHINES.iter().position(|m| m.key == k)) {
            hoops::set_machine(i);
        }
        let p = &self.prefs;
        if let Some(c) = p.get("fabric").and_then(|v| u32::from_str_radix(v, 16).ok()) {
            self.fabric = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        }
        for (key, flag) in [
            ("grid", &mut self.grid),
            ("jumps", &mut self.jumps),
            ("texture", &mut self.textured),
            ("sizes", &mut self.sizes),
            ("show_left", &mut self.show_left),
            ("show_right", &mut self.show_right),
        ] {
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
        self.letter.column_max_mm = self.column_max();
        self.tuning = p.tuning();
        self.want_font = p.get("letter.font").map(str::to_string);
        self.resolve_font();
    }

    /// Picks the remembered font once it shows up in the font list.
    fn resolve_font(&mut self) {
        if let Some(name) = &self.want_font
            && let Some(i) = self.fonts.iter().position(|f| &f.name == name)
        {
            self.letter.font = i;
            self.want_font = None;
        }
    }

    fn store(&mut self) {
        let f = self.fabric;
        self.prefs.set("fabric", format!("{:02x}{:02x}{:02x}", f[0], f[1], f[2]));
        self.prefs.set("grid", self.grid);
        self.prefs.set("jumps", self.jumps);
        self.prefs.set("texture", self.textured);
        self.prefs.set("sizes", self.sizes);
        self.prefs.set("show_left", self.show_left);
        self.prefs.set("show_right", self.show_right);
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

    fn help_overlay(&mut self, ctx: &egui::Context) {
        if !self.help {
            return;
        }
        let muted = self.muted();
        egui::Window::new(t!("Shortcuts")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).spacing([18.0, 5.0]).show(ui, |ui| {
                for (k, what) in SHORTCUTS {
                    ui.label(RichText::new(crate::i18n::tr(k)).monospace().strong());
                    ui.label(crate::i18n::tr(what));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(t!("Drag the design to move it, drag the fabric to pan, scroll to zoom, double-click to show the whole hoop."))
                    .size(12.0)
                    .color(muted),
            );
            ui.label(RichText::new(t!("Press ? or Esc to close.")).size(12.0).color(muted));
        });
    }
}

impl eframe::App for HoopApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_demo();
        self.store();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        if self.theme.refresh() {
            self.apply_theme(ctx);
        }
        self.demo_tick(ctx);

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
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
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

        if let Some(rx) = &self.font_scan
            && let Ok(found) = rx.try_recv()
        {
            self.fonts.extend(found);
            self.font_scan = None;
            self.resolve_font();
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
        self.hoopings_window(ctx);
        self.settings_window(ctx);
        self.wizard_window(ctx);
        self.manual_window(ctx);
        self.optimize_window(ctx);
    }
}

const OK: Color32 = Color32::from_rgb(0x55, 0xc2, 0x8c);
const BAD: Color32 = Color32::from_rgb(0xf0, 0x6b, 0x5f);
const WARN: Color32 = Color32::from_rgb(0xe2, 0xa6, 0x4a);
