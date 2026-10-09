//! The Test stitch-out window: a piece of what's on the hoop cut to an area
//! drawn on the hoop, optionally outlines only, previewed on the hoop and
//! ready to save or send; and the calibration sheet with its results, which
//! tune the defaults and the design check.

use std::hash::{Hash, Hasher};

use super::*;
use crate::testsew::{self, Area, Tuning};

#[derive(Default)]
pub(super) struct TestSew {
    pub open: bool,
    /// Cut the test to `area` instead of sewing everything.
    pub use_area: bool,
    /// Waiting for a rectangle to be dragged on the hoop.
    pub picking: bool,
    /// The area in hoop coordinates (0.1 mm, hoop center at the origin).
    pub area: Option<Area>,
    drag_from: Option<(f32, f32)>,
    /// Edges only, without fills, for lettering and artwork.
    pub outlines: bool,
    /// Don't dim the design to show the test piece on the hoop.
    hide_preview: bool,
    /// The test sheet answers being entered, before they are used.
    answers: Option<Tuning>,
    /// Showing the calibration sheet page instead of the test piece.
    sheet_page: bool,
    /// The last test piece in hoop coordinates with its threads, and a
    /// fingerprint of what it was made from.
    cache: Option<(u64, Result<(Pattern, Vec<Block>), String>)>,
}

impl TestSew {
    /// The test differs from sewing the whole design.
    fn partial(&self) -> bool {
        self.use_area || self.outlines
    }
}

/// One thread per color change segment of `p`, taken from the blocks, which
/// hold the colors the user picked in the Viewer.
fn segment_threads(p: &Pattern, blocks: &[Block]) -> Vec<formats::Thread> {
    let starts = std::iter::once(0).chain((0..p.cmds.len()).filter(|&i| p.cmds[i] == Cmd::ColorChange));
    starts
        .enumerate()
        .map(|(k, s)| {
            blocks.iter().find(|b| b.start == s).map(|b| b.thread.clone()).or_else(|| p.threads.get(k).cloned()).unwrap_or_else(|| formats::stock_thread(k))
        })
        .collect()
}

/// Janome thread numbers for writing a JEF.
fn jef_numbers(threads: &[formats::Thread]) -> Vec<i32> {
    let palette: Vec<(i32, formats::Thread)> = formats::jef_palette().collect();
    threads.iter().map(|t| palette.iter().find(|(_, p)| p == t).map_or_else(|| formats::nearest_jef(t.color), |(i, _)| *i)).collect()
}

/// The piece as a JEF, centered, because the machine places a design by its center.
fn piece_jef(p: &Pattern) -> Vec<u8> {
    let b = p.bounds();
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let points: Vec<(f32, f32)> = p.points.iter().map(|&(x, y)| (x - cx, y - cy)).collect();
    formats::write_jef(&points, &p.cmds, &jef_numbers(&p.threads))
}

fn mm(v: f32) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s} mm")
}

/// A short line of help under a control.
fn hint(ui: &mut egui::Ui, text: &str, muted: Color32) {
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(muted)).wrap());
}

impl HoopApp {
    /// The current design stitched with outlines instead of fills, in the
    /// same coordinates as the full design.
    fn outline_pattern(&self) -> Result<Pattern, String> {
        // Shapes without an edge get a running outline, so their shape and
        // place still show.
        let outline = |s: &mut Settings| {
            s.fill = FillKind::None;
            s.patch_fill = FillKind::None;
            s.underlay = false;
            if s.edge == EdgeKind::None {
                s.edge = EdgeKind::Running;
            }
        };
        match self.mode {
            Mode::Lettering => {
                let mut s = self.letter.clone();
                outline(&mut s);
                lettering::generate(&self.fonts[s.font.min(self.fonts.len() - 1)], &s)
            }
            Mode::Artwork => {
                let (Some(art), Some(s)) = (&self.artwork, &self.art) else { return Err("No artwork open.".into()) };
                let mut s = s.clone();
                outline(&mut s.style);
                for l in &mut s.layers {
                    if matches!(l.look, Look::Fill(_)) {
                        l.look = Look::Default;
                    }
                }
                for e in &mut s.shapes {
                    if matches!(e.look, Some(Look::Fill(_))) {
                        e.look = None;
                    }
                }
                artwork::generate(art, &s)
            }
            Mode::Viewer => Err("Saved designs don't record which stitches are edges.".into()),
        }
    }

    /// The design's stitches in hoop coordinates, one thread per segment.
    fn design_on_hoop(&self, d: &Loaded) -> Result<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>), String> {
        let (pattern, threads) = if self.test.outlines && self.mode != Mode::Viewer {
            let p = self.outline_pattern()?;
            let threads = segment_threads(&p, &[]);
            (p, threads)
        } else {
            (d.pattern.clone(), segment_threads(&d.pattern, &d.blocks))
        };
        // The full design's bounds, so the outlines land where the design is.
        let t = Transform::new(d.bounds, &self.setup.at);
        Ok((pattern.points.iter().map(|&p| t.apply(p)).collect(), pattern.cmds, threads))
    }

    /// The test piece in hoop coordinates.
    fn make_test_piece(&self) -> Result<Pattern, String> {
        let mut points: Vec<(f32, f32)> = Vec::new();
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut threads: Vec<formats::Thread> = Vec::new();
        let mut add = |pts: Vec<(f32, f32)>, cs: Vec<Cmd>, ts: Vec<formats::Thread>| {
            if pts.is_empty() {
                return;
            }
            // Each part is its own run of threads; cropping drops the unused ones.
            if !threads.is_empty() {
                points.push(*points.last().unwrap());
                cmds.push(Cmd::ColorChange);
            }
            threads.extend(ts);
            points.push(pts[0]);
            cmds.push(Cmd::Jump);
            points.extend(pts.into_iter().skip(1));
            cmds.extend(cs.into_iter().skip(1));
        };
        for p in &self.pinned {
            add(p.pattern.points.clone(), p.pattern.cmds.clone(), segment_threads(&p.pattern, &p.blocks));
        }
        if let Some(d) = &self.design {
            let (pts, cs, ts) = self.design_on_hoop(d)?;
            add(pts, cs, ts);
        }
        if points.is_empty() {
            return Err("Open or make a design first, or sew the calibration sheet below.".into());
        }
        let area = match (self.test.use_area, self.test.area) {
            (true, Some(a)) => a,
            (true, None) => return Err("Draw a box on the hoop around the part to test.".into()),
            (false, _) => (f32::MIN, f32::MIN, f32::MAX, f32::MAX),
        };
        testsew::crop(&points, &cmds, &threads, area).ok_or_else(|| "No stitches lie fully inside the box. Draw it over the part to test.".into())
    }

    /// Makes the test piece again when something it depends on changed.
    pub(super) fn refresh_test_piece(&mut self) {
        if !self.test.open {
            self.test.cache = None;
            return;
        }
        let mut h = std::hash::DefaultHasher::new();
        format!("{:?}{:?}{}{:?}{}", self.test.area, self.test.use_area, self.test.outlines, self.setup.at, self.mode as u8).hash(&mut h);
        for part in self.pinned.iter().map(|p| &p.pattern).chain(self.design.as_ref().map(|d| &d.pattern)) {
            for &(x, y) in &part.points {
                (x.to_bits(), y.to_bits()).hash(&mut h);
            }
        }
        if let Some(d) = &self.design {
            for b in &d.blocks {
                b.thread.color.hash(&mut h);
            }
        }
        let key = h.finish();
        if self.test.cache.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let piece = self.make_test_piece().map(|p| {
            let blocks = p.blocks();
            (p, blocks)
        });
        self.test.cache = Some((key, piece));
    }

    /// The box around the picked artwork shapes, 2 mm larger on each side.
    fn picked_area(&self) -> Option<Area> {
        let m = self.shape_map()?;
        let (art, s) = (self.artwork.as_ref()?, self.art.as_ref()?);
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for e in self.sel.iter().filter_map(|&i| s.shapes.get(i)) {
            for (c, _) in e.lines(art) {
                for p in c {
                    let w = m.to_world(p);
                    b = (b.0.min(w.0), b.1.min(w.1), b.2.max(w.0), b.3.max(w.1));
                }
            }
        }
        (b.0 <= b.2).then_some((b.0 - 20.0, b.1 - 20.0, b.2 + 20.0, b.3 + 20.0))
    }

    /// Opens the calibration sheet in the Viewer.
    fn open_test_sheet(&mut self) {
        match testsew::sheet(&self.fonts[0]) {
            Ok(pattern) => {
                self.set_mode(Mode::Viewer);
                let blocks = pattern.blocks();
                let bounds = pattern.bounds();
                self.design = Some(Loaded { name: "test-sheet".into(), pattern, blocks, bounds });
                self.change(|s| {
                    s.hoop = 0;
                    s.at = Placement::default();
                });
                self.shown = self.total();
                self.playing = false;
                self.fit_pending = true;
                self.error = None;
                self.test.use_area = false;
                self.test.outlines = false;
                self.toast("Test sheet loaded. Save or send it, sew it, then enter what looked best.".into());
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Takes the entered sheet results into use: remembered, put into the
    /// lettering and artwork settings, and used by the design check.
    fn use_tuning(&mut self, t: Tuning) {
        self.tuning = t;
        self.prefs.store_tuning(&t);
        t.apply(&mut self.letter);
        if let Some(a) = &mut self.art {
            t.apply(&mut a.style);
        }
        self.store();
        self.check = None;
        self.toast(if t.is_empty() { "Test sheet results cleared. General rules apply again.".into() } else { "Using your test sheet results.".into() });
    }

    /// Dragging a rectangle on the hoop while the area is being picked.
    /// Returns true while picking, so the drag doesn't move anything else.
    pub(super) fn test_area_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        if !self.test.picking {
            return false;
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let at = response.interact_pointer_pos().map(|p| frame.to_world(p));
        if response.drag_started() {
            self.test.drag_from = at;
        }
        if let (Some(a), Some(b)) = (self.test.drag_from, at) {
            if response.dragged() || response.drag_stopped() {
                self.test.area = Some((a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)));
            }
        }
        if response.drag_stopped() {
            self.test.drag_from = None;
            self.test.picking = false;
            // A click or a tiny drag isn't an area; keep waiting for a real one.
            if self.test.area.is_some_and(|a| a.2 - a.0 < 20.0 || a.3 - a.1 < 20.0) {
                self.test.area = None;
                self.test.picking = true;
            }
        }
        true
    }

    /// While the Test window is open: the design dimmed with the test piece
    /// on top, so what's shown is what gets sewn, and the test area.
    pub(super) fn test_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        if !self.test.open {
            return;
        }
        let accent = self.accent();
        if self.test.partial() && !self.test.hide_preview && !self.test.picking {
            if let Some((_, Ok((piece, blocks)))) = &self.test.cache {
                let [r, g, b] = self.fabric;
                painter.rect_filled(frame.rect, 0, Color32::from_rgba_unmultiplied(r, g, b, 200));
                let identity = Transform::new((0.0, 0.0, 0.0, 0.0), &Placement::default());
                let style = DesignStyle { textured: self.textured, show_jumps: false, focus: None, fabric: self.fabric };
                canvas::draw_design(frame, piece, blocks, &identity, piece.points.len(), &style);
            }
        }
        if self.test.picking {
            let tip = "Drag a box around the part to test · Esc to stop";
            let galley = painter.layout_no_wrap(tip.into(), egui::FontId::proportional(14.0), Color32::WHITE);
            let pos = frame.rect.center_top() + vec2(-galley.size().x / 2.0, 16.0);
            painter.rect_filled(egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0)), 8, Color32::from_black_alpha(190));
            painter.galley(pos, galley, Color32::WHITE);
        }
        if !self.test.use_area {
            return;
        }
        let Some(a) = self.test.area else { return };
        let r = egui::Rect::from_two_pos(frame.to_screen(a.0, a.1), frame.to_screen(a.2, a.3));
        if self.test.picking {
            painter.rect_filled(r, 0, accent.gamma_multiply(0.12));
        }
        painter.rect_stroke(r, 0, egui::Stroke::new(1.5, accent), egui::StrokeKind::Middle);
        let text = format!("Test area  {:.0} × {:.0} mm", (a.2 - a.0) / 10.0, (a.3 - a.1) / 10.0);
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), contrast(accent));
        let pos = r.left_top() - vec2(0.0, galley.size().y + 6.0);
        painter.rect_filled(egui::Rect::from_min_size(pos - vec2(0.0, 2.0), galley.size() + vec2(10.0, 4.0)), 3, accent);
        painter.galley(pos + vec2(5.0, 0.0), galley, contrast(accent));
    }

    /// The Calibration sheet page: open the sheet, then enter its results.
    fn sheet_page(&mut self, ui: &mut egui::Ui, answers: &mut Tuning, sheet: &mut bool, use_answers: &mut bool, clear: &mut bool) {
        let accent = self.accent();
        let muted = self.muted();
        hint(ui, "Small samples of each stitch type on one 123 × 111 mm sheet for the SQ14b hoop: about 10 minutes in 2 threads. Sew it once for each kind of fabric.", muted);
        ui.add_space(6.0);
        *sheet = ui.add(primary("Open test sheet", accent)).clicked();
        ui.add_space(10.0);
        card(ui, "After sewing it: what looked best?", accent, |ui| {
            hint(ui, "Samples run left to right on each row. Hoop uses your answers for new designs and in the Check list.", muted);
            ui.add_space(6.0);
            egui::Grid::new("sheet-answers").num_columns(3).spacing(vec2(8.0, 8.0)).show(ui, |ui| {
                for (row, answer) in testsew::SHEET_ROWS.iter().zip(answers.answers()) {
                    ui.label(RichText::new(row.letter).strong().monospace().size(15.0));
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        ui.add(egui::Label::new(RichText::new(row.question).size(12.5)).wrap());
                        ui.label(RichText::new(row.holds).size(11.0).color(muted));
                    });
                    let shown = answer.map_or("Not sewn".to_string(), mm);
                    egui::ComboBox::from_id_salt(("sheet-answer", row.letter)).selected_text(shown).width(82.0).show_ui(ui, |ui| {
                        ui.selectable_value(answer, None, "Not sewn");
                        for &v in row.samples {
                            ui.selectable_value(answer, Some(v), mm(v));
                        }
                    });
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                *use_answers = ui.add_enabled(*answers != self.tuning, primary("Use these results", accent)).clicked();
                *clear = ui.add_enabled(!self.tuning.is_empty(), egui::Button::new("Forget results")).clicked();
            });
        });
        let using = if self.tuning.is_empty() {
            "Not sewn yet, so Hoop uses general rules.".to_string()
        } else {
            let t = self.tuning;
            let parts: Vec<String> = [
                t.satin_max.map(|v| format!("satin up to {}", mm(v))),
                t.fill_min.map(|v| format!("fills from {}", mm(v))),
                t.rows.map(|v| format!("rows {}", mm(v))),
                t.pull.map(|v| format!("pull {}", mm(v))),
                t.letters_min.map(|v| format!("letters from {}", mm(v))),
            ]
            .into_iter()
            .flatten()
            .collect();
            format!("In use: {}.", parts.join(", "))
        };
        ui.add(egui::Label::new(RichText::new(using).size(12.0).color(if self.tuning.is_empty() { muted } else { OK })).wrap());
    }

    pub(super) fn test_window(&mut self, ctx: &egui::Context) {
        if !self.test.open {
            self.test.picking = false;
            return;
        }
        let accent = self.accent();
        let muted = self.muted();
        let can_outline = self.mode != Mode::Viewer && self.design.is_some();
        let can_pick = self.mode == Mode::Artwork && !self.sel.is_empty() && self.shape_map().is_some();
        let speed = SPEEDS[self.speed] as f32;
        // What the piece is, read before the window borrows self.
        let summary: Result<(String, String), String> = match self.test.cache.as_ref().map(|c| &c.1) {
            Some(Ok((p, _))) => {
                let stitches = p.count(Cmd::Stitch);
                let threads = p.threads.len();
                let minutes = (stitches as f32 / speed + threads.saturating_sub(1) as f32).max(1.0);
                let b = p.bounds();
                Ok((
                    format!("{stitches} stitches · {threads} thread{} · about {minutes:.0} min", if threads == 1 { "" } else { "s" }),
                    format!("{:.1} × {:.1} mm, centered on the hoop when sent", (b.2 - b.0) / 10.0, (b.3 - b.1) / 10.0),
                ))
            }
            Some(Err(e)) => Err(e.clone()),
            None => Err(String::new()),
        };
        let nothing = self.design.is_none() && self.pinned.is_empty();
        let mut answers = self.test.answers.unwrap_or(self.tuning);
        let mut open = true;
        let (mut save, mut send, mut sheet, mut use_answers, mut clear) = (false, false, false, false, false);
        egui::Window::new("Test stitch-out")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::RIGHT_TOP)
            .default_pos(egui::pos2(ctx.content_rect().right() - 16.0, self.canvas_rect.top() + 12.0))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                shell::tab_row(ui, &mut self.test.sheet_page, &[(false, "Test a part"), (true, "Calibration sheet")], accent);
                ui.add_space(8.0);
                if self.test.sheet_page {
                    self.sheet_page(ui, &mut answers, &mut sheet, &mut use_answers, &mut clear);
                    return;
                }
                hint(ui, "Sew a small part on scrap before the whole design, with the same fabric, stabilizer and thread.", muted);
                ui.add_space(8.0);

                card(ui, "1  What to sew", accent, |ui| {
                    let before = self.test.use_area;
                    segmented(ui, &mut self.test.use_area, &[(false, "Whole design"), (true, "Part of it")], accent);
                    if self.test.use_area && !before && self.test.area.is_none() {
                        self.test.picking = true;
                    }
                    if self.test.use_area {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let label = if self.test.picking { "Drag on the hoop…" } else if self.test.area.is_some() { "Draw again" } else { "Draw on hoop" };
                            if ui.add_enabled(!self.test.picking, egui::Button::new(label)).on_hover_text("Drag a box on the hoop").clicked() {
                                self.test.picking = true;
                            }
                            if self.mode == Mode::Artwork {
                                let r = ui.add_enabled(can_pick, egui::Button::new("Around picked shapes"));
                                let r = r.on_disabled_hover_text("Pick shapes on the hoop or in Objects first");
                                if r.clicked() {
                                    self.test.area = self.picked_area();
                                    self.test.picking = false;
                                }
                            }
                        });
                        let what = match self.test.area {
                            Some(a) => format!("Box {:.0} × {:.0} mm. ", (a.2 - a.0) / 10.0, (a.3 - a.1) / 10.0),
                            None => String::new(),
                        };
                        hint(ui, &format!("{what}Box a tight corner, the smallest letters or any detail you're unsure of. Only stitches fully inside are sewn; cut runs get tie stitches."), muted);
                    }
                    ui.add_space(6.0);
                    let r = ui.add_enabled(can_outline, egui::Checkbox::new(&mut self.test.outlines, "Outlines only, no fills"));
                    if !can_outline {
                        r.on_disabled_hover_text("Saved files don't record which stitches are edges. Make the design in Lettering or Artwork to test outlines.");
                    }
                    hint(
                        ui,
                        if can_outline {
                            "Edges where the fills go: checks shape, size and placement in a fraction of the time."
                        } else {
                            "Outlines need a design made in Lettering or Artwork."
                        },
                        muted,
                    );
                });

                card(ui, "2  Sew it", accent, |ui| {
                    match &summary {
                        Ok((line, size)) => {
                            ui.label(RichText::new(line).strong());
                            hint(ui, size, muted);
                            if self.test.partial() {
                                ui.checkbox(&mut self.test.hide_preview, "Show the full design");
                            } else {
                                hint(ui, "That's the whole design. Pick a part or outlines above for a shorter test.", muted);
                            }
                        }
                        Err(e) if !e.is_empty() => {
                            ui.label(RichText::new(e).color(if nothing { muted } else { WARN }));
                        }
                        Err(_) => {}
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        save = ui.add_enabled(summary.is_ok(), primary("Save test…", accent)).clicked();
                        send = ui.add_enabled(summary.is_ok(), egui::Button::new("Send to USB…")).clicked();
                    });
                });
            });
        self.test.answers = Some(answers);
        if !open {
            self.test.open = false;
            self.test.picking = false;
            self.test.answers = None;
        }
        if sheet {
            self.open_test_sheet();
        }
        if use_answers {
            self.use_tuning(answers);
        }
        if clear {
            self.test.answers = None;
            self.use_tuning(Tuning::default());
        }
        if save || send {
            let Some((_, Ok((p, _)))) = &self.test.cache else { return };
            let bytes = piece_jef(p);
            let stem = format!("{}-test", self.design_stem());
            if save {
                self.ask_save("Save test as JEF", format!("{stem}.jef"), "Janome JEF", "jef", bytes);
            } else {
                self.show_usb(&stem, Some(bytes));
            }
        }
    }
}
