//! The stitch-out timeline under the hoop: a thin bar to play and scrub the
//! sewing order, colored by thread, that opens into a strip of small
//! renders, one for each thread.

use super::*;

/// Height of the bar itself, and of a thread's card when the strip is open.
const BAR: f32 = 30.0;
const CARD: Vec2 = vec2(68.0, 74.0);

/// Most points drawn for a card's thread, and for the faint rest of the
/// design behind it, so long designs stay cheap to paint.
const CARD_POINTS: usize = 2500;
const GHOST_POINTS: usize = 1500;

/// What a thread card's menu asks for.
enum CardOp {
    Alone(usize),
    PlayFrom(usize),
    Color(usize, [u8; 3]),
    Block(usize, stitchedit::BlockOp),
}

impl HoopApp {
    pub(super) fn timeline(&mut self, ui: &mut egui::Ui) {
        self.timeline_hover = None;
        let Some(d) = &self.design else {
            ui.add_sized(
                vec2(ui.available_width(), BAR - 6.0),
                egui::Label::new(RichText::new(t!("Open a design to play its stitch-out")).size(12.0).color(self.muted())),
            );
            return;
        };
        let total = d.pattern.points.len();
        let accent = self.accent();
        let muted = self.muted();
        let thread = d.blocks.iter().position(|b| self.shown <= b.end).unwrap_or(0);
        let colors = d.blocks.len();
        let stitches = d.pattern.count(Cmd::Stitch);
        let sewn = d.pattern.cmds[..self.shown.min(total)].iter().filter(|c| **c == Cmd::Stitch).count();
        let spm = SPEEDS[self.speed] as f32;
        ui.horizontal(|ui| {
            ui.set_height(BAR - 6.0);
            ui.spacing_mut().item_spacing.x = 2.0;
            let (icon, tip) = if self.playing { ("pause", t!("Pause the stitch-out · Space")) } else { ("play", t!("Play the stitch-out · Space")) };
            if icons::button(ui, true, self.playing, icon, tip) {
                self.toggle_play();
            }
            if icons::button(ui, true, false, "skip-back", t!("Previous thread · [")) {
                self.jump_thread(false);
            }
            if icons::button(ui, true, false, "skip-forward", t!("Next thread · ]")) {
                self.jump_thread(true);
            }
            ui.add_space(8.0);
            // Right side first, so the scrubber takes what is left.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (icon, tip) =
                    if self.timeline_open { ("caret-down", t!("Hide the thread renders · F8")) } else { ("caret-up", t!("Show the thread renders · F8")) };
                if icons::button(ui, true, false, icon, tip) {
                    self.timeline_open = !self.timeline_open;
                }
                egui::ComboBox::from_id_salt("timeline-rate").selected_text(crate::i18n::tr(PLAYBACK[self.play_rate].1)).width(84.0).show_ui(ui, |ui| {
                    for (i, (_, name)) in PLAYBACK.iter().enumerate() {
                        ui.selectable_value(&mut self.play_rate, i, crate::i18n::tr(name));
                    }
                });
                // Time at the machine's speed, thread changes not counted.
                let text = format!("{} / {}  ·  {}/{colors}", clock(sewn as f32 / spm), clock(stitches as f32 / spm), thread + 1);
                ui.label(RichText::new(text).monospace().size(12.0).color(muted)).on_hover_text(t!(
                    "Sewing time at {spm} spm, without thread changes · stitch {sewn} of {stitches} · thread {k} of {colors}",
                    spm = SPEEDS[self.speed],
                    sewn = sewn,
                    stitches = stitches,
                    k = thread + 1,
                    colors = colors
                ));
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if let Some(n) = self.scrubber(ui, accent) {
                        self.playing = false;
                        self.shown = n;
                    }
                });
            });
        });
        if self.timeline_open {
            ui.add_space(4.0);
            self.thread_strip(ui, accent, muted);
            ui.add_space(6.0);
        }
    }

    /// The sewing order as one track, a band per thread, with the needle's
    /// place on it. Gives the stitch dragged or clicked to.
    fn scrubber(&mut self, ui: &mut egui::Ui, accent: Color32) -> Option<usize> {
        let d = self.design.as_ref()?;
        let total = d.pattern.points.len().max(1);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(60.0), 20.0), Sense::click_and_drag());
        let track = egui::Rect::from_center_size(rect.center(), vec2(rect.width() - 8.0, 8.0));
        let p = ui.painter();
        let v = ui.visuals();
        p.rect_filled(track, 4, rgb(self.fabric));
        let x_of = |i: usize| track.left() + track.width() * i as f32 / total as f32;
        let head = x_of(self.shown);
        for (k, b) in d.blocks.iter().enumerate() {
            let band = egui::Rect::from_x_y_ranges(x_of(b.start)..=x_of(b.end).max(x_of(b.start) + 1.0), track.y_range());
            let c = rgb(b.thread.color);
            let lit = band.intersect(egui::Rect::from_x_y_ranges(track.left()..=head, track.y_range()));
            p.rect_filled(band, 0, c.gamma_multiply(0.3));
            if lit.width() > 0.0 {
                p.rect_filled(lit, 0, c);
            }
            if k > 0 {
                p.line_segment(
                    [egui::pos2(band.left(), track.top() - 2.0), egui::pos2(band.left(), track.bottom() + 2.0)],
                    egui::Stroke::new(1.0, v.panel_fill),
                );
            }
        }
        p.rect_stroke(track, 4, egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Outside);
        // A tick under the track where the machine trims or jumps: the slow
        // spots. A run of them is one tick, and ticks never crowd closer
        // than a few pixels.
        let tick = egui::Stroke::new(1.0, v.text_color().gamma_multiply(0.5));
        let mut last = f32::MIN;
        for i in 1..d.pattern.cmds.len() {
            if matches!(d.pattern.cmds[i], Cmd::Jump | Cmd::Trim) && d.pattern.cmds[i - 1] == Cmd::Stitch {
                let x = x_of(i);
                if x - last >= 3.0 {
                    p.line_segment([egui::pos2(x, track.bottom() + 2.0), egui::pos2(x, rect.bottom())], tick);
                    last = x;
                }
            }
        }
        let knob = egui::pos2(head, track.center().y);
        p.line_segment([egui::pos2(head, rect.top() + 1.0), egui::pos2(head, rect.bottom() - 1.0)], egui::Stroke::new(2.0, accent));
        p.circle_filled(knob, if resp.hovered() || resp.dragged() { 5.5 } else { 4.5 }, accent);
        let mut hover = None;
        let at = |x: f32| (((x - track.left()) / track.width()).clamp(0.0, 1.0) * total as f32).round() as usize;
        if let Some(pos) = resp.hover_pos() {
            let i = at(pos.x);
            if let Some(k) = d.blocks.iter().position(|b| i <= b.end) {
                hover = Some(k);
                let b = &d.blocks[k];
                resp.clone().on_hover_text_at_pointer(t!("Stitch {i} · thread {k}: {name}", i = i, k = k + 1, name = b.thread.name));
            }
        }
        let picked = if resp.dragged() || resp.clicked() { resp.interact_pointer_pos().map(|pos| at(pos.x)) } else { None };
        if hover.is_some() {
            self.timeline_hover = hover;
        }
        picked
    }

    /// A small render of each thread's stitches, in sewing order.
    fn thread_strip(&mut self, ui: &mut egui::Ui, accent: Color32, muted: Color32) {
        let Some(d) = &self.design else { return };
        let t = Transform::new(d.bounds, &self.setup.at);
        let (sw, sh) = self.setup.at.size(d.size().0, d.size().1);
        let center = (self.setup.at.dx, self.setup.at.dy);
        let fabric = rgb(self.fabric);
        let finished = self.shown >= d.pattern.points.len();
        let ghost = thin(d.pattern.points.len(), GHOST_POINTS);
        let mut jump = None;
        let mut hover = None;
        let mut op = None;
        let viewer = self.mode == Mode::Viewer;
        egui::ScrollArea::horizontal().id_salt("thread-strip").auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (k, b) in d.blocks.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(CARD, Sense::click());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let v = ui.visuals();
                    let current = !finished && self.shown > b.start && self.shown <= b.end;
                    let done = self.shown >= b.end;
                    let p = ui.painter();
                    let fill = if current {
                        accent.gamma_multiply(0.18)
                    } else if resp.hovered() {
                        v.widgets.hovered.weak_bg_fill
                    } else {
                        v.extreme_bg_color
                    };
                    p.rect_filled(rect, 6, fill);
                    let edge = if current { egui::Stroke::new(1.5, accent) } else { egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color) };
                    p.rect_stroke(rect, 6, edge, egui::StrokeKind::Inside);
                    // The thread on the fabric, the rest of the design faint behind it.
                    let cloth = egui::Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(rect.width() - 8.0, rect.height() - 22.0));
                    p.rect_filled(cloth, 4, fabric);
                    let art = cloth.shrink(4.0);
                    let s = (art.width() / sw.max(1.0)).min(art.height() / sh.max(1.0));
                    let to_card = |pt: (f32, f32)| {
                        let (x, y) = t.apply(pt);
                        art.center() + vec2(x - center.0, y - center.1) * s
                    };
                    for (j, o) in d.blocks.iter().enumerate() {
                        if j != k {
                            paint_run(p, &d.pattern, o.start, o.end, ghost, &to_card, rgb(o.thread.color).gamma_multiply(0.15));
                        }
                    }
                    let c = rgb(b.thread.color);
                    let upto = if done { b.end } else { self.shown.clamp(b.start, b.end) };
                    // Not sewn yet stays faint; the needle's thread fills in as it plays.
                    paint_run(p, &d.pattern, b.start, b.end, thin(b.end - b.start, CARD_POINTS), &to_card, c.gamma_multiply(if done { 1.0 } else { 0.3 }));
                    if !done && upto > b.start {
                        paint_run(p, &d.pattern, b.start, upto, thin(b.end - b.start, CARD_POINTS), &to_card, c);
                    }
                    let swatch = egui::Rect::from_min_size(egui::pos2(rect.left() + 6.0, rect.bottom() - 15.0), vec2(10.0, 10.0));
                    p.circle_filled(swatch.center(), 5.0, c);
                    p.circle_stroke(swatch.center(), 5.0, egui::Stroke::new(1.0, muted.gamma_multiply(0.6)));
                    let label =
                        p.layout_no_wrap(format!("{} · {}", k + 1, short(b.stitches)), egui::FontId::proportional(10.5), if current { accent } else { muted });
                    p.galley(egui::pos2(swatch.right() + 4.0, swatch.center().y - label.size().y / 2.0), label, muted);
                    let code = if b.thread.code.is_empty() { String::new() } else { format!(" ({})", b.thread.code) };
                    let resp = resp.on_hover_text(t!(
                        "Thread {k}: {name}{code} · {stitches} stitches\nClick to sew up to the end of it",
                        k = k + 1,
                        name = b.thread.name,
                        code = code,
                        stitches = b.stitches
                    ));
                    if resp.hovered() {
                        hover = Some(k);
                    }
                    if resp.clicked() {
                        jump = Some(b.end);
                    }
                    resp.context_menu(|ui| {
                        let alone = if self.focus == Some(k) { t!("Show all threads") } else { t!("Show this thread alone") };
                        if ui.button(alone).clicked() {
                            op = Some(CardOp::Alone(k));
                        }
                        if ui.button(t!("Play from here")).clicked() {
                            op = Some(CardOp::PlayFrom(b.start));
                        }
                        if viewer {
                            ui.separator();
                            ui.menu_button(t!("Thread color"), |ui| {
                                let mut c = Color32::from_rgb(b.thread.color[0], b.thread.color[1], b.thread.color[2]);
                                if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
                                    op = Some(CardOp::Color(k, [c.r(), c.g(), c.b()]));
                                }
                            });
                            if let Some(o) = HoopApp::block_menu(ui) {
                                op = Some(CardOp::Block(k, o));
                            }
                        }
                        if matches!(op, Some(CardOp::Alone(_) | CardOp::PlayFrom(_))) {
                            ui.close();
                        }
                    });
                }
            });
        });
        if hover.is_some() {
            self.timeline_hover = hover;
        }
        if let Some(n) = jump {
            self.playing = false;
            self.shown = n;
        }
        match op {
            Some(CardOp::Alone(k)) => self.focus = if self.focus == Some(k) { None } else { Some(k) },
            Some(CardOp::PlayFrom(n)) => {
                self.shown = n;
                self.play_acc = 0.0;
                self.playing = true;
            }
            Some(CardOp::Color(k, c)) => {
                if let Some(b) = self.design.as_mut().and_then(|d| d.blocks.get_mut(k)) {
                    b.thread.color = c;
                }
            }
            Some(CardOp::Block(k, o)) => self.block_op(k, o),
            None => {}
        }
    }
}

/// The step that keeps `n` records down to about `most`.
fn thin(n: usize, most: usize) -> usize {
    (n / most).max(1)
}

/// The stitches of records `from..to`, every `step`th one, as lines broken
/// at jumps and trims.
fn paint_run(p: &egui::Painter, pattern: &Pattern, from: usize, to: usize, step: usize, at: &dyn Fn((f32, f32)) -> egui::Pos2, color: Color32) {
    let stroke = egui::Stroke::new(1.0, color);
    let mut line: Vec<egui::Pos2> = Vec::new();
    let mut i = from;
    while i < to {
        if pattern.cmds[i] == Cmd::Stitch {
            line.push(at(pattern.points[i]));
        } else if line.len() > 1 {
            p.add(egui::Shape::line(std::mem::take(&mut line), stroke));
        } else {
            line.clear();
        }
        i += step;
    }
    if line.len() > 1 {
        p.add(egui::Shape::line(line, stroke));
    }
}

/// Minutes as m:ss.
fn clock(minutes: f32) -> String {
    let s = (minutes * 60.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A stitch count kept short for a card: 840, 12.4k.
fn short(n: usize) -> String {
    if n < 1000 { n.to_string() } else { format!("{:.1}k", n as f32 / 1000.0) }
}
