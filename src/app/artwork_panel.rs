//! The Artwork tab: taking in SVGs and pictures, tracing, rebuilding the
//! stitches, and the Source, Style and Threads pages.

use super::*;

/// The example artwork: the square Omarchy mark, glyph U+E900 of
/// Omarchy's icon font.
const EXAMPLE_FONT: &str = "/usr/share/omarchy/default/fonts/omarchy/omarchy.ttf";
const EXAMPLE_GLYPH: char = '\u{e900}';

/// A font's glyph as an SVG document, or None when the font or glyph is
/// missing.
fn glyph_svg(font: &[u8], c: char) -> Option<String> {
    struct Path(String);
    impl ttf_parser::OutlineBuilder for Path {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0 += &format!("M{x} {y}");
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0 += &format!("L{x} {y}");
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.0 += &format!("Q{x1} {y1} {x} {y}");
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.0 += &format!("C{x1} {y1} {x2} {y2} {x} {y}");
        }
        fn close(&mut self) {
            self.0 += "Z";
        }
    }
    let face = ttf_parser::Face::parse(font, 0).ok()?;
    let mut path = Path(String::new());
    let b = face.outline_glyph(face.glyph_index(c)?, &mut path)?;
    // Fonts count y upwards; SVG downwards.
    let (w, h) = (b.x_max - b.x_min, b.y_max - b.y_min);
    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><path fill="#000" fill-rule="nonzero" transform="matrix(1 0 0 -1 {} {})" d="{}"/></svg>"##,
        -b.x_min, b.y_max, path.0
    ))
}

impl HoopApp {
    /// Shows freshly read artwork in the Artwork tab, or the reason it failed.
    pub(super) fn use_artwork(&mut self, result: Result<artwork::Artwork, String>) -> bool {
        self.set_mode(Mode::Artwork);
        // New artwork isn't the project that was open; opening a project sets it again.
        self.project_path = None;
        match result {
            Ok(art) => {
                let mut s = ArtSettings::for_artwork(&art);
                self.tuning.apply(&mut s.style);
                s.style.column_max_mm = self.column_max();
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

    /// The first time Artwork opens empty, the Omarchy logo is there as an
    /// example to try things on. It counts as nothing changed, so leaving
    /// it asks nothing.
    pub(super) fn artwork_example(&mut self) {
        if self.mode != Mode::Artwork || self.artwork.is_some() || std::mem::replace(&mut self.example_shown, true) {
            return;
        }
        let Some(svg) = std::fs::read(EXAMPLE_FONT).ok().and_then(|f| glyph_svg(&f, EXAMPLE_GLYPH)) else { return };
        self.picture = None;
        if self.use_artwork(artwork::parse(t!("Omarchy logo (example)"), svg.as_bytes())) {
            if let Some(a) = &mut self.artwork {
                a.name = t!("Omarchy logo (example)").into();
            }
            // A patch-sized mark, well inside the smallest common hoop.
            if let Some(s) = &mut self.art {
                s.width_mm = 90.0;
            }
            self.clean = self.snapshot();
            self.committed = self.clean.clone();
        }
    }

    /// Traces the picture again, keeping the stitch settings unless `fresh`.
    pub(super) fn retrace(&mut self, fresh: bool) {
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

    pub(super) fn paste_svg(&mut self, text: &str) -> bool {
        self.picture = None;
        self.use_artwork(artwork::parse(t!("Pasted SVG"), text.trim().as_bytes()))
    }

    pub(super) fn regenerate_art(&mut self) {
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
                self.title_base = t!("{name} — Hoop", name = art.name);
            }
            Err(e) => {
                self.design = None;
                self.error = Some(e);
            }
        }
    }

    pub(super) fn artwork_panel(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect, tab: Tab) {
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
                        ui.label(
                            RichText::new(format!("{} · {}", tn!(shapes, "{n} shape", "{n} shapes"), tn!(a.layers.len(), "{n} color", "{n} colors")))
                                .color(muted),
                        );
                        if a.skipped_images > 0 {
                            ui.label(
                                RichText::new(tn!(
                                    a.skipped_images,
                                    "Skipped {n} embedded picture. Only vector shapes can be stitched.",
                                    "Skipped {n} embedded pictures. Only vector shapes can be stitched."
                                ))
                                .size(12.0)
                                .color(muted),
                            );
                        }
                    }
                    None => {
                        ui.label(
                            RichText::new(t!("Open an SVG or a picture (PNG, JPG), or drop one on the hoop. Logos and clip art with flat colors work best."))
                                .color(muted),
                        );
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
                    if ui
                        .add(egui::Button::new(t!("Paste SVG")).selected(self.paste_open).min_size(vec2(w, 30.0)))
                        .on_hover_text(t!("Paste SVG code, or press Ctrl+V anywhere with SVG code copied"))
                        .clicked()
                    {
                        self.paste_open = !self.paste_open;
                    }
                    if ui.add_enabled(self.design.is_some(), egui::Button::new(t!("Save…")).min_size(vec2(w, 30.0))).on_hover_text("Ctrl+S").clicked() {
                        self.save_design();
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
                ui.label(
                    RichText::new(t!("Pictures are reduced to a few flat colors and outlined. Logos and clip art work best; photos don't stitch well."))
                        .size(12.0)
                        .color(muted),
                );
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
        if tab == Tab::Source {
            self.draw_card(ui);
        }
        let hoop = self.hoop_size();
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
        if tab == Tab::Source {
            card(ui, t!("Size"), accent, |ui| {
                // Width and height stay in proportion; either one sets the size.
                let a = art.aspect().max(1e-3);
                slider_row(ui, t!("Width"), &mut s.width_mm, 10.0..=360.0, " mm", 1);
                let mut h = s.width_mm * a;
                if slider_row(ui, t!("Height"), &mut h, 10.0 * a..=360.0 * a, " mm", 1).changed() {
                    s.width_mm = (h / a).clamp(10.0, 360.0);
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("Keeps its proportions")).size(12.0).color(muted));
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
            let fabric = self.fabric;
            // Stitches and thread length per thread color, from the design.
            let usage: Vec<([u8; 3], usize, f32)> =
                self.design.as_ref().map_or(Vec::new(), |d| d.blocks.iter().map(|b| (b.thread.color, b.stitches, d.pattern.thread_meters(b))).collect());
            card(ui, t!("Threads"), accent, |ui| {
                // Colors no shape uses any more are left out.
                let used_layers: Vec<bool> = (0..art.layers.len()).map(|l| s.shapes.iter().any(|e| e.layer == l && !e.hidden)).collect();
                let mut groups = artwork::thread_groups(art, s);
                groups.retain(|g| g.1.iter().any(|&i| used_layers[i]));
                for (g, (thread, members)) in groups.iter().enumerate() {
                    let mut on = members.iter().all(|&i| s.layers[i].enabled);
                    let mut t = *thread;
                    ui.horizontal(|ui| {
                        let tip = if s.off_leaves_fabric {
                            t!("Stitch these colors. Off: not sewn, and the fabric shows where they are")
                        } else {
                            t!("Stitch these colors. Off: not sewn, as if they weren't in the drawing")
                        };
                        if ui.checkbox(&mut on, "").on_hover_text(tip).changed() {
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

                        // A color the same as the fabric is usually a hole in
                        // the drawing (white details on white cloth): offer to
                        // leave the fabric showing instead of sewing it.
                        let fabric_like = members.iter().all(|&i| crate::threads::distance(art.layers[i].color, fabric) < 8.0);
                        let is_hole = members.iter().all(|&i| s.layers[i].look == Look::Hole);
                        if fabric_like
                            && on
                            && !is_hole
                            && !members.iter().any(|&i| art.layers[i].background)
                            && ui
                                .small_button(t!("Leave fabric?"))
                                .on_hover_text(t!(
                                    "This color matches the fabric. Leave the fabric showing here instead of sewing it, and don't sew shapes below through it."
                                ))
                                .clicked()
                        {
                            for &i in members {
                                s.layers[i].look = Look::Hole;
                            }
                        }
                        if members.iter().any(|&i| art.layers[i].background) {
                            ui.label(RichText::new(t!("background")).size(11.0).color(muted));
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
                            Look::Hole => t!("Leave fabric").into(),
                        };
                        egui::ComboBox::from_id_salt(("look", g)).selected_text(name).width(78.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut look, Look::Default, t!("Auto")).on_hover_text(t!("Use the Fill card below"));
                            for k in [FillKind::Tatami, FillKind::Satin, FillKind::Wave, FillKind::Open] {
                                ui.selectable_value(&mut look, Look::Fill(k), crate::i18n::tr(crate::lettering::fill_name(k)).to_string());
                            }
                            ui.selectable_value(&mut look, Look::Outline, t!("Outline")).on_hover_text(t!("Only the shape's outline"));
                            ui.selectable_value(&mut look, Look::Applique, t!("Appliqué"))
                                .on_hover_text(t!("Sew a piece of fabric on: placement line, stop, tack-down, stop, satin cover"));
                            ui.selectable_value(&mut look, Look::Hole, t!("Leave fabric"))
                                .on_hover_text(t!("Not sewn; the fabric shows here, and nothing below is sewn through it"));
                        });
                        for &i in members {
                            s.layers[i].look = look;
                        }
                        let color = crate::formats::jef_thread(*thread, 0).color;
                        let (st, m) = usage.iter().filter(|u| u.0 == color).fold((0, 0.0), |a, u| (a.0 + u.1, a.1 + u.2));
                        if st > 0 {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(t!("{stitches} st · {v0:.1} m", stitches = st, v0 = m)).size(11.5).color(muted));
                            });
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
        // With shapes picked, the style cards edit just those shapes;
        // otherwise the drawing's style, which every other shape follows.
        let picked: Vec<usize> = self.sel.iter().copied().filter(|&i| i < s.shapes.len()).collect();
        if picked.is_empty() {
            card(ui, t!("Quick styles"), accent, |ui| {
                if let Some(i) = preset_chips(ui, &artwork::PRESETS.map(|p| (p.0, p.1))) {
                    (artwork::PRESETS[i].2)(s);
                }
            });
            if let Some(tip) = fabric_card(ui, &mut s.style, muted) {
                self.status = Some((tip.to_string(), std::time::Instant::now()));
            }
            fill_card(ui, &mut s.style, muted, accent, false);
        } else {
            let own = picked.iter().filter(|&&i| s.shapes[i].style.is_some()).count();
            let mut shared = false;
            card(ui, &tn!(picked.len(), "{n} picked shape", "{n} picked shapes"), accent, |ui| {
                let note = if own == 0 {
                    t!("Changes below give the picked shapes their own style. The rest keep the drawing's style.")
                } else {
                    t!("The picked shapes have their own style. Deselect (Esc) to change the drawing's style.")
                };
                ui.label(RichText::new(note).size(12.0).color(muted));
                if own > 0 && ui.button(t!("Use the drawing's style")).on_hover_text(t!("Drop the picked shapes' own style")).clicked() {
                    shared = true;
                }
            });
            if shared {
                for &i in &picked {
                    s.shapes[i].style = None;
                }
            }
            let first = picked[0];
            let mut st = s.shapes[first].style.clone().unwrap_or_else(|| s.style.clone());
            let before = st.clone();
            if let Some(tip) = fabric_card(ui, &mut st, muted) {
                self.status = Some((tip.to_string(), std::time::Instant::now()));
            }
            fill_card(ui, &mut st, muted, accent, false);
            edge_card(ui, &mut st, accent, t!("Outline on fills"), false);
            if st != before {
                for &i in &picked {
                    s.shapes[i].style = Some(st.clone());
                }
            }
            ui.label(RichText::new(t!("Clean-up and line settings apply to the whole drawing.")).size(12.0).color(muted));
        }
        card_folded(ui, t!("Clean-up"), accent, |ui| {
            ui.checkbox(&mut s.knockout, t!("Cut hidden parts")).on_hover_text(t!("Leave areas covered by shapes on top unstitched, so layers don't pile up"));
            ui.checkbox(&mut s.off_leaves_fabric, t!("Colors switched off leave fabric"))
                .on_hover_text(t!("A color unticked in Threads isn't sewn and leaves the fabric showing, so shapes below aren't sewn through it. Off: it is as if the color weren't there."));
            slider_row(ui, t!("Detail"), &mut s.min_detail_mm, 0.0..=5.0, " mm", 1).on_hover_text(t!("Shapes smaller than this many mm across are left out"));
        });
        if picked.is_empty() {
            edge_card(ui, &mut s.style, accent, t!("Outline on fills"), false);
        }
        card(ui, t!("Lines"), accent, |ui| {
            ui.label(RichText::new(t!("How strokes in the SVG are stitched")).size(12.0).color(muted));
            segmented(
                ui,
                &mut s.stroke,
                &[(EdgeKind::Satin, t!("Satin")), (EdgeKind::Running, t!("Run")), (EdgeKind::Triple, t!("Triple")), (EdgeKind::None, t!("None"))],
                accent,
            );
            if s.stroke == EdgeKind::Satin {
                ui.add_space(4.0);
                slider_row(ui, t!("Min"), &mut s.stroke_min_mm, 0.5..=4.0, " mm", 1)
                    .on_hover_text(t!("Thinnest satin line; thinner strokes are widened to this"));
            }
        });
        if ui.button(t!("Reset settings")).on_hover_text(t!("Also puts every shape back as it was in the drawing")).clicked() {
            self.art = Some(ArtSettings::for_artwork(art));
            self.sel.clear();
        }
        let _ = (canvas_rect, muted);
        ui.add_space(10.0);
    }
}
