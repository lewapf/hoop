//! New project wizard: from "what do you want to make" to a design on the
//! hoop and a file for the machine, one question at a time.

use std::path::PathBuf;

use super::*;
use crate::lettering::{PatchShape, TextShape};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Make {
    Name,
    Monogram,
    Badge,
    KeyFob,
    Coaster,
    Lace,
    Logo,
    Shape,
}

impl Make {
    const ALL: [Make; 8] = [Make::Name, Make::Monogram, Make::Badge, Make::KeyFob, Make::Coaster, Make::Lace, Make::Logo, Make::Shape];

    fn title(self) -> &'static str {
        match self {
            Make::Name => n!("Name or text"),
            Make::Monogram => n!("Monogram"),
            Make::Badge => n!("Round badge"),
            Make::KeyFob => n!("Key fob (in the hoop)"),
            Make::Coaster => n!("Coaster (in the hoop)"),
            Make::Lace => n!("Lace ornament"),
            Make::Logo => n!("Logo or picture"),
            Make::Shape => n!("Shape from the library"),
        }
    }

    fn about(self) -> &'static str {
        match self {
            Make::Name => n!("Lettering in satin columns, for towels, bags and shirts"),
            Make::Monogram => n!("Three initials, the middle one larger"),
            Make::Badge => n!("Text round a circle with a stitched background and border"),
            Make::KeyFob => n!("An appliqué tag with a key-ring eyelet, finished on both sides"),
            Make::Coaster => n!("A round appliqué coaster backed with fabric"),
            Make::Lace => n!("Free-standing lace on water-soluble stabilizer"),
            Make::Logo => n!("An SVG or a picture turned into stitches"),
            Make::Shape => n!("A ready-made shape: heart, star, animal, sport…"),
        }
    }

    fn text_based(self) -> bool {
        !matches!(self, Make::Logo | Make::Shape)
    }
}

pub(super) struct Wizard {
    pub open: bool,
    step: usize,
    make: Make,
    fabric: usize,
    text: String,
    file: Option<PathBuf>,
    pick: Option<Receiver<Option<PathBuf>>>,
    size_mm: f32,
    hoop: usize,
    thread: i32,
    accent: i32,
}

impl Default for Wizard {
    fn default() -> Self {
        Wizard { open: false, step: 0, make: Make::Name, fabric: 0, text: String::new(), file: None, pick: None, size_mm: 20.0, hoop: 0, thread: 42, accent: 1 }
    }
}

const STEPS: usize = 6;

impl HoopApp {
    pub(super) fn open_wizard(&mut self) {
        self.wizard = Wizard { open: true, hoop: self.setup.hoop, ..Default::default() };
    }

    /// Sensible starting values for each kind of project.
    fn wizard_defaults(&mut self) {
        let w = &mut self.wizard;
        let (text, size) = match w.make {
            Make::Name => (t!("Anna"), 20.0),
            Make::Monogram => (t!("AMB"), 40.0),
            Make::Badge => (t!("BREMEN\nGERMANY"), 9.0),
            Make::KeyFob => (t!("MILA"), 10.0),
            Make::Coaster => (t!("Home"), 14.0),
            Make::Lace => (t!("Noel"), 14.0),
            Make::Logo => ("", 80.0),
            Make::Shape => ("", 60.0),
        };
        w.text = text.to_string();
        w.size_mm = size;
        w.file = None;
    }

    /// Builds the project from the answers.
    fn wizard_create(&mut self, ctx: &egui::Context) {
        let w = &self.wizard;
        let (make, fabric, size, hoop, thread, accent) = (w.make, w.fabric, w.size_mm, w.hoop, w.thread, w.accent);
        let text = w.text.clone();
        let file = w.file.clone();
        let fab = FABRICS_SEW[fabric.min(FABRICS_SEW.len() - 1)];
        let apply_fabric = |s: &mut Settings| {
            s.density_mm = s.fill.default_rows_mm() + fab.1;
            s.pull_mm = fab.2;
            s.stitch_mm = fab.3;
            s.underlay = true;
            s.underlay_kind = fab.4;
        };
        if make.text_based() {
            self.set_mode(Mode::Lettering);
            let font = self.letter.font;
            let mut s = Settings { font, text, height_mm: size, column_max_mm: self.column_max(), ..Default::default() };
            let preset = |name: &str| lettering::PRESETS.iter().find(|p| p.0 == name).map(|p| p.2);
            match make {
                Make::Name => lettering::style(&mut s, FillKind::Auto, 45.0, EdgeKind::None),
                Make::Monogram => {
                    lettering::style(&mut s, FillKind::Auto, 45.0, EdgeKind::None);
                    s.shape = TextShape::Monogram;
                }
                Make::Badge => {
                    lettering::style(&mut s, FillKind::Column, 90.0, EdgeKind::None);
                    s.shape = TextShape::Circle;
                    s.circle_mm = size * 6.0;
                    s.patch = PatchShape::Circle;
                    s.patch_pad_mm = 4.0;
                    s.patch_fill = FillKind::Tatami;
                    s.patch_border_mm = 3.0;
                }
                Make::KeyFob => preset("Key fob").map_or((), |f| f(&mut s)),
                Make::Coaster => preset("Coaster").map_or((), |f| f(&mut s)),
                Make::Lace => preset("Lace").map_or((), |f| f(&mut s)),
                _ => {}
            }
            s.height_mm = size;
            apply_fabric(&mut s);
            s.fill_thread = thread;
            s.edge_thread = thread;
            s.patch_border_thread = accent;
            if s.patch != PatchShape::None && !s.patch_applique {
                s.patch_thread = accent;
            }
            self.letter = s;
            self.setup.hoop = hoop;
            self.setup.at = Placement::default();
            self.regenerate(ctx);
        } else if let Some(path) = file {
            self.open(ctx, &path);
            if let Some(s) = &mut self.art {
                apply_fabric(&mut s.style);
                s.width_mm = size;
            }
            self.setup.hoop = hoop;
            self.setup.at = Placement::default();
        }
        self.reset_history();
    }

    /// For screenshots: picks a kind by name and creates it straight away.
    pub(super) fn wizard_debug(&mut self, ctx: &egui::Context, make: &str) {
        if let Some(m) = Make::ALL.into_iter().find(|m| format!("{m:?}") == make) {
            self.wizard.make = m;
            self.wizard_defaults();
            if m == Make::Shape {
                self.wizard.file = library::shapes_dir().map(|d| d.join("butterfly.svg"));
            }
            self.wizard_create(ctx);
            self.wizard.step = STEPS - 1;
        }
    }

    pub(super) fn wizard_window(&mut self, ctx: &egui::Context) {
        if !self.wizard.open {
            return;
        }
        if let Some(rx) = &self.wizard.pick
            && let Ok(answer) = rx.try_recv()
        {
            self.wizard.pick = None;
            if answer.is_some() {
                self.wizard.file = answer;
            }
        }
        let muted = self.muted();
        let accent = self.accent();
        let mut open = true;
        let mut next = false;
        let mut back = false;
        let mut create = false;
        let mut pick: Option<Option<PathBuf>> = None;
        let mut finish: Option<u8> = None;
        egui::Window::new(t!("New project")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(
            ctx,
            |ui| {
                ui.set_width(460.0);
                let w = &mut self.wizard;
                ui.label(RichText::new(t!("Step {k} of {n}", k = w.step + 1, n = STEPS)).size(12.0).color(muted));
                ui.add_space(4.0);
                match w.step {
                    0 => {
                        ui.heading(t!("What would you like to make?"));
                        for m in Make::ALL {
                            let r =
                                ui.add(egui::Button::selectable(w.make == m, RichText::new(crate::i18n::tr(m.title())).strong()).min_size(vec2(440.0, 0.0)));
                            ui.label(RichText::new(crate::i18n::tr(m.about())).size(11.5).color(muted));
                            if r.clicked() {
                                w.make = m;
                            }
                        }
                    }
                    1 => {
                        ui.heading(t!("What will you sew on?"));
                        ui.horizontal_wrapped(|ui| {
                            for (i, f) in FABRICS_SEW.iter().enumerate() {
                                if ui.selectable_label(w.fabric == i, crate::i18n::tr(f.0)).clicked() {
                                    w.fabric = i;
                                }
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(RichText::new(crate::i18n::tr(FABRICS_SEW[w.fabric].5)).color(muted));
                        if matches!(w.make, Make::Lace) {
                            ui.label(
                                RichText::new(t!("Lace is sewn on two layers of water-soluble stabilizer, whatever the fabric choice."))
                                    .size(12.0)
                                    .color(muted),
                            );
                        }
                    }
                    2 => match w.make {
                        Make::Logo | Make::Shape => {
                            ui.heading(if w.make == Make::Logo { t!("Pick the logo or picture") } else { t!("Pick a shape") });
                            if w.make == Make::Shape {
                                if let Some(dir) = library::shapes_dir() {
                                    let mut names: Vec<String> = std::fs::read_dir(&dir)
                                        .map(|rd| {
                                            rd.flatten()
                                                .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
                                                .filter(|n| n != "LICENSE")
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    names.sort();
                                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            for n in names {
                                                let path = dir.join(format!("{n}.svg"));
                                                let on = w.file.as_ref() == Some(&path);
                                                if ui.selectable_label(on, n.replace('-', " ")).clicked() {
                                                    w.file = Some(path);
                                                }
                                            }
                                        });
                                    });
                                }
                            } else if ui.button(t!("Choose a file…")).clicked() {
                                pick = Some(None);
                            }
                            if let Some(f) = &w.file {
                                ui.label(RichText::new(f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).strong());
                            }
                        }
                        Make::Monogram => {
                            ui.heading(t!("Your initials"));
                            ui.add(egui::TextEdit::singleline(&mut w.text).char_limit(3).font(egui::TextStyle::Heading));
                        }
                        _ => {
                            ui.heading(t!("Your text"));
                            ui.label(
                                RichText::new(if w.make == Make::Badge {
                                    t!("Line 1 goes round the top, line 2 round the bottom.")
                                } else {
                                    t!("One or more lines.")
                                })
                                .size(12.0)
                                .color(muted),
                            );
                            ui.add(egui::TextEdit::multiline(&mut w.text).desired_rows(2).font(egui::TextStyle::Heading).desired_width(f32::INFINITY));
                        }
                    },
                    3 => {
                        ui.heading(t!("How big?"));
                        let label = if w.make.text_based() { t!("Letter height") } else { t!("Width") };
                        let range = if w.make.text_based() { 5.0..=80.0 } else { 20.0..=200.0 };
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.add(egui::Slider::new(&mut w.size_mm, range).suffix(" mm").fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label(t!("Hoop"));
                            egui::ComboBox::from_id_salt("wiz-hoop").selected_text(hoops::get(w.hoop).name).show_ui(ui, |ui| {
                                for (i, h) in hoops::list().iter().enumerate() {
                                    ui.selectable_value(&mut w.hoop, i, format!("{}  {} × {} mm", h.name, h.w, h.h));
                                }
                            });
                        });
                        if w.make.text_based() && w.size_mm < 8.0 {
                            ui.label(
                                RichText::new(t!("Letters under 8 mm are sewn as narrow satin; use a thin (60 wt) thread if you have one."))
                                    .size(12.0)
                                    .color(WARN),
                            );
                        }
                    }
                    4 => {
                        ui.heading(t!("Thread colors"));
                        if w.make.text_based() {
                            jef_picker_labeled(ui, "wiz-main", t!("Text"), &mut w.thread);
                            if matches!(w.make, Make::Badge | Make::KeyFob | Make::Coaster | Make::Lace) {
                                jef_picker_labeled(ui, "wiz-accent", t!("Border"), &mut w.accent);
                            }
                        } else {
                            ui.label(RichText::new(t!("Artwork keeps its own colors; change them later in the Threads tab.")).color(muted));
                        }
                    }
                    _ => {
                        ui.heading(t!("Ready"));
                        ui.label(t!("Your design is on the hoop. Check it, then:"));
                        let steps: Vec<String> = match w.make {
                            Make::KeyFob | Make::Coaster => vec![
                                t!("Hoop tear-away or no-show mesh stabilizer.").into(),
                                t!("At each stop, do what the thread name says: lay the front fabric, trim, tape backing under the hoop.").into(),
                                t!("Cut round the border, close to the satin; for a fob, punch the eyelet.").into(),
                            ],
                            Make::Lace => vec![
                                t!("Hoop two layers of water-soluble stabilizer.").into(),
                                t!("Sew, cut away the extra stabilizer and rinse the rest out.").into(),
                            ],
                            _ => vec![crate::i18n::tr(FABRICS_SEW[w.fabric].5).to_string()],
                        };
                        for s in steps {
                            ui.label(format!("•  {s}"));
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.add(primary(t!("Save…"), accent)).clicked() {
                                finish = Some(0);
                            }
                            if ui.button(t!("Send to USB…")).clicked() {
                                finish = Some(1);
                            }
                            if ui.button(t!("Save worksheet…")).clicked() {
                                finish = Some(2);
                            }
                        });
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if w.step > 0 && w.step < STEPS - 1 && ui.button(t!("Back")).clicked() {
                        back = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = match w.step {
                            2 => {
                                if w.make.text_based() {
                                    !w.text.trim().is_empty()
                                } else {
                                    w.file.is_some()
                                }
                            }
                            _ => true,
                        };
                        if w.step == STEPS - 2 {
                            if ui.add_enabled(ready, primary(t!("Create"), accent)).clicked() {
                                create = true;
                            }
                        } else if w.step < STEPS - 1 {
                            if ui.add_enabled(ready, primary(t!("Next"), accent)).clicked() {
                                next = true;
                            }
                        } else if ui.button(t!("Close")).clicked() {
                            w.open = false;
                        }
                    });
                });
            },
        );
        if !open {
            self.wizard.open = false;
        }
        if back {
            self.wizard.step -= 1;
        }
        if next {
            if self.wizard.step == 0 {
                self.wizard_defaults();
            }
            self.wizard.step += 1;
        }
        if create {
            self.wizard_create(ctx);
            self.wizard.step = STEPS - 1;
        }
        if pick.is_some() && self.wizard.pick.is_none() {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            let dir = library::designs_dir();
            std::thread::spawn(move || {
                let _ = tx.send(
                    rfd::FileDialog::new()
                        .set_directory(dir)
                        .add_filter(t!("Artwork"), &["svg", "svgz", "png", "jpg", "jpeg", "webp", "bmp", "gif"])
                        .pick_file(),
                );
                ctx.request_repaint();
            });
            self.wizard.pick = Some(rx);
        }
        match finish {
            Some(0) => self.save_design(),
            Some(1) => self.open_usb(),
            Some(2) => self.save_worksheet(),
            _ => {}
        }
    }
}
