//! A 30-second demo for screen recordings: `hoop --demo` or File → Play
//! demo. Hoop plays a fixed script (lettering typed in, arcs and a badge,
//! a library shape stitched out, the check, the languages) with a caption
//! at the bottom of the canvas. The user's settings are put back after.

use std::time::Instant;

use super::*;
use crate::lettering::{PatchShape, TextShape};

#[derive(Clone)]
enum Step {
    /// A big card over the canvas: title and subtitle.
    Card(&'static str, &'static str),
    Caption(&'static str),
    Lettering,
    Text(String),
    Layout(TextShape),
    Badge,
    Shape(&'static str),
    Play,
    Density(bool),
    Language(&'static str),
    /// Opens one of the app's windows, closing any other.
    Window(Win),
    End,
}

#[derive(Clone, Copy)]
enum Win {
    None,
    Wizard,
    Threads,
    Split,
    Settings,
    Manual,
}

pub(super) struct Demo {
    start: Instant,
    steps: Vec<(f32, Step)>,
    next: usize,
    card: Option<(&'static str, &'static str)>,
    caption: Option<(&'static str, f32)>,
    saved: Option<(Settings, String, Mode, Setup)>,
}

/// The script: (seconds from the start, step).
fn script() -> Vec<(f32, Step)> {
    let mut s = vec![
        (0.0, Step::Lettering),
        (0.0, Step::Card(n!("Hoop"), n!("Embroidery designs for your machine"))),
        (2.5, Step::Caption(n!("Type it, and it's stitched"))),
    ];
    // Typed in letter by letter.
    let word = "Hoop";
    for (k, _) in word.char_indices() {
        s.push((2.6 + k as f32 * 0.3, Step::Text(word[..=k].to_string())));
    }
    s.extend([
        (6.0, Step::Caption(n!("Arcs, circles and envelopes"))),
        (6.0, Step::Layout(TextShape::Arch)),
        (7.3, Step::Layout(TextShape::Bulge)),
        (8.5, Step::Caption(n!("Badges in one click"))),
        (8.5, Step::Badge),
        (11.0, Step::Caption(n!("A wizard from idea to file"))),
        (11.0, Step::Window(Win::Wizard)),
        (13.0, Step::Window(Win::None)),
        (13.0, Step::Caption(n!("109 shapes, satin that turns with them"))),
        (13.0, Step::Shape("butterfly")),
        (13.8, Step::Play),
        (18.0, Step::Caption(n!("Checked before you sew"))),
        (18.0, Step::Density(true)),
        (20.0, Step::Density(false)),
        (20.0, Step::Caption(n!("Madeira, Isacord, Sulky… and the spools you own"))),
        (20.0, Step::Window(Win::Threads)),
        (22.5, Step::Caption(n!("Too big for the hoop? Split it"))),
        (22.5, Step::Window(Win::Split)),
        (25.0, Step::Caption(n!("English · Polski · Deutsch"))),
        (25.0, Step::Window(Win::Settings)),
        (25.8, Step::Language("pl")),
        (26.6, Step::Language("de")),
        (27.5, Step::Language("en")),
        (27.5, Step::Caption(n!("A manual built in"))),
        (27.5, Step::Window(Win::Manual)),
        (29.0, Step::Window(Win::None)),
        (29.0, Step::Card(n!("Save · USB · PDF"), n!("Hoop — free, for your machine"))),
        (31.5, Step::End),
    ]);
    s
}

impl HoopApp {
    pub(super) fn start_demo(&mut self) {
        let saved = Some((self.letter.clone(), crate::i18n::language(), self.mode, self.setup));
        self.demo = Some(Demo { start: Instant::now(), steps: script(), next: 0, card: None, caption: None, saved });
    }

    pub(super) fn demo_running(&self) -> bool {
        self.demo.is_some()
    }

    /// Runs the steps that are due. Called every frame while the demo plays.
    pub(super) fn demo_tick(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.demo else { return };
        ctx.request_repaint();
        let now = d.start.elapsed().as_secs_f32();
        let mut due = Vec::new();
        while d.next < d.steps.len() && d.steps[d.next].0 <= now {
            due.push(d.steps[d.next].1.clone());
            d.next += 1;
        }
        for step in due {
            match step {
                Step::Card(a, b) => {
                    if let Some(d) = &mut self.demo {
                        d.card = Some((a, b));
                    }
                }
                Step::Caption(c) => {
                    if let Some(d) = &mut self.demo {
                        d.card = None;
                        d.caption = Some((c, now));
                    }
                }
                Step::Lettering => {
                    self.set_mode(Mode::Lettering);
                    let font = self.letter.font;
                    self.letter = Settings { font, text: "Hoop".into(), height_mm: 30.0, ..Default::default() };
                    lettering::style(&mut self.letter, FillKind::Column, 90.0, EdgeKind::None);
                    self.letter.fill_thread = 42;
                    self.setup = Setup { hoop: 0, at: Placement::default() };
                    self.design = None;
                    self.fit_pending = true;
                }
                Step::Text(t) => {
                    self.letter.text = t;
                    self.error = None;
                    self.fit_pending = true;
                }
                Step::Layout(shape) => {
                    self.letter.shape = shape;
                    self.letter.envelope_mm = 16.0;
                }
                Step::Badge => {
                    let s = &mut self.letter;
                    s.text = "HOOP\nEMBROIDERY".into();
                    s.height_mm = 9.0;
                    s.shape = TextShape::Circle;
                    s.circle_mm = 60.0;
                    s.patch = PatchShape::Circle;
                    s.patch_fill = FillKind::Tatami;
                    s.patch_pad_mm = 4.0;
                    s.patch_border_mm = 3.0;
                    s.patch_thread = 232;
                    s.patch_border_thread = 239;
                    s.fill_thread = 2;
                    self.fit_pending = true;
                }
                Step::Shape(name) => {
                    if let Some(path) = library::shapes_dir().map(|d| d.join(format!("{name}.svg"))) {
                        self.open(&path);
                        if let Some(a) = &mut self.art {
                            a.width_mm = 110.0;
                            if let Some(l) = a.layers.first_mut() {
                                l.thread = 230;
                            }
                        }
                        self.fit_pending = true;
                    }
                }
                Step::Play => {
                    self.regenerate_art();
                    self.play_rate = PLAYBACK.len() - 1;
                    self.shown = 0;
                    self.playing = true;
                }
                Step::Density(on) => {
                    self.playing = false;
                    self.shown = self.total();
                    self.density = on;
                }
                Step::Language(code) => {
                    crate::i18n::set_language(code);
                }
                Step::Window(w) => {
                    self.wizard.open = false;
                    self.threads.open = false;
                    self.hoopings.open = false;
                    self.settings.open = false;
                    self.manual.open = false;
                    match w {
                        Win::None => {}
                        Win::Wizard => self.open_wizard(),
                        Win::Threads => self.threads.open = true,
                        Win::Split => {
                            self.open_hoopings();
                            // The small free-arm hoop, so the shape needs several.
                            self.hoopings.hoop = 3;
                        }
                        Win::Settings => self.settings.open = true,
                        Win::Manual => self.manual.open = true,
                    }
                }
                Step::End => {
                    self.stop_demo();
                }
            }
        }
    }

    /// Ends the demo and puts the user's settings back.
    pub(super) fn stop_demo(&mut self) {
        if let Some(d) = self.demo.take() {
            if let Some((letter, lang, mode, setup)) = d.saved {
                self.letter = letter;
                crate::i18n::set_language(if lang.is_empty() { self.prefs.get("language").unwrap_or("system") } else { &lang });
                self.setup = setup;
                self.set_mode(mode);
            }
            self.density = false;
            self.playing = false;
            self.wizard.open = false;
            self.threads.open = false;
            self.hoopings.open = false;
            self.settings.open = false;
            self.manual.open = false;
        }
    }

    /// The caption or card over the canvas.
    pub(super) fn demo_overlay(&self, canvas: &egui::Painter, frame: &Frame) {
        let Some(d) = &self.demo else { return };
        // Above the windows, so captions stay readable over them.
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("demo"));
        let painter = &canvas.ctx().layer_painter(layer).with_clip_rect(frame.rect);
        let now = d.start.elapsed().as_secs_f32();
        let accent = self.accent();
        if let Some((title, sub)) = d.card {
            painter.rect_filled(frame.rect, 0, Color32::from_black_alpha(170));
            let c = frame.rect.center();
            painter.text(c - vec2(0.0, 24.0), egui::Align2::CENTER_CENTER, crate::i18n::tr(title), egui::FontId::proportional(64.0), Color32::WHITE);
            painter.text(
                c + vec2(0.0, 36.0),
                egui::Align2::CENTER_CENTER,
                crate::i18n::tr(sub),
                egui::FontId::proportional(22.0),
                Color32::from_white_alpha(220),
            );
            return;
        }
        if let Some((text, since)) = d.caption {
            // Fade in over a quarter second.
            let a = ((now - since) / 0.25).clamp(0.0, 1.0);
            let galley = painter.layout(
                crate::i18n::tr(text).to_string(),
                egui::FontId::proportional(26.0),
                Color32::WHITE.gamma_multiply(a),
                frame.rect.width() - 120.0,
            );
            let size = galley.size() + vec2(48.0, 24.0);
            let r = egui::Rect::from_center_size(frame.rect.center_bottom() - vec2(0.0, 70.0), size);
            painter.rect_filled(r, 12, Color32::from_black_alpha((200.0 * a) as u8));
            painter.rect_filled(egui::Rect::from_min_size(r.left_top(), vec2(6.0, r.height())), 3, accent.gamma_multiply(a));
            painter.galley(r.min + vec2(24.0, 12.0), galley, Color32::WHITE);
        }
    }
}
