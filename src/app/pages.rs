//! The Design, Hoop, Stitch and View pages of the right dock, the Colors
//! list, and the one-click fixes for a bad fit.

use super::*;

impl HoopApp {
    /// The right dock's page for the chosen tab.
    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect, tab: Tab) {
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
                        ui.label(RichText::new(t!("Open a JEF, DST, EXP, PES, PEC or VP3 file, or drop one on the hoop.")).color(muted));
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
                    if ui.add_enabled(self.design.is_some(), egui::Button::new(t!("Save…")).min_size(vec2(w, 30.0))).on_hover_text("Ctrl+S").clicked() {
                        self.save_design();
                    }
                });
            });
        }
        if tab == Tab::Design {
            self.stitch_tools_card(ui);
        }
        // When the design fits, the hoop card says so in a line; problems get
        // a box of their own at the top.
        let mut fit_line: Option<(String, String, Vec<(String, FitFix)>)> = None;
        if tab == Tab::Hoop
            && let Some(d) = &self.design
        {
            let fit = match (self.pinned.is_empty(), self.hoop_bounds()) {
                (false, Some(b)) => {
                    let at = Placement { dx: (b.0 + b.2) / 2.0, dy: (b.1 + b.3) / 2.0, ..Default::default() };
                    hoops::fit(self.hoop_size(), (b.2 - b.0, b.3 - b.1), &at)
                }
                _ => hoops::fit(self.hoop_size(), d.size(), &self.setup.at),
            };
            let (ok, title, detail) = match fit {
                Fit::Inside(m) => (true, t!("Fits the {v0}", v0 = hoops::get(self.setup.hoop).name), t!("Closest edge is {m:.1} mm away.", m = m)),
                Fit::TooBig { over_w, over_h, fits_turned } => {
                    let mut parts = Vec::new();
                    if over_w > 0.0 {
                        parts.push(t!("{over_w:.1} mm too wide", over_w = over_w));
                    }
                    if over_h > 0.0 {
                        parts.push(t!("{over_h:.1} mm too tall", over_h = over_h));
                    }
                    let hint = if fits_turned { t!(" Rotate it 90° (R) and it fits.") } else { t!(" Pick a bigger hoop.") };
                    (false, t!("Doesn't fit this hoop").into(), format!("{}.{hint}", parts.join(", ")))
                }
                Fit::OffEdge { side, by } => {
                    (false, t!("Off the sewing area").into(), t!("Sticks out {by:.1} mm on the {side}. Press C to center.", by = by, side = side))
                }
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
                    let i = hoops::smallest_fitting(size);
                    if i != self.setup.hoop && matches!(hoops::fit((hoops::get(i).w, hoops::get(i).h), size, &unturned), Fit::Inside(_)) {
                        actions.push((t!("Use {hoop}", hoop = hoops::get(i).name), FitFix::Hoop(i)));
                    }
                    if self.mode != Mode::Viewer {
                        actions.push((t!("Shrink to fit").into(), FitFix::Scale));
                    }
                    actions.push((t!("Split into hoopings…").into(), FitFix::Split));
                }
                Fit::OffEdge { .. } => actions.push((t!("Center").into(), FitFix::Center)),
                Fit::Inside(m) if self.mode != Mode::Viewer && m > 15.0 => actions.push((t!("Grow to fill hoop").into(), FitFix::Scale)),
                Fit::Inside(_) => {}
            }
            let mut chosen = None;
            if let Fit::Inside(m) = fit {
                fit_line = Some((title.to_string(), t!("{m:.1} mm to spare", m = m), std::mem::take(&mut actions)));
            }
            if !ok {
                egui::Frame::new()
                    .fill(color.gamma_multiply(0.12))
                    .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5)))
                    .corner_radius(10)
                    .inner_margin(12)
                    .show(ui, |ui| {
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
            }
            if let Some(fix) = chosen {
                self.apply_fix(fix, canvas_rect);
            }
        }

        if tab == Tab::Hoop {
            let mut chosen = None;
            card(ui, t!("Hoop"), accent, |ui| {
                if let Some((title, spare, actions)) = &fit_line {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.5, OK);
                        ui.label(RichText::new(title).color(OK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(spare).size(12.0).color(muted)).on_hover_text(t!("From the design to the closest edge of the sewing area"));
                        });
                    });
                    if !actions.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            for (label, fix) in actions {
                                if ui.small_button(label).clicked() {
                                    chosen = Some(*fix);
                                }
                            }
                        });
                    }
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    ui.label(t!("Machine"));
                    let current = hoops::machine_index();
                    let mut pick = current;
                    egui::ComboBox::from_id_salt("page-machine")
                        .selected_text(crate::i18n::tr(hoops::machine().name))
                        .width(ui.available_width() - 8.0)
                        .show_ui(ui, |ui| {
                            for (i, m) in hoops::MACHINES.iter().enumerate() {
                                ui.selectable_value(&mut pick, i, format!("{} · {}", crate::i18n::tr(m.name), m.out.ext().to_uppercase()));
                            }
                        });
                    if pick != current {
                        self.choose_machine(pick, canvas_rect);
                    }
                });
                ui.add_space(4.0);
                let design_size = self.design.as_ref().map(|d| d.size());
                // The design as it sits, turned or not, in mm, for the tiles' drawings.
                let footprint = self.design.as_ref().map(|d| {
                    let (w, h) = self.setup.at.size(d.size().0, d.size().1);
                    (w / 10.0, h / 10.0)
                });
                let biggest = hoops::list().iter().fold(1.0f32, |m, h| m.max(h.w).max(h.h));
                let w = (ui.available_width() - 8.0) / 2.0;
                for row in hoops::list().chunks(2).enumerate() {
                    ui.horizontal(|ui| {
                        for (j, h) in row.1.iter().enumerate() {
                            let i = row.0 * 2 + j;
                            let fits =
                                design_size.is_none_or(|s| !matches!(hoops::fit((h.w, h.h), s, &Placement::default()), Fit::TooBig { fits_turned: false, .. }));
                            if hoop_tile(ui, w, i, h, fits, self.setup.hoop == i, footprint, biggest, accent, muted).clicked() {
                                self.change(|s| s.hoop = i);
                                self.fit_view(canvas_rect, false);
                            }
                        }
                    });
                }
            });
            if let Some(fix) = chosen {
                self.apply_fix(fix, canvas_rect);
            }

            card(ui, t!("Position"), accent, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let w = (ui.available_width() - 18.0) / 4.0;
                let at = self.setup.at;
                ui.horizontal(|ui| {
                    let b = |t: &str| egui::Button::new(t).min_size(vec2(w, 28.0));
                    if ui.add(b(t!("Left 90°"))).on_hover_text(t!("Rotate left · Shift+R")).clicked() {
                        self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
                    }
                    if ui.add(b(t!("Right 90°"))).on_hover_text(t!("Rotate right · R")).clicked() {
                        self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
                    }
                    if ui.add(b(t!("Mirror")).selected(at.mirror)).on_hover_text("M").clicked() {
                        self.change(|s| s.at.mirror = !s.at.mirror);
                    }
                    if ui.add(b(t!("Center"))).on_hover_text("C").clicked() {
                        self.change(|s| {
                            s.at.dx = 0.0;
                            s.at.dy = 0.0;
                        });
                    }
                });
                ui.add_space(2.0);
                // Offset from the hoop's center in mm, up and right positive.
                ui.horizontal(|ui| {
                    // Adding 0.0 turns -0.0 into 0.0.
                    let (mut x, mut y) = (at.dx / 10.0 + 0.0, -at.dy / 10.0 + 0.0);
                    ui.label(RichText::new("X").color(muted));
                    let rx = ui.add_sized(vec2(w, 24.0), mm_field(&mut x)).on_hover_text(t!("Right of the hoop's center"));
                    ui.label(RichText::new("Y").color(muted));
                    let ry = ui.add_sized(vec2(w, 24.0), mm_field(&mut y)).on_hover_text(t!("Above the hoop's center"));
                    if rx.changed() || ry.changed() {
                        self.change(|s| {
                            s.at.dx = x * 10.0;
                            s.at.dy = -y * 10.0;
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{}°", at.quarter_turns as u32 * 90)).monospace().color(muted)).on_hover_text(t!("Turned"));
                    });
                });
            });
        }

        if tab == Tab::Hoop && (self.design.is_some() || !self.pinned.is_empty()) {
            let mut remove = None;
            let mut clear = false;
            let mut pin = false;
            let body = |ui: &mut egui::Ui| {
                if self.pinned.is_empty() {
                    ui.label(
                        RichText::new(t!(
                            "Pin this design to the hoop, then make or open the next part, like a name under a logo. Saving saves all of them as one file."
                        ))
                        .size(12.0)
                        .color(muted),
                    );
                }
                for (i, p) in self.pinned.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).monospace().color(muted));
                        ui.label(&p.name);
                        ui.label(
                            RichText::new(t!("{v0:.0} × {v1:.0} mm", v0 = (p.bounds.2 - p.bounds.0) / 10.0, v1 = (p.bounds.3 - p.bounds.1) / 10.0))
                                .size(12.0)
                                .color(muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(t!("Remove")).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.design.is_some(), egui::Button::new(t!("Pin to hoop")))
                        .on_hover_text(t!("P · keep this design where it is and start the next part"))
                        .clicked()
                    {
                        pin = true;
                    }
                    if !self.pinned.is_empty() && ui.button(t!("Clear pinned")).clicked() {
                        clear = true;
                    }
                });
            };
            // Folded until something is pinned: most designs are one part.
            if self.pinned.is_empty() {
                card_folded(ui, t!("Combine"), accent, body);
            } else {
                card(ui, t!("Combine"), accent, body);
            }
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

        if tab == Tab::Stitch && self.design.is_some() {
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
            let top: f32 = d.blocks.iter().map(|b| p.thread_meters(b)).sum();
            let rows = [
                (t!("Size"), t!("{v0:.1} × {v1:.1} mm", v0 = w / 10.0, v1 = h / 10.0)),
                (t!("Stitches"), stitches.to_string()),
                (t!("Colors"), d.blocks.len().to_string()),
                // Bobbin thread runs about a third of the top thread.
                (t!("Thread"), t!("{v0:.0} m · bobbin ≈ {v1:.0} m", v0 = top, v1 = top / 3.0)),
                (t!("Jumps"), if trims > 0 { t!("{v0} · {trims} trims", v0 = p.count(Cmd::Jump), trims = trims) } else { p.count(Cmd::Jump).to_string() }),
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
            card(ui, t!("Fabric"), accent, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let mut preset = false;
                    for (name, c) in FABRICS {
                        let selected = self.fabric == c;
                        preset |= selected;
                        let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                        if selected {
                            ui.painter().circle_stroke(rect.center(), 12.5, egui::Stroke::new(2.0, accent));
                        }
                        ui.painter().circle(rect.center(), 10.0, rgb(c), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
                        if resp.on_hover_text(crate::i18n::tr(name)).clicked() {
                            self.fabric = c;
                        }
                    }
                    // Any other color: a round swatch like the rest, showing the
                    // color while it is in use and a plus otherwise.
                    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    let ink = ui.visuals().text_color();
                    if preset {
                        ui.painter().circle_stroke(rect.center(), 10.0, egui::Stroke::new(1.0, ink.gamma_multiply(0.5)));
                        super::icons::paint(ui, "plus", egui::Rect::from_center_size(rect.center(), vec2(12.0, 12.0)), ink);
                    } else {
                        ui.painter().circle_stroke(rect.center(), 12.5, egui::Stroke::new(2.0, accent));
                        ui.painter().circle(rect.center(), 10.0, rgb(self.fabric), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
                    }
                    let resp = resp.on_hover_text(t!("Other fabric color"));
                    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                        let f = self.fabric;
                        let mut c = Color32::from_rgb(f[0], f[1], f[2]);
                        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
                            self.fabric = [c.r(), c.g(), c.b()];
                        }
                    });
                });
            });
            card(ui, t!("Display"), accent, |ui| {
                // A switch with its key, the key lined up on the right.
                let toggle = |ui: &mut egui::Ui, on: &mut bool, label: &str, key: &str| {
                    ui.horizontal(|ui| {
                        ui.checkbox(on, label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(key).monospace().size(12.0).color(muted));
                        });
                    });
                };
                toggle(ui, &mut self.textured, t!("Thread texture"), "T");
                toggle(ui, &mut self.grid, t!("Template grid, 10 mm"), "G");
                toggle(ui, &mut self.jumps, t!("Jump stitches"), "J");
                toggle(ui, &mut self.sizes, t!("Size label under the design"), "");
                ui.add_space(4.0);
                if ui
                    .add_enabled(self.design.is_some(), egui::Button::new(t!("Save picture…")))
                    .on_hover_text(t!("A PNG of the hoop preview, to share · Ctrl+Shift+S"))
                    .clicked()
                {
                    self.save_picture();
                }
            });
        }
        ui.add_space(14.0);
    }

    /// The Colors list in the left dock: each thread of the design in sewing
    /// order. Clicking one shows it on its own.
    pub(super) fn colors_list(&mut self, ui: &mut egui::Ui) {
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
            let (shown, total) = (self.shown, self.total());
            let mut toggle = None;
            let mut block_op = None;
            let matched: Vec<Option<formats::Thread>> = self.design.as_ref().unwrap().blocks.iter().map(|b| self.chart_thread(&b.thread)).collect();
            let chart_name = self.chart().name.clone();
            let d = self.design.as_mut().unwrap();
            let meters: Vec<f32> = d.blocks.iter().map(|b| d.pattern.thread_meters(b)).collect();
            let mut copied = false;
            {
                for (k, b) in d.blocks.iter_mut().enumerate() {
                    let selected = focus == Some(k);
                    // The thread the stitch-out is on, while it is part way.
                    let sewing = shown < total && shown > b.start && shown <= b.end;
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
                                ui.label(
                                    RichText::new(t!("{code}{v0:.1} m · {stitches} st", code = code, v0 = meters[k], stitches = b.stitches))
                                        .size(11.5)
                                        .color(muted),
                                )
                                .on_hover_text(t!(
                                    "{stitches} stitches, about {v1:.1} m of thread",
                                    stitches = b.stitches,
                                    v1 = meters[k]
                                ));
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
                    if sewing {
                        let rect = r.response.rect;
                        ui.painter().line_segment(
                            [rect.left_top() + vec2(1.0, 6.0), rect.left_bottom() + vec2(1.0, -6.0)],
                            egui::Stroke::new(3.0, accent),
                        );
                        let at = egui::Rect::from_center_size(egui::pos2(rect.right() - 12.0, rect.top() + 12.0), vec2(14.0, 14.0));
                        super::icons::paint(ui, "needle", at, accent);
                    }
                    let row = ui
                        .interact(r.response.rect, ui.id().with(("thread", k)), Sense::click())
                        .on_hover_text(t!("Click to show this thread on its own and sew up to it"));
                    if row.clicked() {
                        toggle = Some(k);
                    }
                    if viewer {
                        row.context_menu(|ui| {
                            if let Some(op) = HoopApp::block_menu(ui) {
                                block_op = Some((k, op));
                            }
                        });
                    }
                }
                if !d.pattern.format.stores_colors() {
                    ui.label(
                        RichText::new(t!("{format} files don't store thread colors. Click a swatch to set the real ones.", format = d.pattern.format))
                            .size(12.0)
                            .color(muted),
                    );
                }
                if ui.small_button(t!("Copy thread list")).on_hover_text(t!("For shopping or notes")).clicked() {
                    let list: Vec<String> = d
                        .blocks
                        .iter()
                        .enumerate()
                        .map(|(k, b)| {
                            let alt = matched[k].as_ref().map_or(String::new(), |t| format!(" ({chart_name} {} {})", t.code, t.name));
                            t!(
                                "{v0}. {name} {code}{alt} — {stitches} stitches, about {v4:.1} m",
                                v0 = k + 1,
                                name = b.thread.name,
                                code = b.thread.code,
                                alt = alt,
                                stitches = b.stitches,
                                v4 = meters[k]
                            )
                        })
                        .collect();
                    ui.ctx().copy_text(format!("{}\n{}", d.name, list.join("\n")));
                    copied = true;
                }
            }
            if copied {
                self.toast(t!("Thread list copied").into());
            }
            if ui
                .small_button(t!("Thread chart: {chart_name}…", chart_name = chart_name))
                .on_hover_text(t!("Pick a brand and mark the threads you have"))
                .clicked()
            {
                self.threads.open = true;
            }
            if let Some((k, op)) = block_op {
                self.block_op(k, op);
            }
            // Showing a thread alone also sews the timeline up to its end;
            // showing all again sews everything.
            if let Some(k) = toggle {
                self.playing = false;
                if self.focus == Some(k) {
                    self.focus = None;
                    self.shown = self.total();
                } else {
                    self.focus = Some(k);
                    self.shown = self.design.as_ref().map_or(0, |d| d.blocks[k].end);
                }
            }
        }
    }

    pub(super) fn apply_fix(&mut self, fix: FitFix, canvas_rect: egui::Rect) {
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
            FitFix::Split => self.open_hoopings(),
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
}

/// A drag-or-type field for a length in mm, to a tenth.
fn mm_field(v: &mut f32) -> egui::DragValue<'_> {
    egui::DragValue::new(v).speed(0.1).fixed_decimals(1).suffix(" mm")
}
