//! The Lettering tab: rebuilding the design from its settings and the
//! Text, Style and Patch pages.

use super::*;

impl HoopApp {
    pub(super) fn regenerate(&mut self) {
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
                self.title_base = t!("Lettering — Hoop").into();
            }
            Err(e) => {
                self.design = None;
                self.error = Some(e);
            }
        }
    }

    pub(super) fn lettering_panel(&mut self, ui: &mut egui::Ui, tab: Tab) {
        let muted = self.muted();
        let accent = self.accent();
        let mut tip_later = None;
        // Previews of the bundled and added fonts for the font list.
        let thumbs: Vec<Option<egui::TextureHandle>> = if tab == Tab::Text {
            let ctx = ui.ctx().clone();
            (0..self.fonts.len()).map(|i| self.font_thumb(&ctx, i)).collect()
        } else {
            Vec::new()
        };
        let mut more_fonts = false;
        let s = &mut self.letter;
        if tab == Tab::Text {
            card(ui, t!("Text"), accent, |ui| {
                ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(2).desired_width(f32::INFINITY).font(egui::TextStyle::Heading));
                let current = self.fonts.get(s.font).map_or("", |f| f.name.as_str()).to_string();
                let sizes = &self.fontwin.sizes;
                let text_color = ui.visuals().text_color();
                let list = egui::ComboBox::from_id_salt("font").selected_text(&current).width(ui.available_width()).height(420.0).show_ui(ui, |ui| {
                    let search = ui.add(egui::TextEdit::singleline(&mut self.font_filter).hint_text(t!("Search fonts")).desired_width(f32::INFINITY));
                    // Typing goes straight into the search when the list opens.
                    if ui.ctx().memory(|m| m.focused().is_none()) {
                        search.request_focus();
                    }
                    if ui.button(t!("Get more fonts from Google Fonts…")).clicked() {
                        more_fonts = true;
                        ui.close();
                    }
                    let filter = self.font_filter.clone();
                    let fonts = &self.fonts;
                    let ppp = ui.ctx().pixels_per_point();
                    let row = |ui: &mut egui::Ui, font: &mut usize, i: usize| {
                        let f = &fonts[i];
                        let mm = sizes.get(&f.name).copied().flatten();
                        let sel = *font == i;
                        let btn = match &thumbs[i] {
                            Some(tex) => egui::Button::selectable(sel, egui::Image::new((tex.id(), tex.size_vec2() / ppp)).tint(text_color)),
                            None => egui::Button::selectable(sel, f.name.as_str()),
                        };
                        let note = mm.map_or(String::new(), |mm| format!("{mm} mm"));
                        let note = RichText::new(note).size(11.0);
                        let note = if sel { note } else { note.color(muted) };
                        let r = ui.add(btn.right_text(note).min_size(egui::vec2(ui.available_width(), 0.0)));
                        let r = match mm {
                            Some(mm) => r.on_hover_text(format!("{}\n{}", f.name, t!("Stitches cleanly from about {mm} mm tall", mm = mm))),
                            None => r.on_hover_text(&f.name),
                        };
                        if r.clicked() {
                            *font = i;
                        }
                    };
                    let shown = |f: &lettering::FontSource| lettering::font_matches(&f.name, &filter);
                    let header = |ui: &mut egui::Ui, text: &str| {
                        ui.add_space(4.0);
                        ui.label(RichText::new(text).size(11.0).strong().color(muted));
                    };
                    for g in lettering::FontGroup::ALL {
                        let mut list: Vec<usize> = (0..fonts.len()).filter(|&i| fonts[i].group == Some(g) && shown(&fonts[i])).collect();
                        list.sort_by_key(|&i| fonts[i].name.to_lowercase());
                        if !list.is_empty() {
                            header(ui, g.name());
                            for i in list {
                                row(ui, &mut s.font, i);
                            }
                        }
                    }
                    let added: Vec<usize> = (0..fonts.len()).filter(|&i| fonts[i].downloaded() && shown(&fonts[i])).collect();
                    if !added.is_empty() {
                        header(ui, t!("Added from Google Fonts"));
                        for i in added {
                            row(ui, &mut s.font, i);
                        }
                    }
                    let system: Vec<usize> = (0..fonts.len()).filter(|&i| !fonts[i].featured && !fonts[i].downloaded() && shown(&fonts[i])).collect();
                    if !system.is_empty() {
                        header(ui, t!("Installed on this computer"));
                        for i in system {
                            row(ui, &mut s.font, i);
                        }
                    }
                    if self.font_scan.is_some() {
                        ui.label(RichText::new(t!("Looking for installed fonts…")).color(muted));
                    }
                    if !filter.trim().is_empty() {
                        if !fonts.iter().any(shown) {
                            ui.label(RichText::new(t!("No font here matches.")).color(muted));
                        }
                        if ui.button(t!("Search Google Fonts for “{q}”", q = filter.trim())).clicked() {
                            more_fonts = true;
                            ui.close();
                        }
                    }
                });
                // Each time the list opens, it starts with an empty search.
                if list.inner.is_none() && !more_fonts {
                    self.font_filter.clear();
                }
                if let Some(Some(mm)) = sizes.get(&current) {
                    if s.height_mm < *mm {
                        ui.label(
                            RichText::new(t!(
                                "Thin strokes of this font get lost below about {mm} mm. Make the letters taller or pick a bolder font.",
                                mm = mm
                            ))
                            .size(12.0)
                            .color(super::WARN),
                        );
                    } else {
                        ui.label(RichText::new(t!("This font stitches cleanly from about {mm} mm tall.", mm = mm)).size(12.0).color(muted));
                    }
                }
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
                    egui::ComboBox::from_id_salt("text-shape").selected_text(crate::i18n::tr(s.shape.name())).width(ui.available_width() - 8.0).show_ui(
                        ui,
                        |ui| {
                            for sh in lettering::TextShape::ALL {
                                ui.selectable_value(&mut s.shape, sh, crate::i18n::tr(sh.name()));
                            }
                        },
                    );
                });
                match s.shape {
                    lettering::TextShape::Arc => {
                        slider_row(ui, t!("Arc"), &mut s.arc_deg, -360.0..=360.0, "°", 0)
                            .on_hover_text(t!("Bend the text into an arch (positive) or a smile (negative). 360° closes the circle."));
                    }
                    lettering::TextShape::Circle => {
                        slider_row(ui, t!("Circle"), &mut s.circle_mm, 20.0..=180.0, " mm", 0).on_hover_text(t!("Inside diameter of the ring of text"));
                        ui.label(
                            RichText::new(t!("Line 1 runs round the top, line 2 round the bottom, more lines sit in the middle.")).size(12.0).color(muted),
                        );
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
                    &[
                        (PatchShape::None, t!("Off")),
                        (PatchShape::Rect, t!("Box")),
                        (PatchShape::Rounded, t!("Round")),
                        (PatchShape::Oval, t!("Oval")),
                        (PatchShape::Circle, t!("Circle")),
                        (PatchShape::Fob, t!("Fob")),
                    ],
                    accent,
                );
                if s.patch != PatchShape::None {
                    ui.add_space(4.0);
                    slider_row(ui, t!("Padding"), &mut s.patch_pad_mm, 1.0..=25.0, " mm", 1).on_hover_text(t!("Space between the text and the patch edge"));
                    slider_row(ui, t!("Border"), &mut s.patch_border_mm, 0.0..=6.0, " mm", 1).on_hover_text(t!("Satin border width; 0 for none"));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(t!("Background")).color(muted));
                        for (k, app, name, about) in [
                            (FillKind::Tatami, false, t!("Filled"), t!("Stitched solid")),
                            (FillKind::Open, false, t!("Light"), t!("An open fill, the fabric shows through")),
                            (FillKind::Crosshatch, false, t!("Lace"), t!("A double lattice for free-standing lace on water-soluble stabilizer")),
                            (FillKind::None, false, t!("Fabric"), t!("No background stitching")),
                            (
                                FillKind::None,
                                true,
                                t!("Appliqué"),
                                t!("A piece of fabric sewn on: placement line, stop, tack-down, stop, then the cover stitch"),
                            ),
                        ] {
                            let on = s.patch_fill == k && s.patch_applique == app;
                            if ui.selectable_label(on, name).on_hover_text(about).clicked() {
                                s.patch_fill = k;
                                s.patch_applique = app;
                            }
                        }
                    });
                    if s.patch_applique {
                        ui.label(
                            RichText::new(t!("The machine stops twice: lay the fabric over the placement line, then trim it close to the tack-down."))
                                .size(12.0)
                                .color(muted),
                        );
                        jef_picker_labeled(ui, "patch-bg", t!("Guide"), &mut s.patch_thread);
                    } else if s.patch_fill != FillKind::None {
                        jef_picker_labeled(ui, "patch-bg", t!("Fill"), &mut s.patch_thread);
                    }
                    if s.patch_border_mm > 0.0 {
                        jef_picker_labeled(ui, "patch-border", t!("Border"), &mut s.patch_border_thread);
                    }
                    ui.checkbox(&mut s.patch_ith, t!("In the hoop: finish the back")).on_hover_text(t!("Before the border the machine stops: tape backing fabric under the hoop; a tack-down holds it and the border covers both edges. Cut out round the border."));
                    if s.patch_fill == FillKind::Crosshatch {
                        ui.label(
                            RichText::new(t!("Lace: hoop two layers of water-soluble stabilizer, sew, then rinse the stabilizer away."))
                                .size(12.0)
                                .color(muted),
                        );
                    }
                }
            });
        }
        if let Some(tip) = tip_later {
            self.toast(tip.to_string());
        }
        if more_fonts {
            self.fontwin.search(&self.font_filter);
            self.font_filter.clear();
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
}
