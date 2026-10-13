//! Right-click menus: on artwork shapes (on the hoop or in Objects), on
//! empty fabric in Artwork, and on the design elsewhere. Also copying and
//! pasting shapes, which goes through the system clipboard as a marker so
//! that Ctrl+V still pastes SVG code copied from elsewhere.

use super::shapes::Arrange;
use super::shell::{Dock, Tab, item};
use super::tools::{Form, Pick};
use super::*;
use crate::artwork::{self, Look, ShapeEdit};
use crate::lettering::FillKind;

/// What the clipboard holds when Hoop copied shapes.
const MARKER: &str = "hoop-shapes";

impl HoopApp {
    /// Copies the picked shapes, with their outlines, so they can be pasted
    /// into this drawing or another one.
    pub(super) fn copy_shapes(&mut self, ctx: &egui::Context) {
        let (Some(art), Some(s)) = (&self.artwork, &self.art) else { return };
        let mut sel = self.sel.clone();
        sel.sort_unstable();
        self.shape_clip = sel
            .iter()
            .filter_map(|&i| s.shapes.get(i))
            .map(|e| {
                let mut c = e.clone();
                if c.own.is_none() {
                    c.own = Some(e.base(art).clone());
                }
                c.source = usize::MAX;
                c
            })
            .collect();
        if !self.shape_clip.is_empty() {
            ctx.copy_text(MARKER.to_string());
            self.toast(tn!(self.shape_clip.len(), "Copied {n} shape", "Copied {n} shapes"));
        }
    }

    /// Pastes copied shapes on top, 5 mm off, and picks them. Copied groups
    /// become new groups.
    pub(super) fn paste_shapes(&mut self) {
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
        if self.shape_clip.is_empty() {
            return;
        }
        let (scale, _) = artwork::frame(art, s);
        let mut ids: Vec<(u32, u32)> = Vec::new();
        let first = s.shapes.len();
        for c in &self.shape_clip {
            let mut e: ShapeEdit = c.clone();
            e.name = t!("{name} copy", name = e.name);
            e.dx += 50.0 / scale;
            e.dy += 50.0 / scale;
            if e.layer >= s.layers.len() {
                e.layer = 0;
                if let Some(own) = &mut e.own {
                    own.layer = 0;
                }
            }
            e.group = e.group.map(|g| match ids.iter().find(|p| p.0 == g) {
                Some(p) => p.1,
                None => {
                    let id = s.groups.iter().map(|x| x.id).max().unwrap_or(0) + 1;
                    let name = self.shape_clip_groups.iter().find(|x| x.id == g).map_or_else(|| t!("Group {n}", n = id), |x| x.name.clone());
                    s.groups.push(artwork::Group { id, name });
                    ids.push((g, id));
                    id
                }
            });
            s.shapes.push(e);
        }
        self.sel = (first..s.shapes.len()).collect();
        self.edit_points = false;
    }

    /// Ctrl+C on picked shapes, and Ctrl+V of shapes Hoop copied.
    pub(super) fn clipboard_events(&mut self, ctx: &egui::Context) {
        if self.mode != Mode::Artwork || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (copy, paste) = ctx
            .input(|i| (i.events.iter().any(|e| matches!(e, egui::Event::Copy)), i.events.iter().any(|e| matches!(e, egui::Event::Paste(t) if t == MARKER))));
        if copy && !self.sel.is_empty() {
            self.remember_clip_groups();
            self.copy_shapes(ctx);
        }
        if paste {
            self.paste_shapes();
        }
    }

    fn remember_clip_groups(&mut self) {
        self.shape_clip_groups = self.art.as_ref().map_or(Vec::new(), |s| s.groups.clone());
    }

    /// Picks what was right-clicked on the hoop, before its menu opens:
    /// the shape (with its group) unless it is already picked.
    pub(super) fn right_click_pick(&mut self, response: &egui::Response, frame: &Frame) {
        if !response.secondary_clicked() || self.edit_points {
            return;
        }
        let (Some(m), Some(art), Some(s), Some(p)) = (self.shape_map(), &self.artwork, &self.art, response.interact_pointer_pos()) else { return };
        let hit = artwork::shape_at(art, s, m.to_stitch(m.to_user(frame.to_world(p))), 6.0 / self.view.scale);
        match hit {
            Some(i) if !self.sel.contains(&i) => {
                self.sel = match s.shapes[i].group {
                    Some(g) => s.members(g),
                    None => vec![i],
                };
                self.edit_points = false;
            }
            Some(_) => {}
            None => self.sel.clear(),
        }
    }

    /// The canvas's right-click menu: for the picked shapes, empty fabric
    /// in Artwork, or the design.
    pub(super) fn canvas_menu(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        if self.mode == Mode::Artwork && self.art.is_some() {
            if self.sel.is_empty() {
                self.fabric_menu(ui, canvas_rect);
            } else {
                self.shape_menu(ui);
            }
        } else {
            self.design_menu(ui, canvas_rect);
        }
    }

    /// Opens a page of the right dock.
    fn show_tab(&mut self, tab: Tab) {
        self.show_right = true;
        self.tab[self.mode as usize] = tab;
    }

    /// Everything that can be done to the picked shapes.
    pub(super) fn shape_menu(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.art else { return };
        let n = s.shapes.len();
        self.sel.retain(|&i| i < n);
        if self.sel.is_empty() {
            return;
        }
        let sel = self.sel.clone();
        let one = (sel.len() == 1).then(|| sel[0]);
        let muted = self.muted();
        // The one group the picked shapes make up, if they do.
        let group = s.shapes[sel[0]].group.filter(|&g| {
            let m = s.members(g);
            m.len() == sel.len() && m.iter().all(|i| sel.contains(i))
        });
        let in_group = sel.iter().any(|&i| s.shapes[i].group.is_some());
        let title = match (one, group) {
            (_, Some(g)) => s.groups.iter().find(|x| x.id == g).map_or(String::new(), |x| x.name.clone()),
            (Some(i), None) => s.shapes[i].name.clone(),
            (None, None) => tn!(sel.len(), "{n} shape", "{n} shapes"),
        };
        ui.label(RichText::new(title).strong().color(muted));
        ui.separator();

        let mut op: Option<Arrange> = None;
        let can_paste = !self.shape_clip.is_empty();
        if item(ui, true, t!("Copy"), "Ctrl+C") {
            self.remember_clip_groups();
            let ctx = ui.ctx().clone();
            self.copy_shapes(&ctx);
        }
        if item(ui, can_paste, t!("Paste"), "Ctrl+V") {
            self.paste_shapes();
            return;
        }
        for (text, keys, a) in [(t!("Duplicate"), "Ctrl+D", Arrange::Duplicate), (t!("Delete"), "Del", Arrange::Delete)] {
            if item(ui, true, text, keys) {
                op = Some(a);
            }
        }
        ui.separator();

        ui.menu_button(t!("Thread"), |ui| {
            let Some(s) = &mut self.art else { return };
            for l in 0..s.layers.len() {
                let thread = crate::formats::jef_thread(s.layers[l].thread, l);
                let on = sel.iter().all(|&i| s.shapes[i].layer == l);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter().rect_filled(r, 3, rgb(thread.color));
                    if ui.add(egui::Button::selectable(on, &thread.name)).clicked() {
                        for &i in &sel {
                            s.shapes[i].layer = l;
                        }
                        s.layers[l].enabled = true;
                        ui.close();
                    }
                });
            }
        });
        if let Some(i) = one
            && item(ui, true, t!("Use its thread for new shapes"), "")
        {
            self.tools.paint = self.art.as_ref().map(|s| s.shapes[i].layer);
        }
        ui.menu_button(t!("Stitch as"), |ui| {
            let Some(s) = &mut self.art else { return };
            let first = s.shapes[sel[0]].look;
            let mut look = first;
            ui.selectable_value(&mut look, None, t!("Thread's look"));
            for k in [FillKind::Auto, FillKind::Column, FillKind::Tatami, FillKind::Satin, FillKind::Contour, FillKind::Wave, FillKind::Open] {
                ui.selectable_value(&mut look, Some(Look::Fill(k)), crate::i18n::tr(crate::lettering::fill_name(k)));
            }
            ui.selectable_value(&mut look, Some(Look::Outline), t!("Outline"));
            ui.selectable_value(&mut look, Some(Look::Applique), t!("Appliqué"));
            ui.selectable_value(&mut look, Some(Look::Hole), t!("Leave fabric"));
            if look != first {
                for &i in &sel {
                    s.shapes[i].look = look;
                }
                ui.close();
            }
        });
        ui.separator();

        ui.menu_button(t!("Arrange"), |ui| {
            for (text, keys, a) in [
                (t!("Bring to front"), "Shift+]", Arrange::Front),
                (t!("Bring forward"), "]", Arrange::Forward),
                (t!("Send backward"), "[", Arrange::Backward),
                (t!("Send to back"), "Shift+[", Arrange::Back),
            ] {
                if item(ui, true, text, keys) {
                    op = Some(a);
                }
            }
        });
        ui.menu_button(t!("Align"), |ui| {
            for (text, a) in [
                (t!("Left edges"), Arrange::Left),
                (t!("Middles across"), Arrange::CenterX),
                (t!("Right edges"), Arrange::Right),
                (t!("Top edges"), Arrange::Top),
                (t!("Middles up and down"), Arrange::Middle),
                (t!("Bottom edges"), Arrange::Bottom),
            ] {
                if item(ui, true, text, "") {
                    op = Some(a);
                }
            }
            if sel.len() == 1 {
                ui.label(RichText::new(t!("One shape aligns to the drawing's frame.")).size(11.5).color(muted));
            }
        });
        let mut turn: Option<f32> = None;
        let mut flip: Option<bool> = None;
        let mut reset = false;
        ui.menu_button(t!("Transform"), |ui| {
            if item(ui, true, t!("Turn 90° right"), "") {
                turn = Some(90.0);
            }
            if item(ui, true, t!("Turn 90° left"), "") {
                turn = Some(-90.0);
            }
            if item(ui, true, t!("Flip left to right"), "") {
                flip = Some(true);
            }
            if item(ui, true, t!("Flip upside down"), "") {
                flip = Some(false);
            }
            if item(ui, true, t!("Reset move, size and turn"), "") {
                reset = true;
            }
        });
        ui.separator();

        // Grouping.
        if let Some(g) = group {
            if item(ui, true, t!("Rename group"), "") {
                self.renaming = Some(g);
                self.show_left = true;
                self.dock = Dock::Objects;
            }
            if item(ui, true, t!("Ungroup"), "Ctrl+Shift+G") {
                op = Some(Arrange::Ungroup);
            }
        } else {
            if item(ui, true, t!("Group"), "Ctrl+G") {
                op = Some(Arrange::Group);
            }
            if in_group && item(ui, true, t!("Ungroup"), "Ctrl+Shift+G") {
                op = Some(Arrange::Ungroup);
            }
        }
        let pinned = self.art.as_ref().is_some_and(|s| sel.iter().all(|&i| s.shapes[i].pinned));
        if self.art.as_ref().is_some_and(|s| s.group_colors)
            && item(ui, true, if pinned { t!("Unpin from its sewing place") } else { t!("Pin to its sewing place") }, "")
            && let Some(s) = &mut self.art
        {
            for &i in &sel {
                s.shapes[i].pinned = !pinned;
            }
        }
        ui.separator();

        // The one picked shape's outline and stitching.
        if let Some(i) = one {
            let (made, angles, entry) =
                self.art.as_ref().map(|s| (s.shapes[i].made.is_some(), !s.shapes[i].angles.is_empty(), s.shapes[i].entry.is_some())).unwrap_or_default();
            if made && item(ui, true, t!("Change its form…"), "") {
                self.show_tab(Tab::Shape);
            }
            if item(ui, true, t!("Edit points"), "") {
                self.set_pick(Pick::Points);
            }
            if item(ui, true, t!("Add stitch angle"), "") {
                self.set_pick(Pick::Angle);
            }
            if item(ui, true, t!("Set start point"), "") {
                self.set_pick(Pick::Start);
            }
            if (angles || entry)
                && item(ui, true, t!("Clear stitch angles and start point"), "")
                && let Some(s) = &mut self.art
            {
                s.shapes[i].angles.clear();
                s.shapes[i].entry = None;
            }
            ui.separator();
        }

        // Picking more or fewer.
        if item(ui, true, t!("Pick everything in this thread"), "")
            && let Some(s) = &self.art
        {
            let layers: Vec<usize> = sel.iter().map(|&i| s.shapes[i].layer).collect();
            self.sel = (0..s.shapes.len()).filter(|&i| !s.shapes[i].hidden && layers.contains(&s.shapes[i].layer)).collect();
        }
        if item(ui, true, t!("Pick the others instead"), "")
            && let Some(s) = &self.art
        {
            self.sel = (0..s.shapes.len()).filter(|&i| !s.shapes[i].hidden && !sel.contains(&i)).collect();
        }
        if item(ui, true, t!("Hide"), "Ctrl+H") {
            op = Some(Arrange::Hide);
        }
        if item(ui, true, t!("Hide the others"), "")
            && let Some(s) = &mut self.art
        {
            for (i, e) in s.shapes.iter_mut().enumerate() {
                e.hidden = !sel.contains(&i);
            }
        }
        if item(ui, true, t!("Show all shapes"), "") {
            op = Some(Arrange::ShowAll);
        }
        ui.separator();
        if item(ui, true, t!("Properties…"), "") {
            self.show_tab(Tab::Shape);
        }

        if let Some(a) = op {
            self.arrange(a);
        }
        if turn.is_some() || flip.is_some() || reset {
            let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
            for &i in &sel {
                let e = &mut s.shapes[i];
                if let Some(d) = turn {
                    e.rotate = (e.rotate + d + 180.0).rem_euclid(360.0) - 180.0;
                }
                if let Some(across) = flip {
                    flip_shape(art, e, across);
                }
                if reset {
                    (e.dx, e.dy, e.scale, e.rotate) = (0.0, 0.0, 1.0, 0.0);
                }
            }
        }
    }

    /// Right-click on empty fabric in Artwork.
    fn fabric_menu(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        if item(ui, !self.shape_clip.is_empty(), t!("Paste"), "Ctrl+V") {
            self.paste_shapes();
        }
        if item(ui, true, t!("Select all shapes"), "Ctrl+A") {
            self.arrange(Arrange::SelectAll);
        }
        if item(ui, true, t!("Show all shapes"), "") {
            self.arrange(Arrange::ShowAll);
        }
        ui.separator();
        let mut add = None;
        ui.menu_button(t!("Add shape"), |ui| {
            for name in artwork::BUILT_IN {
                if item(ui, true, crate::i18n::tr(name), "") {
                    add = Some(name);
                }
            }
        });
        if let Some(name) = add {
            self.add_built_in(name);
        }
        if item(ui, true, t!("Add a symbol…"), "") {
            self.show_left = true;
            self.dock = Dock::Symbols;
        }
        ui.menu_button(t!("Draw"), |ui| {
            for (text, keys, pick) in [
                (t!("Pen"), "B", Pick::Pen),
                (t!("Rectangle"), "U", Pick::Shape(Form::Rect)),
                (t!("Ellipse"), "", Pick::Shape(Form::Ellipse)),
                (t!("Polygon"), "", Pick::Shape(Form::Polygon)),
                (t!("Star"), "", Pick::Shape(Form::Star)),
                (t!("Line"), "W", Pick::Line),
            ] {
                if item(ui, true, text, keys) {
                    self.set_pick(pick);
                }
            }
        });
        ui.separator();
        self.view_items(ui, canvas_rect);
    }

    /// Right-click on the design in Viewer and Lettering.
    fn design_menu(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let has = self.design.is_some();
        if item(ui, has, t!("Rotate 90° right"), "R") {
            self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 1) % 4);
        }
        if item(ui, has, t!("Rotate 90° left"), "Shift+R") {
            self.change(|s| s.at.quarter_turns = (s.at.quarter_turns + 3) % 4);
        }
        if item(ui, has, t!("Mirror"), "M") {
            self.change(|s| s.at.mirror = !s.at.mirror);
        }
        if item(ui, has, t!("Center in hoop"), "C") {
            self.change(|s| {
                s.at.dx = 0.0;
                s.at.dy = 0.0;
            });
        }
        if item(ui, has, t!("Pin to hoop"), "P") {
            self.pin();
        }
        ui.separator();
        if item(ui, has, if self.playing { t!("Pause") } else { t!("Play the stitch-out") }, "Space") {
            self.toggle_play();
        }
        self.view_items(ui, canvas_rect);
    }

    fn view_items(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        if item(ui, true, t!("Show the whole hoop"), "F") {
            self.fit_view(canvas_rect, false);
        }
        if item(ui, self.design.is_some(), t!("Zoom to the design"), "D") {
            self.fit_view(canvas_rect, true);
        }
        if item(ui, true, if self.grid { t!("Hide the grid") } else { t!("Show the grid") }, "G") {
            self.grid = !self.grid;
        }
    }
}

/// Mirrors a shape in place, around its middle: left to right when
/// `across`, else upside down. Bakes its move first, so its points change.
fn flip_shape(art: &artwork::Artwork, e: &mut ShapeEdit, across: bool) {
    e.bake(art);
    let Some(own) = &mut e.own else { return };
    let (cx, cy) = artwork::middle(&own.lines);
    let f = |p: (f32, f32)| if across { (2.0 * cx - p.0, p.1) } else { (p.0, 2.0 * cy - p.1) };
    for (c, _) in &mut own.lines {
        for p in c.iter_mut() {
            *p = f(*p);
        }
    }
    e.angles = e.angles.iter().map(|&(a, b)| (f(a), f(b))).collect();
    e.entry = e.entry.map(f);
}
