//! The Test stitch-out window: a piece of what's on the hoop cut to an area
//! drawn on the hoop, optionally outlines only, ready to save or send; and
//! the calibration sheet.

use std::hash::{Hash, Hasher};

use super::*;
use crate::testsew::{self, Area};

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
    /// The last test piece and a fingerprint of what it was made from.
    cache: Option<(u64, Result<Pattern, String>)>,
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
pub(super) fn jef_numbers(threads: &[formats::Thread]) -> Vec<i32> {
    let palette: Vec<(i32, formats::Thread)> = formats::jef_palette().collect();
    threads.iter().map(|t| palette.iter().find(|(_, p)| p == t).map_or_else(|| formats::nearest_jef(t.color), |(i, _)| *i)).collect()
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
            return Err("Open or make a design first.".into());
        }
        let area = match (self.test.use_area, self.test.area) {
            (true, Some(a)) => a,
            (true, None) => return Err("Drag a rectangle on the hoop around the part to test.".into()),
            (false, _) => (f32::MIN, f32::MIN, f32::MAX, f32::MAX),
        };
        let mut piece = testsew::crop(&points, &cmds, &threads, area).ok_or("No stitches inside the area. Draw it over the part to test.")?;
        // The machine places a design by its center, so center the piece.
        let b = piece.bounds();
        let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
        for p in &mut piece.points {
            *p = (p.0 - cx, p.1 - cy);
        }
        Ok(piece)
    }

    /// The test piece for the current settings, made again only when
    /// something it depends on changed.
    fn test_piece(&mut self) -> Result<Pattern, String> {
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
        if let Some((k, piece)) = &self.test.cache {
            if *k == key {
                return piece.clone();
            }
        }
        let piece = self.make_test_piece();
        self.test.cache = Some((key, piece.clone()));
        piece
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
                self.toast("Test sheet: rows A to E are explained in the Test window.".into());
            }
            Err(e) => self.error = Some(e),
        }
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
            // A click or a tiny drag isn't an area.
            if self.test.area.is_some_and(|a| a.2 - a.0 < 20.0 || a.3 - a.1 < 20.0) {
                self.test.area = None;
                self.test.picking = true;
            }
        }
        true
    }

    /// The test area on the hoop while the Test window is open.
    pub(super) fn test_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        if !self.test.open || !self.test.use_area {
            return;
        }
        let Some(a) = self.test.area else { return };
        let accent = self.accent();
        let r = egui::Rect::from_two_pos(frame.to_screen(a.0, a.1), frame.to_screen(a.2, a.3));
        painter.rect_filled(r, 0, accent.gamma_multiply(0.12));
        painter.rect_stroke(r, 0, egui::Stroke::new(1.5, accent), egui::StrokeKind::Middle);
        let text = format!("Test area  {:.0} × {:.0} mm", (a.2 - a.0) / 10.0, (a.3 - a.1) / 10.0);
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), contrast(accent));
        let pos = r.left_top() - vec2(0.0, galley.size().y + 6.0);
        painter.rect_filled(egui::Rect::from_min_size(pos - vec2(0.0, 2.0), galley.size() + vec2(10.0, 4.0)), 3, accent);
        painter.galley(pos + vec2(5.0, 0.0), galley, contrast(accent));
    }

    pub(super) fn test_window(&mut self, ctx: &egui::Context) {
        if !self.test.open {
            self.test.picking = false;
            return;
        }
        let accent = self.accent();
        let muted = self.muted();
        let piece = self.test_piece();
        let mut open = true;
        let (mut save, mut send, mut sheet) = (false, false, false);
        let can_outline = self.mode != Mode::Viewer && self.design.is_some();
        egui::Window::new("Test stitch-out")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(egui::pos2(ctx.content_rect().right() - 400.0, self.canvas_rect.top() + 16.0))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new("Sew a small part on scrap fabric first to check edges, small details and density.").size(12.0).color(muted));
                ui.add_space(6.0);
                card(ui, "What to sew", accent, |ui| {
                    ui.radio_value(&mut self.test.use_area, false, "Everything on the hoop");
                    if ui.radio_value(&mut self.test.use_area, true, "Only an area of the hoop").clicked() && self.test.area.is_none() {
                        self.test.picking = true;
                    }
                    if self.test.use_area {
                        ui.horizontal(|ui| {
                            ui.add_space(22.0);
                            let label = if self.test.picking { "Drag on the hoop…" } else if self.test.area.is_some() { "Draw again" } else { "Draw area" };
                            if ui.add_enabled(!self.test.picking, egui::Button::new(label)).clicked() {
                                self.test.picking = true;
                            }
                            if let Some(a) = self.test.area {
                                ui.label(RichText::new(format!("{:.0} × {:.0} mm", (a.2 - a.0) / 10.0, (a.3 - a.1) / 10.0)).monospace().color(muted));
                            }
                        });
                        ui.label(RichText::new("Stitches cut by the area's border are left out; parts inside are joined with tied-off jumps.").size(11.5).color(muted));
                    }
                    ui.add_space(4.0);
                    let r = ui.add_enabled(can_outline, egui::Checkbox::new(&mut self.test.outlines, "Outlines only, no fills"));
                    if can_outline {
                        r.on_hover_text("Sews the edges where the fills would go, to check shape and placement. Shapes without an edge get a running outline.");
                    } else {
                        r.on_disabled_hover_text("Saved files don't record which stitches are edges. Make the design in Lettering or Artwork to test outlines.");
                    }
                });
                match &piece {
                    Ok(p) => {
                        let stitches = p.count(Cmd::Stitch);
                        let minutes = stitches as f32 / SPEEDS[self.speed] as f32 + p.threads.len().saturating_sub(1) as f32;
                        let b = p.bounds();
                        ui.label(format!(
                            "{stitches} stitches · {} thread{} · {:.0} × {:.0} mm · about {:.0} min",
                            p.threads.len(),
                            if p.threads.len() == 1 { "" } else { "s" },
                            (b.2 - b.0) / 10.0,
                            (b.3 - b.1) / 10.0,
                            minutes.max(1.0)
                        ));
                    }
                    Err(e) => {
                        ui.label(RichText::new(e).color(if self.design.is_none() && self.pinned.is_empty() { muted } else { BAD }));
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    save = ui.add_enabled(piece.is_ok(), primary("Save test…", accent)).clicked();
                    send = ui.add_enabled(piece.is_ok(), egui::Button::new("Send to USB…")).clicked();
                });
                ui.add_space(10.0);
                card(ui, "Calibration sheet", accent, |ui| {
                    ui.label(RichText::new("One sheet of small samples, 123 × 111 mm for the SQ14b hoop, about 10 minutes in 2 threads. Sew it once to learn what your machine, thread and fabric like.").size(12.0).color(muted));
                    ui.add_space(4.0);
                    sheet = ui.button("Open test sheet").clicked();
                    ui.add_space(4.0);
                    egui::CollapsingHeader::new("What each row tests").id_salt("sheet-rows").show(ui, |ui| {
                        for (row, what) in testsew::SHEET_ROWS {
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new(row).strong().monospace());
                                ui.add(egui::Label::new(RichText::new(what).size(12.0)).wrap());
                            });
                        }
                    });
                });
            });
        if !open {
            self.test.open = false;
            self.test.picking = false;
        }
        if sheet {
            self.open_test_sheet();
        }
        let Ok(p) = piece else { return };
        if save || send {
            let bytes = formats::write_jef(&p.points, &p.cmds, &jef_numbers(&p.threads));
            let stem = format!("{}-test", self.design_stem());
            if save {
                self.ask_save("Save test as JEF", format!("{stem}.jef"), "Janome JEF", "jef", bytes);
            } else {
                self.show_usb(&stem, Some(bytes));
            }
        }
    }
}
