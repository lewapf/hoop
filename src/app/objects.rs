//! The Objects list in the left dock, like a layers panel: every shape
//! topmost first, groups as folders, with a tool row, a search, eye and
//! lock per row and a small picture of each shape in its thread color.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{Color32, RichText, Sense, vec2};

use super::shapes::{Arrange, ListDrag, Row};
use super::*;
use crate::artwork::{self, Kind, ShapeEdit};

#[derive(Default)]
pub(super) struct ObjectsUi {
    filter: String,
    /// The shape whose name is being edited.
    rename: Option<usize>,
    /// The row under the pointer, outlined on the hoop.
    pub hover: Option<usize>,
    /// Row pictures by shape id, with the hash of what they show.
    thumbs: HashMap<u32, (u64, egui::TextureHandle)>,
}

const ROW_H: f32 = 30.0;
const THUMB: f32 = 22.0;

/// A shape drawn in its thread color, `px` square, for its row.
fn thumb_image(lines: &[(crate::stitch::Contour, bool)], kind: &Kind, color: [u8; 3], px: u32) -> Option<egui::ColorImage> {
    use resvg::tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};
    let b = artwork::extent(lines);
    if b.0 > b.2 {
        return None;
    }
    let pad = 2.0;
    let s = (px as f32 - 2.0 * pad) / (b.2 - b.0).max(b.3 - b.1).max(1e-3);
    let (ox, oy) = (px as f32 / 2.0 - (b.0 + b.2) / 2.0 * s, px as f32 / 2.0 - (b.1 + b.3) / 2.0 * s);
    let mut pb = PathBuilder::new();
    for (c, closed) in lines {
        for (k, p) in c.iter().enumerate() {
            let (x, y) = (p.0 * s + ox, p.1 * s + oy);
            if k == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
        }
        if *closed || matches!(kind, Kind::Fill { .. }) {
            pb.close();
        }
    }
    let path = pb.finish()?;
    let mut pm = Pixmap::new(px, px)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;
    match kind {
        Kind::Fill { even_odd } => {
            pm.fill_path(&path, &paint, if *even_odd { FillRule::EvenOdd } else { FillRule::Winding }, Transform::identity(), None);
        }
        Kind::Stroke { width } => {
            let stroke = Stroke { width: (width * s).max(1.5), line_cap: LineCap::Round, ..Default::default() };
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    let pixels = pm.pixels().iter().map(|p| Color32::from_rgba_premultiplied(p.red(), p.green(), p.blue(), p.alpha())).collect();
    Some(egui::ColorImage::new([px as usize, px as usize], pixels))
}

fn lines_hash(lines: &[(crate::stitch::Contour, bool)], kind: &Kind, color: [u8; 3]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    color.hash(&mut h);
    matches!(kind, Kind::Fill { even_odd: true }).hash(&mut h);
    if let Kind::Stroke { width } = kind {
        width.to_bits().hash(&mut h);
    }
    for (c, closed) in lines {
        closed.hash(&mut h);
        for p in c {
            (p.0.to_bits(), p.1.to_bits()).hash(&mut h);
        }
    }
    h.finish()
}

/// A small eye or lock toggle; returns the click.
fn toggle(ui: &mut egui::Ui, rect: egui::Rect, id: impl Hash + std::fmt::Debug, icon: &'static str, tint: Color32, tip: &str) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click()).on_hover_text(tip);
    if resp.hovered() {
        ui.painter().rect_filled(rect.expand(2.0), 4, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    icons::paint(ui, icon, rect, tint);
    resp
}

impl HoopApp {
    pub(super) fn objects_list(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        self.objects.hover = None;
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else {
            ui.label(RichText::new(t!("Open an SVG or a picture, or start from shapes, to see its parts here.")).size(12.0).color(muted));
            return;
        };
        let n = s.shapes.len();
        self.sel.retain(|&i| i < n);
        let picked = !self.sel.is_empty();
        let in_group = self.sel.iter().any(|&i| s.shapes[i].group.is_some());
        let whole_group = in_group && self.sel.iter().all(|&i| s.shapes[i].group.is_some() && s.shapes[i].group == s.shapes[self.sel[0]].group);

        // The tool row.
        let mut op: Option<Arrange> = None;
        let mut optimize = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let add = icons::icon_button(ui, true, false, "plus", t!("Add a shape"));
            egui::Popup::menu(&add).show(|ui| {
                for name in artwork::BUILT_IN {
                    if ui.button(crate::i18n::tr(name)).clicked() {
                        self.list_add = Some(name);
                    }
                }
            });
            if whole_group {
                if icons::button(ui, true, false, "ungroup", t!("Ungroup: turn the group back into loose shapes · Ctrl+Shift+G")) {
                    op = Some(Arrange::Ungroup);
                }
            } else if icons::button(ui, picked, false, "group", t!("Group the picked shapes, like a folder of layers · Ctrl+G")) {
                op = Some(Arrange::Group);
            }
            if icons::button(ui, picked, false, "copy", t!("Duplicate · Ctrl+D")) {
                op = Some(Arrange::Duplicate);
            }
            if icons::button(ui, picked, false, "trash", t!("Delete · Del")) {
                op = Some(Arrange::Delete);
            }
            ui.separator();
            if icons::button(ui, picked, false, "up", t!("Bring forward: sewn later, on top · ]")) {
                op = Some(Arrange::Forward);
            }
            if icons::button(ui, picked, false, "down", t!("Send backward: sewn earlier, below · [")) {
                op = Some(Arrange::Backward);
            }
            ui.separator();
            if icons::button(ui, n > 0, false, "wand", t!("Optimize: name shapes by thread, break apart compound fills, join touching lines, drop specks")) {
                optimize = true;
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let r =
                ui.add(egui::TextEdit::singleline(&mut self.objects.filter).hint_text(t!("Find a shape by name")).desired_width(ui.available_width() - 4.0));
            let icon = egui::Rect::from_center_size(egui::pos2(r.rect.right() - 12.0, r.rect.center().y), vec2(14.0, 14.0));
            if self.objects.filter.is_empty() {
                icons::paint(ui, "search", icon, muted);
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(tn!(n, "{n} shape · {picked} picked", "{n} shapes · {picked} picked", picked = self.sel.len())).size(12.0).color(muted));
            let pinned = s.shapes.iter().filter(|e| e.pinned).count();
            if s.group_colors
                && pinned > 0
                && ui
                    .small_button(t!("Unpin all"))
                    .on_hover_text(tn!(
                        pinned,
                        "{n} shape is sewn where you placed it. Let Hoop group it by thread again.",
                        "{n} shapes are sewn where you placed them. Let Hoop group them by thread again."
                    ))
                    .clicked()
            {
                for e in &mut s.shapes {
                    e.pinned = false;
                }
            }
        });
        ui.separator();
        if let Some(op) = op {
            self.arrange(op);
            return;
        }
        if optimize {
            self.optimize.open = true;
        }

        let mods = ui.input(|i| i.modifiers);
        let query = self.objects.filter.trim().to_lowercase();
        // Rows topmost first: a group's header, then its shapes (unless
        // folded). A search shows the matching shapes alone.
        let mut list: Vec<Row> = Vec::new();
        for i in (0..n).rev() {
            if !query.is_empty() {
                if s.shapes[i].name.to_lowercase().contains(&query) {
                    list.push(Row::Shape(i));
                }
                continue;
            }
            match s.shapes[i].group {
                Some(g) => {
                    if !list.iter().any(|r| matches!(r, Row::Head(h) if *h == g)) {
                        list.push(Row::Head(g));
                    }
                    if !self.folded.contains(&g) {
                        list.push(Row::Shape(i));
                    }
                }
                None => list.push(Row::Shape(i)),
            }
        }
        if list.is_empty() && !query.is_empty() {
            ui.label(RichText::new(t!("No shape is called that.")).color(muted));
        }

        let mut rows: Vec<(Row, egui::Rect)> = Vec::new();
        let mut menus: Vec<(Row, egui::Response)> = Vec::new();
        let mut clicked = None;
        let mut drag_from = None;
        let mut group_clicked: Option<u32> = None;
        let mut group_drag: Option<u32> = None;
        let mut fold: Option<u32> = None;
        let mut solo: Option<usize> = None;
        let mut hover = None;
        let ppp = ui.ctx().pixels_per_point();
        let thumbs = &mut self.objects.thumbs;
        let rename = &mut self.objects.rename;
        let v = ui.visuals().clone();
        let text = v.text_color();
        let fabric = self.fabric;
        egui::ScrollArea::vertical().id_salt("objects").auto_shrink([false, false]).show_rows(ui, ROW_H, list.len(), |ui, range| {
            let width = ui.available_width();
            for &row in &list[range] {
                let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click_and_drag());
                rows.push((row, rect));
                menus.push((row, resp.clone()));
                let row_hovered = resp.hovered();
                let fill_row = |ui: &egui::Ui, selected: bool| {
                    if selected {
                        ui.painter().rect_filled(rect.shrink2(vec2(0.0, 1.0)), 4, accent.gamma_multiply(0.25));
                    } else if row_hovered {
                        ui.painter().rect_filled(rect.shrink2(vec2(0.0, 1.0)), 4, v.widgets.hovered.weak_bg_fill);
                    }
                };
                let eye = egui::Rect::from_center_size(egui::pos2(rect.left() + 13.0, rect.center().y), vec2(16.0, 16.0));
                let lock = eye.translate(vec2(22.0, 0.0));
                let x0 = lock.right() + 8.0;

                if let Row::Head(g) = row {
                    let members = s.members(g);
                    fill_row(ui, !members.is_empty() && members.iter().all(|i| self.sel.contains(i)));
                    let all_hidden = members.iter().all(|&i| s.shapes[i].hidden);
                    let all_locked = members.iter().all(|&i| s.shapes[i].locked);
                    let ink = if all_hidden { muted.gamma_multiply(0.6) } else { text };
                    if toggle(ui, eye, ("geye", g), if all_hidden { "eye-off" } else { "eye" }, ink, t!("Show or hide the whole group")).clicked() {
                        for &i in &members {
                            s.shapes[i].hidden = !all_hidden;
                        }
                    }
                    if all_locked || row_hovered {
                        let tint = if all_locked { text } else { muted.gamma_multiply(0.6) };
                        if toggle(ui, lock, ("glock", g), if all_locked { "lock" } else { "unlock" }, tint, t!("Lock or unlock the whole group")).clicked() {
                            for &i in &members {
                                s.shapes[i].locked = !all_locked;
                            }
                        }
                    }
                    let open = !self.folded.contains(&g);
                    let caret = egui::Rect::from_center_size(egui::pos2(x0 + 8.0, rect.center().y), vec2(14.0, 14.0));
                    if toggle(ui, caret, ("fold", g), if open { "caret-down" } else { "caret-right" }, ink, t!("Fold or unfold the group")).clicked() {
                        fold = Some(g);
                    }
                    let folder = egui::Rect::from_center_size(egui::pos2(x0 + 30.0, rect.center().y), vec2(THUMB - 4.0, THUMB - 4.0));
                    icons::paint(ui, "folder", folder, accent);
                    let name = s.groups.iter().find(|x| x.id == g).map_or(String::new(), |x| x.name.clone());
                    let name_at = folder.right() + 8.0;
                    if self.renaming == Some(g) {
                        let field = egui::Rect::from_min_max(egui::pos2(name_at, rect.top() + 3.0), rect.max - vec2(6.0, 3.0));
                        let mut t = name.clone();
                        let edit = ui.put(field, egui::TextEdit::singleline(&mut t));
                        edit.request_focus();
                        if t != name
                            && let Some(x) = s.groups.iter_mut().find(|x| x.id == g)
                        {
                            x.name = t;
                        }
                        if edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.renaming = None;
                        }
                    } else {
                        ui.painter().text(egui::pos2(name_at, rect.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(13.5), ink);
                        let count = tn!(members.len(), "{n} shape", "{n} shapes");
                        ui.painter().text(rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, count, egui::FontId::proportional(11.0), muted);
                    }
                    let resp = resp.on_hover_text(t!("A group: click picks all of it, double-click renames it, drag moves it as one block"));
                    if resp.double_clicked() {
                        self.renaming = Some(g);
                    } else if resp.clicked() {
                        group_clicked = Some(g);
                    }
                    if resp.drag_started() {
                        group_drag = Some(g);
                    }
                    continue;
                }
                let Row::Shape(i) = row else { continue };
                if row_hovered {
                    hover = Some(i);
                }
                let e: &mut ShapeEdit = &mut s.shapes[i];
                fill_row(ui, self.sel.contains(&i));
                let ink = if e.hidden { muted.gamma_multiply(0.6) } else { text };
                let tip = t!("Show or hide this shape · Alt+click shows it alone");
                if toggle(ui, eye, ("eye", i), if e.hidden { "eye-off" } else { "eye" }, ink, tip).clicked() {
                    if mods.alt {
                        solo = Some(i);
                    } else {
                        e.hidden = !e.hidden;
                    }
                }
                if e.locked || row_hovered {
                    let tint = if e.locked { text } else { muted.gamma_multiply(0.6) };
                    let tip = if e.locked {
                        t!("Locked: can't be picked or moved on the hoop. Click to unlock.")
                    } else {
                        t!("Lock: keep it from being picked or moved on the hoop")
                    };
                    if toggle(ui, lock, ("lock", i), if e.locked { "lock" } else { "unlock" }, tint, tip).clicked() {
                        e.locked = !e.locked;
                    }
                }
                // Shapes in a group sit indented under its header.
                let indent = if e.group.is_some() && query.is_empty() { 16.0 } else { 0.0 };
                if indent > 0.0 {
                    ui.painter().vline(x0 + 7.0, rect.y_range(), egui::Stroke::new(1.0, muted.gamma_multiply(0.4)));
                }
                let thumb = egui::Rect::from_min_size(egui::pos2(x0 + indent, rect.center().y - THUMB / 2.0), vec2(THUMB, THUMB));
                // On the fabric, so black and white threads both show.
                ui.painter().rect_filled(thumb.expand(1.0), 4, rgb(fabric));
                ui.painter().rect_stroke(thumb.expand(1.0), 4, egui::Stroke::new(1.0, muted.gamma_multiply(0.5)), egui::StrokeKind::Outside);
                // The thread it sews in, which may differ from the drawing's color.
                let color = s.layers.get(e.layer).map_or(art.layers[e.layer].color, |l| crate::formats::jef_thread(l.thread, 0).color);
                let lines = e.lines(art);
                let kind = e.base(art).kind.clone();
                let key = lines_hash(&lines, &kind, color);
                let px = (THUMB * ppp).round() as u32;
                let tex = match thumbs.get(&e.id) {
                    Some((k, tex)) if *k == key => Some(tex.clone()),
                    _ => thumb_image(&lines, &kind, color, px).map(|img| {
                        let tex = ui.ctx().load_texture(format!("obj-{}", e.id), img, egui::TextureOptions::LINEAR);
                        thumbs.insert(e.id, (key, tex.clone()));
                        tex
                    }),
                };
                if let Some(tex) = tex {
                    let tint = if e.hidden { Color32::from_white_alpha(70) } else { Color32::WHITE };
                    ui.painter().image(tex.id(), thumb, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
                }
                let name_at = thumb.right() + 8.0;
                // Pin, then the tag, from the right.
                let mut right = rect.right() - 6.0;
                if s.group_colors {
                    let pin = egui::Rect::from_center_size(egui::pos2(rect.right() - 14.0, rect.center().y), vec2(14.0, 14.0));
                    let tip = if e.pinned {
                        t!("Placed by hand: sewn exactly here in the list. Click to let Hoop group it by thread again.")
                    } else {
                        t!("Pin: sew exactly here in the list instead of grouping it by thread")
                    };
                    let pin_resp = ui.interact(pin, ui.id().with(("pin", i)), Sense::click()).on_hover_text(tip);
                    if pin_resp.clicked() {
                        e.pinned = !e.pinned;
                    }
                    if e.pinned || row_hovered || pin_resp.hovered() {
                        icons::paint(ui, "pin", pin, if e.pinned { accent } else { muted.gamma_multiply(0.7) });
                    }
                    right -= 20.0;
                }
                let tag = if e.text.is_some() {
                    t!("text")
                } else if e.style.is_some() {
                    t!("own style")
                } else if e.source >= art.shapes.len() {
                    t!("added")
                } else if e.own.is_some() {
                    t!("edited")
                } else if e.moved() {
                    t!("moved")
                } else {
                    match kind {
                        Kind::Fill { .. } => t!("fill"),
                        Kind::Stroke { .. } => t!("line"),
                    }
                };
                if *rename == Some(i) {
                    let field = egui::Rect::from_min_max(egui::pos2(name_at, rect.top() + 3.0), egui::pos2(right, rect.bottom() - 3.0));
                    let edit = ui.put(field, egui::TextEdit::singleline(&mut e.name));
                    edit.request_focus();
                    if edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        *rename = None;
                    }
                } else {
                    let tag_g = ui.painter().layout_no_wrap(tag.to_string(), egui::FontId::proportional(11.0), muted);
                    let tag_w = tag_g.size().x;
                    ui.painter().galley(egui::pos2(right - tag_w, rect.center().y - tag_g.size().y / 2.0), tag_g, muted);
                    // The name gives way to the tag.
                    let room = (right - tag_w - 8.0 - name_at).max(10.0);
                    let name_g = ui.painter().layout(e.name.clone(), egui::FontId::proportional(13.5), ink, f32::INFINITY);
                    let clip = egui::Rect::from_min_max(egui::pos2(name_at, rect.top()), egui::pos2(name_at + room, rect.bottom()));
                    ui.painter().with_clip_rect(clip).galley(egui::pos2(name_at, rect.center().y - name_g.size().y / 2.0), name_g, ink);
                }
                let thread = s.layers.get(e.layer).map(|l| crate::formats::jef_thread(l.thread, 0));
                let resp = resp.on_hover_ui(|ui| {
                    ui.label(RichText::new(&e.name).strong());
                    if let Some(t) = &thread {
                        ui.label(format!("{} · {} {}", tag, t.code, t.name));
                    }
                    ui.label(RichText::new(t!("Double-click to rename · drag to restack")).size(11.0).color(muted));
                });
                if resp.double_clicked() {
                    *rename = Some(i);
                } else if resp.clicked() {
                    clicked = Some(i);
                }
                if resp.drag_started() {
                    drag_from = Some(i);
                }
            }
        });
        self.objects.hover = hover;
        if let Some(i) = solo {
            // Alt+click: show this shape alone, or everything again.
            let alone = (0..n).all(|k| s.shapes[k].hidden == (k != i));
            for (k, e) in s.shapes.iter_mut().enumerate() {
                e.hidden = !alone && k != i;
            }
        }
        if let Some(g) = fold
            && !self.folded.remove(&g)
        {
            self.folded.insert(g);
        }
        if let Some(i) = drag_from {
            self.list_drag = Some(i);
        }
        if let Some(g) = group_drag {
            self.group_drag = Some(g);
        }
        if let Some(g) = group_clicked {
            let members = s.members(g);
            if mods.command {
                let fresh: Vec<usize> = members.into_iter().filter(|i| !self.sel.contains(i)).collect();
                self.sel.extend(fresh);
            } else {
                self.select_only(None);
                self.sel = members;
            }
        }
        // While rows or a group are dragged, show where they land; drop on release.
        let dragging = self.list_drag.map(ListDrag::One).or(self.group_drag.map(ListDrag::Group));
        if let Some(what) = dragging
            && let Some(p) = ui.ctx().pointer_latest_pos()
        {
            let target = rows.iter().min_by(|a, b| (a.1.center().y - p.y).abs().total_cmp(&(b.1.center().y - p.y).abs())).copied();
            let released = !ui.input(|i| i.pointer.any_down());
            if let Some((row, r)) = target {
                // A shape dropped just under a group's header goes into the group.
                let into = matches!((what, row), (ListDrag::One(_), Row::Head(_))) && p.y >= r.center().y;
                let y = if p.y < r.center().y { r.top() } else { r.bottom() };
                let x = if into { r.left() + 34.0 } else { r.left() };
                ui.painter().hline(x..=r.right(), y, egui::Stroke::new(2.0, accent));
                if let ListDrag::One(i) = what
                    && self.sel.contains(&i)
                    && self.sel.len() > 1
                {
                    let tag = tn!(self.sel.len(), "{n} shape", "{n} shapes");
                    ui.painter().text(p + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, tag, egui::FontId::proportional(12.0), accent);
                }
                if released {
                    self.list_drag = None;
                    self.group_drag = None;
                    // Rows are topmost first, so above a row means later in stitch order.
                    let above = p.y < r.center().y;
                    self.drop_rows(what, row, above);
                    return;
                }
            } else if released {
                self.list_drag = None;
                self.group_drag = None;
            }
        }
        if let Some(i) = clicked {
            if mods.command {
                self.toggle_selected(i);
            } else if mods.shift
                && let Some(p) = self.primary()
            {
                let (a, b) = (p.min(i), p.max(i));
                self.sel = (a..=b).collect();
            } else {
                self.select_only(Some(i));
            }
        }
        // Right-click a row for the shape menu; it picks the row first.
        for (row, resp) in &menus {
            if resp.secondary_clicked() {
                let picked = match *row {
                    Row::Shape(i) => vec![i],
                    Row::Head(g) => self.art.as_ref().map_or(Vec::new(), |s| s.members(g)),
                };
                if !picked.iter().all(|i| self.sel.contains(i)) || matches!(row, Row::Head(_)) {
                    self.select_only(None);
                    self.sel = picked;
                }
            }
            resp.context_menu(|ui| self.shape_menu(ui));
        }
        if let Some(name) = self.list_add.take() {
            self.add_built_in(name);
        }
    }
}
