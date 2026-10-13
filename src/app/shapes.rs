//! Editing artwork shapes: selection, dragging with snap guides, resize and
//! turn handles, point editing, the Objects list and the Shape tab.

use super::*;
use crate::artwork::{Artwork, ShapeEdit};
use crate::lettering::FillKind;

/// Artwork user units to stitch units to hoop coordinates and back.
#[derive(Clone, Copy)]
pub struct ShapeMap {
    pub scale: f32,
    pub mid: (f32, f32),
    pub t: Transform,
}

impl ShapeMap {
    pub fn to_stitch(self, u: (f32, f32)) -> (f32, f32) {
        ((u.0 - self.mid.0) * self.scale, (u.1 - self.mid.1) * self.scale)
    }

    pub fn to_world(self, u: (f32, f32)) -> (f32, f32) {
        self.t.apply(self.to_stitch(u))
    }

    pub fn to_user(self, w: (f32, f32)) -> (f32, f32) {
        let p = self.t.invert(w);
        (p.0 / self.scale + self.mid.0, p.1 / self.scale + self.mid.1)
    }
}

/// A row of the Objects list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Row {
    Head(u32),
    Shape(usize),
}

/// What is being dragged in the Objects list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ListDrag {
    One(usize),
    Group(u32),
}

#[derive(Clone, Copy)]
pub(super) enum Handle {
    Corner,
    Turn,
}

/// The selected shape's box on screen and the knob above it for turning.
pub(super) struct Handles {
    pub rect: egui::Rect,
    pub turn: egui::Pos2,
}

impl Handles {
    pub fn corners(&self) -> [egui::Pos2; 4] {
        [self.rect.left_top(), self.rect.right_top(), self.rect.right_bottom(), self.rect.left_bottom()]
    }

    pub fn hit(&self, p: egui::Pos2) -> Option<Handle> {
        if (self.turn - p).length() <= 9.0 {
            return Some(Handle::Turn);
        }
        self.corners().iter().any(|c| (*c - p).length() <= 9.0).then_some(Handle::Corner)
    }
}

fn screen_box(frame: &Frame, m: ShapeMap, art: &Artwork, e: &ShapeEdit) -> Option<egui::Rect> {
    let pts: Vec<egui::Pos2> = e.lines(art).iter().flat_map(|l| l.0.iter()).map(|&q| frame.to_screen_pt(m.to_world(q))).collect();
    (!pts.is_empty()).then(|| egui::Rect::from_points(&pts))
}

pub(super) fn shape_handles(frame: &Frame, m: ShapeMap, art: &Artwork, e: &ShapeEdit) -> Option<Handles> {
    // Keep tiny shapes grabbable: the box is at least 24 px across.
    let rect = screen_box(frame, m, art, e)?.expand(4.0);
    let rect = egui::Rect::from_center_size(rect.center(), rect.size().max(vec2(24.0, 24.0)));
    Some(Handles { rect, turn: rect.center_top() - vec2(0.0, 24.0) })
}

/// Where a shape turns and scales around, in user units.
fn pivot(art: &Artwork, e: &ShapeEdit) -> (f32, f32) {
    let c = artwork::middle(&e.base(art).lines);
    (c.0 + e.dx, c.1 + e.dy)
}

fn union(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

const EMPTY: (f32, f32, f32, f32) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);

/// Things that can be done to the selected shapes, from menus, keys and buttons.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Arrange {
    SelectAll,
    Deselect,
    Duplicate,
    Delete,
    Hide,
    ShowAll,
    Front,
    Back,
    Forward,
    Backward,
    Left,
    CenterX,
    Right,
    Top,
    Middle,
    Bottom,
    Group,
    Ungroup,
}

impl HoopApp {
    /// How artwork user units map onto the hoop, when an artwork design is shown.
    pub(super) fn shape_map(&self) -> Option<ShapeMap> {
        if self.mode != Mode::Artwork || self.playing {
            return None;
        }
        let (art, s, d) = (self.artwork.as_ref()?, self.art.as_ref()?, self.design.as_ref()?);
        let (scale, mid) = artwork::frame(art, s);
        Some(ShapeMap { scale, mid, t: Transform::new(d.bounds, &self.setup.at) })
    }

    /// The shape the panels edit: the last one picked.
    pub(super) fn primary(&self) -> Option<usize> {
        let n = self.art.as_ref().map_or(0, |a| a.shapes.len());
        self.sel.last().copied().filter(|&i| i < n)
    }

    pub(super) fn select_only(&mut self, i: Option<usize>) {
        if self.sel.len() != 1 || self.sel.first().copied() != i {
            self.edit_points = false;
        }
        self.sel = i.into_iter().collect();
    }

    pub(super) fn toggle_selected(&mut self, i: usize) {
        match self.sel.iter().position(|&k| k == i) {
            Some(at) => {
                self.sel.remove(at);
            }
            None => self.sel.push(i),
        }
        self.edit_points = false;
    }

    /// Applies a new stacking order (old indices, bottom first) and keeps the
    /// same shapes selected. The selected shapes were placed by hand, so they
    /// are pinned: sewn where they now sit, like layers.
    fn restack(&mut self, order: Vec<usize>) {
        self.reorder(&order);
        self.keep_groups_together();
        let Some(s) = &mut self.art else { return };
        for &i in &self.sel {
            s.shapes[i].pinned = true;
        }
    }

    /// Applies a new stacking order (old indices, bottom first), keeping
    /// the same shapes selected.
    fn reorder(&mut self, order: &[usize]) {
        let Some(s) = &mut self.art else { return };
        let old = std::mem::take(&mut s.shapes);
        s.shapes = order.iter().map(|&i| old[i].clone()).collect();
        self.sel = self.sel.iter().filter_map(|i| order.iter().position(|o| o == i)).collect();
    }

    /// Brings each group's shapes back next to each other (after a move,
    /// duplicate or optimize split them) and forgets empty groups.
    pub(super) fn keep_groups_together(&mut self) {
        let Some(s) = &mut self.art else { return };
        if s.groups.is_empty() && s.shapes.iter().all(|e| e.group.is_none()) {
            return;
        }
        s.prune_groups();
        if let Some(order) = s.gathered() {
            self.reorder(&order);
        }
    }

    /// Drops a dragged row or group in the Objects list next to `row`,
    /// above or below it on screen. A shape joins the group it lands in, or
    /// leaves its group when it lands outside; a group moves as one block
    /// and never lands inside another.
    pub(super) fn drop_rows(&mut self, what: ListDrag, row: Row, above: bool) {
        let Some(s) = &mut self.art else { return };
        let n = s.shapes.len();
        // A row that is one of several picked carries them all along.
        let moving: Vec<usize> = match what {
            ListDrag::One(i) if self.sel.contains(&i) && self.sel.len() > 1 => {
                let mut v = self.sel.clone();
                v.sort();
                v
            }
            ListDrag::One(i) => vec![i],
            ListDrag::Group(g) => s.members(g),
        };
        let span = |g: u32| {
            let m = s.members(g);
            (m.first().copied().unwrap_or(0), m.last().copied().unwrap_or(0))
        };
        // The shape to land next to, whether after it in stitch order, and the group to join.
        let (anchor, after, join) = match (what, row) {
            (ListDrag::One(_), Row::Shape(t)) => (t, above, s.shapes[t].group),
            (ListDrag::One(_), Row::Head(h)) => (span(h).1, true, (!above).then_some(h)),
            (ListDrag::Group(g), Row::Head(h)) if g == h => return,
            (ListDrag::Group(g), Row::Shape(t)) => match s.shapes[t].group {
                Some(h) if h == g => return,
                Some(h) => {
                    let (lo, hi) = span(h);
                    if above { (hi, true, None) } else { (lo, false, None) }
                }
                None => (t, above, None),
            },
            (ListDrag::Group(_), Row::Head(h)) => {
                let (lo, hi) = span(h);
                if above { (hi, true, None) } else { (lo, false, None) }
            }
        };
        if moving.len() > 1 && moving.contains(&anchor) && matches!(what, ListDrag::One(_)) {
            return;
        }
        if moving.contains(&anchor) && moving.len() == 1 {
            // Dropped on itself: only its group may change.
            if let ListDrag::One(i) = what
                && s.shapes[i].group != join
            {
                s.shapes[i].group = join;
            }
            return;
        }
        if let ListDrag::One(_) = what {
            for &i in &moving {
                s.shapes[i].group = join;
            }
        }
        let mut order: Vec<usize> = (0..n).filter(|k| !moving.contains(k)).collect();
        let at = order.iter().position(|&k| k == anchor).map_or(order.len(), |k| if after { k + 1 } else { k });
        order.splice(at..at, moving.iter().copied());
        self.sel = moving;
        match what {
            ListDrag::One(_) => self.restack(order),
            ListDrag::Group(_) => {
                self.reorder(&order);
                self.keep_groups_together();
            }
        }
    }

    /// A shape with the rest of its group, as a click on the hoop picks it.
    fn with_group(&self, i: usize) -> Vec<usize> {
        match self.art.as_ref().and_then(|s| Some((s, s.shapes.get(i)?.group?))) {
            Some((s, g)) => s.members(g),
            None => vec![i],
        }
    }

    pub(super) fn arrange(&mut self, op: Arrange) {
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
        let n = s.shapes.len();
        self.sel.retain(|&i| i < n);
        let picked = |i: usize, sel: &[usize]| sel.contains(&i);
        match op {
            Arrange::SelectAll => {
                self.sel = (0..n).filter(|&i| !s.shapes[i].hidden).collect();
                self.edit_points = false;
            }
            Arrange::Deselect => self.select_only(None),
            Arrange::ShowAll => {
                for e in &mut s.shapes {
                    e.hidden = false;
                }
            }
            _ if self.sel.is_empty() => {}
            Arrange::Duplicate => {
                // 5 mm to the right and down, on top of everything.
                let (scale, _) = artwork::frame(art, s);
                let mut sel: Vec<usize> = self.sel.clone();
                sel.sort_unstable();
                let copies = sel
                    .into_iter()
                    .map(|i| {
                        let mut copy = s.shapes[i].clone();
                        copy.dx += 50.0 / scale;
                        copy.dy += 50.0 / scale;
                        copy.name = t!("{name} copy", name = copy.name);
                        copy
                    })
                    .collect();
                self.sel = s.add_copies(copies).collect();
                self.edit_points = false;
            }
            Arrange::Delete => {
                let mut sel = std::mem::take(&mut self.sel);
                sel.sort_unstable();
                sel.dedup();
                for i in sel.into_iter().rev() {
                    s.shapes.remove(i);
                }
                self.edit_points = false;
            }
            Arrange::Hide => {
                for &i in &self.sel {
                    s.shapes[i].hidden = true;
                }
                self.sel.clear();
            }
            Arrange::Group => {
                // The picked shapes, stacked together where the topmost one is.
                let id = s.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
                let n_groups = s.groups.len() + 1;
                s.groups.push(artwork::Group { id, name: t!("Group {n}", n = n_groups) });
                let top = self.sel.iter().copied().max().unwrap_or(0);
                for &i in &self.sel {
                    s.shapes[i].group = Some(id);
                }
                let (mine, rest): (Vec<usize>, Vec<usize>) = (0..n).partition(|&i| picked(i, &self.sel));
                let at = rest.iter().filter(|&&i| i < top).count();
                let mut order = rest;
                order.splice(at..at, mine);
                self.reorder(&order);
                self.keep_groups_together();
            }
            Arrange::Ungroup => {
                let ids: Vec<u32> = self.sel.iter().filter_map(|&i| s.shapes[i].group).collect();
                for e in &mut s.shapes {
                    if e.group.is_some_and(|g| ids.contains(&g)) {
                        e.group = None;
                    }
                }
                s.prune_groups();
            }
            Arrange::Front | Arrange::Back => {
                let (mut mine, rest): (Vec<usize>, Vec<usize>) = (0..n).partition(|&i| picked(i, &self.sel));
                let order = if op == Arrange::Front {
                    rest.into_iter().chain(mine).collect()
                } else {
                    mine.extend(rest);
                    mine
                };
                self.restack(order);
            }
            Arrange::Forward | Arrange::Backward => {
                let mut order: Vec<usize> = (0..n).collect();
                if op == Arrange::Forward {
                    for k in (0..n.saturating_sub(1)).rev() {
                        if picked(order[k], &self.sel) && !picked(order[k + 1], &self.sel) {
                            order.swap(k, k + 1);
                        }
                    }
                } else {
                    for k in 1..n {
                        if picked(order[k], &self.sel) && !picked(order[k - 1], &self.sel) {
                            order.swap(k, k - 1);
                        }
                    }
                }
                self.restack(order);
            }
            _ => {
                // Align to the selection's box, or a single shape to the drawing's frame.
                let boxes: Vec<(usize, (f32, f32, f32, f32))> = self.sel.iter().map(|&i| (i, artwork::extent(&s.shapes[i].lines(art)))).collect();
                let target = if boxes.len() > 1 { boxes.iter().fold(EMPTY, |a, b| union(a, b.1)) } else { art.bounds };
                for (i, b) in boxes {
                    let e = &mut s.shapes[i];
                    match op {
                        Arrange::Left => e.dx += target.0 - b.0,
                        Arrange::Right => e.dx += target.2 - b.2,
                        Arrange::CenterX => e.dx += (target.0 + target.2 - b.0 - b.2) / 2.0,
                        Arrange::Top => e.dy += target.1 - b.1,
                        Arrange::Bottom => e.dy += target.3 - b.3,
                        Arrange::Middle => e.dy += (target.1 + target.3 - b.1 - b.3) / 2.0,
                        _ => {}
                    }
                }
            }
        }
    }

    pub(super) fn add_built_in(&mut self, name: &str) {
        self.add_shapes(&crate::i18n::tr_any(name), |at, size, layer| vec![artwork::built_in(name, at, size, layer)]);
    }

    /// Adds a symbol to the drawing, half the frame across, in the middle.
    pub(super) fn add_symbol(&mut self, sym: &crate::symbols::Symbol, svg: &[u8]) -> Result<(), String> {
        let drawn = artwork::parse(sym.file, svg)?;
        let b = drawn.bounds;
        let (mx, my) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
        let across = (b.2 - b.0).max(b.3 - b.1).max(1e-3);
        // A frame goes round what is drawn already, with room to spare.
        let around = (sym.category == crate::symbols::Category::Frames)
            .then(|| {
                let (art, s) = (self.artwork.as_ref()?, self.art.as_ref()?);
                let on = |e: &&ShapeEdit| !e.hidden && s.layers.get(e.layer).is_none_or(|l| l.enabled);
                let box_ = s.shapes.iter().filter(on).fold(EMPTY, |a, e| union(a, artwork::extent(&e.lines(art))));
                (box_.0 < box_.2).then(|| (((box_.0 + box_.2) / 2.0, (box_.1 + box_.3) / 2.0), (box_.2 - box_.0).max(box_.3 - box_.1) * 1.35))
            })
            .flatten();
        self.add_shapes(crate::i18n::tr(sym.name), |at, size, layer| {
            let (at, size) = around.unwrap_or((at, size));
            let k = size / across;
            drawn
                .shapes
                .into_iter()
                .map(|mut sh| {
                    for (c, _) in &mut sh.lines {
                        for p in c.iter_mut() {
                            *p = (at.0 + (p.0 - mx) * k, at.1 + (p.1 - my) * k);
                        }
                    }
                    if let artwork::Kind::Stroke { width } = &mut sh.kind {
                        *width *= k;
                    }
                    sh.layer = layer;
                    sh
                })
                .collect()
        });
        Ok(())
    }

    /// The thread new shapes get: the one picked in the tool palette, else
    /// the picked shape's.
    pub(super) fn paint_layer(&self) -> usize {
        let n = self.art.as_ref().map_or(0, |s| s.layers.len());
        self.tools.paint.filter(|&l| l < n).or_else(|| self.primary().and_then(|i| self.art.as_ref()?.shapes.get(i)).map(|e| e.layer)).unwrap_or(0)
    }

    /// Adds new shapes on the current thread, made by `make` from the
    /// middle of the drawing, a size and the thread, and picks them.
    fn add_shapes(&mut self, name: &str, make: impl FnOnce(crate::lettering::Pt, f32, usize) -> Vec<artwork::Shape>) {
        let layer = self.paint_layer();
        let Some(art) = &self.artwork else { return };
        let size = (art.bounds.2 - art.bounds.0).min(art.bounds.3 - art.bounds.1) * 0.5;
        let mid = ((art.bounds.0 + art.bounds.2) / 2.0, (art.bounds.1 + art.bounds.3) / 2.0);
        let first = self.art.as_ref().map_or(0, |s| s.shapes.len());
        for own in make(mid, size, layer) {
            self.push_shape(name, own, None);
        }
        let n = self.art.as_ref().map_or(0, |s| s.shapes.len());
        self.sel = (first..n).collect();
    }

    /// Adds one shape of its own on top, numbered after others of the same
    /// name, and picks it.
    pub(super) fn push_shape(&mut self, name: &str, own: artwork::Shape, made: Option<artwork::Made>) {
        let Some(s) = &mut self.art else { return };
        let layer = own.layer;
        let n = s.shapes.iter().filter(|e| e.name.starts_with(name)).count();
        let id = s.fresh_id();
        s.shapes.push(ShapeEdit {
            source: usize::MAX,
            own: Some(own),
            name: if n == 0 { name.to_string() } else { format!("{name} {}", n + 1) },
            layer,
            look: None,
            hidden: false,
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            rotate: 0.0,
            angles: Vec::new(),
            entry: None,
            style: None,
            pinned: false,
            made,
            text: None,
            group: None,
            id,
            locked: false,
        });
        if let Some(l) = s.layers.get_mut(layer) {
            l.enabled = true;
        }
        self.sel = vec![s.shapes.len() - 1];
        self.edit_points = false;
    }

    /// Mouse on the canvas for artwork shapes. Returns true when a double
    /// click was used up (adding a point), so it doesn't also refit the view.
    pub(super) fn shape_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        let Some(m) = self.shape_map() else {
            self.guides.clear();
            return false;
        };
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return false };
        self.sel.retain(|&i| i < s.shapes.len());
        let pointer = response.interact_pointer_pos();
        let user = pointer.map(|p| m.to_user(frame.to_world(p)));
        let slop = 6.0 / self.view.scale;
        let (origin, mods, middle) = ui.input(|i| (i.pointer.press_origin(), i.modifiers, i.pointer.button_down(egui::PointerButton::Middle)));
        let single = (self.sel.len() == 1).then(|| self.sel[0]);
        let node_under = |s: &ArtSettings, p: egui::Pos2| -> Option<(usize, usize)> {
            let e = s.shapes.get(single?)?;
            let mut best = (64.0f32, None);
            for (li, (c, _)) in e.lines(art).iter().enumerate() {
                for (pi, &q) in c.iter().enumerate() {
                    let d = (frame.to_screen_pt(m.to_world(q)) - p).length_sq();
                    if d < best.0 {
                        best = (d, Some((li, pi)));
                    }
                }
            }
            best.1
        };
        let mut handled_double = false;

        if response.clicked()
            && let (Some(u), Some(p)) = (user, pointer)
            && !(self.edit_points && node_under(s, p).is_some())
        {
            let hit = artwork::shape_at(art, s, m.to_stitch(u), slop);
            // A shape in a group picks the whole group; Alt picks just the shape.
            let one = mods.alt;
            match hit {
                Some(i) if mods.command => {
                    for k in if one { vec![i] } else { self.with_group(i) } {
                        self.toggle_selected(k);
                    }
                }
                None if mods.command => {}
                Some(i) if !one => {
                    self.select_only(Some(i));
                    self.sel = self.with_group(i);
                }
                _ => self.select_only(hit),
            }
            return false;
        }
        let Some(s) = &mut self.art else { return false };
        if self.edit_points
            && response.secondary_clicked()
            && let (Some(i), Some((li, pi))) = (single, pointer.and_then(|p| node_under(s, p)))
        {
            let e = &mut s.shapes[i];
            e.bake(art);
            if let Some(own) = &mut e.own {
                let (c, closed) = &mut own.lines[li];
                if c.len() > if *closed { 3 } else { 2 } {
                    c.remove(pi);
                }
            }
        }
        if self.edit_points
            && response.double_clicked()
            && let (Some(i), Some(u)) = (single, user)
        {
            let e = &mut s.shapes[i];
            e.bake(art);
            if let Some(own) = &mut e.own {
                // Insert on the nearest segment, if the click was on the outline.
                let mut best = (slop / m.scale, None);
                for (li, (c, closed)) in own.lines.iter().enumerate() {
                    let segs = if *closed { c.len() } else { c.len().saturating_sub(1) };
                    for k in 0..segs {
                        let d = artwork::near_line(&vec![c[k], c[(k + 1) % c.len()]], false, u);
                        if d < best.0 {
                            best = (d, Some((li, k + 1)));
                        }
                    }
                }
                if let Some((li, at)) = best.1 {
                    own.lines[li].0.insert(at, u);
                    handled_double = true;
                }
            }
        }

        // Drags start where the button went down, not where the pointer is
        // once it has moved far enough to count as a drag. Shift+drag and
        // the middle button are left to the design move and the pan.
        if response.drag_started()
            && !mods.shift
            && !middle
            && let Some(p) = origin
        {
            let u = m.to_user(frame.to_world(p));
            let handle =
                single.and_then(|i| s.shapes.get(i)).filter(|_| !self.edit_points).and_then(|e| shape_handles(frame, m, art, e)).and_then(|h| h.hit(p));
            if let (Some(i), Some(h)) = (single, handle) {
                let e = &s.shapes[i];
                let c = pivot(art, e);
                self.drag = Some(match h {
                    Handle::Corner => Drag::Scale { from: e.scale, start: lettering::dist(c, u).max(1e-3) },
                    Handle::Turn => Drag::Turn { from: e.rotate, start: (u.1 - c.1).atan2(u.0 - c.0).to_degrees() },
                });
            } else if let Some((line, point)) = node_under(s, p).filter(|_| self.edit_points) {
                if let Some(i) = single {
                    s.shapes[i].bake(art);
                    self.drag = Some(Drag::Node { line, point });
                }
            } else if let Some(i) = artwork::shape_at(art, s, m.to_stitch(u), slop) {
                // Press on any shape and drag: it is picked and moved in one
                // go. Pressing on one of several selected moves them all.
                if !self.sel.contains(&i) {
                    let picked = match s.shapes[i].group {
                        Some(g) if !mods.alt => s.members(g),
                        _ => vec![i],
                    };
                    if mods.command {
                        self.sel.extend(picked);
                    } else {
                        self.sel = picked;
                    }
                    self.edit_points = false;
                }
                let from: Vec<(usize, (f32, f32))> = self.sel.iter().map(|&k| (k, (s.shapes[k].dx, s.shapes[k].dy))).collect();
                let moving = self.sel.iter().fold(EMPTY, |a, &k| union(a, artwork::extent(&s.shapes[k].lines(art))));
                // Edges and middles of everything else, and the frame, to snap to.
                let mut xs = vec![(art.bounds.0 + art.bounds.2) / 2.0];
                let mut ys = vec![(art.bounds.1 + art.bounds.3) / 2.0];
                for (k, e) in s.shapes.iter().enumerate() {
                    if self.sel.contains(&k) || e.hidden {
                        continue;
                    }
                    let b = artwork::extent(&e.lines(art));
                    if b.0 <= b.2 {
                        xs.extend([b.0, (b.0 + b.2) / 2.0, b.2]);
                        ys.extend([b.1, (b.1 + b.3) / 2.0, b.3]);
                    }
                }
                self.drag = Some(Drag::Shape { start: u, from, moving, snap: (xs, ys) });
            } else {
                self.drag = Some(Drag::Band { start: p, add: mods.command });
            }
        }

        if response.dragged()
            && let Some(u) = user
        {
            match &self.drag {
                Some(Drag::Shape { start, from, moving, snap }) => {
                    let (mut dx, mut dy) = (u.0 - start.0, u.1 - start.1);
                    self.guides.clear();
                    // Snap to other shapes within 6 px (Alt turns it off),
                    // else to 0.5 mm on the fabric.
                    let near = 6.0 / (self.view.scale * m.scale);
                    let grid = 5.0 / m.scale;
                    let mut fit = |d: &mut f32, edges: [f32; 3], targets: &[f32], vertical: bool| {
                        let shift = *d;
                        let best =
                            edges.iter().flat_map(|&e| targets.iter().map(move |&t| (t - (e + shift), t))).min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
                        match best {
                            Some((gap, t)) if !mods.alt && gap.abs() <= near => {
                                *d += gap;
                                self.guides.push((vertical, t));
                            }
                            _ => *d = (*d / grid).round() * grid,
                        }
                    };
                    fit(&mut dx, [moving.0, (moving.0 + moving.2) / 2.0, moving.2], &snap.0, true);
                    fit(&mut dy, [moving.1, (moving.1 + moving.3) / 2.0, moving.3], &snap.1, false);
                    for &(k, (fx, fy)) in from {
                        if let Some(e) = s.shapes.get_mut(k) {
                            (e.dx, e.dy) = (fx + dx, fy + dy);
                        }
                    }
                }
                Some(Drag::Scale { from, start }) => {
                    if let Some(i) = single {
                        let k = from * lettering::dist(pivot(art, &s.shapes[i]), u) / start;
                        // Whole percents read better in the panel.
                        s.shapes[i].scale = (k * 100.0).round().clamp(10.0, 400.0) / 100.0;
                    }
                }
                Some(Drag::Turn { from, start }) => {
                    if let Some(i) = single {
                        let c = pivot(art, &s.shapes[i]);
                        let a = (u.1 - c.1).atan2(u.0 - c.0).to_degrees();
                        let r = (from + a - start + 180.0).rem_euclid(360.0) - 180.0;
                        // Shift snaps to 15°.
                        s.shapes[i].rotate = if mods.shift { (r / 15.0).round() * 15.0 } else { r.round() };
                    }
                }
                Some(Drag::Node { line, point }) => {
                    if let Some(i) = single
                        && let Some(own) = &mut s.shapes[i].own
                        && let Some(q) = own.lines.get_mut(*line).and_then(|l| l.0.get_mut(*point))
                    {
                        *q = u;
                    }
                }
                _ => {}
            }
        }

        if response.drag_stopped() {
            self.guides.clear();
            if let (Some(Drag::Band { start, add }), Some(end)) = (&self.drag, pointer) {
                let band = egui::Rect::from_two_pos(*start, end);
                let mut picked: Vec<usize> = if *add { self.sel.clone() } else { Vec::new() };
                for (i, e) in s.shapes.iter().enumerate() {
                    let on = !e.hidden && !e.locked && s.layers.get(e.layer).is_none_or(|l| l.enabled);
                    if on && !picked.contains(&i) && screen_box(frame, m, art, e).is_some_and(|b| b.intersects(band)) {
                        picked.push(i);
                    }
                }
                self.sel = picked;
                self.edit_points = false;
            }
        }
        handled_double
    }

    /// The shape under the pointer, with the cursor set to what a press would do.
    pub(super) fn shape_hover(&self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> Option<usize> {
        let (m, art, s) = (self.shape_map()?, self.artwork.as_ref()?, self.art.as_ref()?);
        let cursor = |c| ui.ctx().set_cursor_icon(c);
        match self.drag {
            Some(Drag::Shape { .. } | Drag::Node { .. }) => cursor(egui::CursorIcon::Grabbing),
            Some(Drag::Scale { .. }) => cursor(egui::CursorIcon::ResizeNwSe),
            Some(Drag::Turn { .. }) => cursor(egui::CursorIcon::Alias),
            Some(Drag::Band { .. }) => cursor(egui::CursorIcon::Crosshair),
            Some(_) => {}
            None => {
                let p = response.hover_pos()?;
                let single = (self.sel.len() == 1).then(|| self.sel[0]);
                let handle =
                    single.and_then(|i| s.shapes.get(i)).filter(|_| !self.edit_points).and_then(|e| shape_handles(frame, m, art, e)).and_then(|h| h.hit(p));
                match handle {
                    Some(Handle::Corner) => cursor(egui::CursorIcon::ResizeNwSe),
                    Some(Handle::Turn) => cursor(egui::CursorIcon::Alias),
                    None if !ui.input(|i| i.modifiers.shift) => {
                        let hit = artwork::shape_at(art, s, m.to_stitch(m.to_user(frame.to_world(p))), 6.0 / self.view.scale);
                        if hit.is_some() {
                            cursor(egui::CursorIcon::Grab);
                        }
                        return hit;
                    }
                    None => {}
                }
            }
        }
        None
    }

    /// Outlines of the hovered and selected shapes, handles, points, snap
    /// guides and the selection band.
    pub(super) fn shape_overlay(&self, painter: &egui::Painter, frame: &Frame, hover: Option<usize>) {
        let (Some(m), Some(art), Some(s)) = (self.shape_map(), &self.artwork, &self.art) else { return };
        let accent = self.accent();
        let outline = |e: &ShapeEdit, stroke: egui::Stroke, halo: bool| {
            for (c, closed) in &e.lines(art) {
                let mut pts: Vec<egui::Pos2> = c.iter().map(|&q| frame.to_screen_pt(m.to_world(q))).collect();
                if *closed && !pts.is_empty() {
                    pts.push(pts[0]);
                }
                if halo {
                    painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(stroke.width + 1.5, Color32::from_black_alpha(120))));
                }
                painter.add(egui::Shape::line(pts, stroke));
            }
        };
        if let Some(h) = hover.filter(|h| !self.sel.contains(h)).and_then(|h| s.shapes.get(h)) {
            outline(h, egui::Stroke::new(1.0, Color32::from_white_alpha(170)), false);
        }
        for &i in &self.sel {
            if let Some(e) = s.shapes.get(i) {
                outline(e, egui::Stroke::new(1.5, accent), true);
            }
        }
        let dash = egui::Stroke::new(1.0, accent.gamma_multiply(0.8));
        let dashed_rect = |r: egui::Rect| {
            painter.add(egui::Shape::dashed_line(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()], dash, 4.0, 3.0));
        };
        if let [i] = self.sel[..]
            && let Some(e) = s.shapes.get(i)
        {
            let count: usize = e.base(art).lines.iter().map(|l| l.0.len()).sum();
            if !self.edit_points {
                if let Some(h) = shape_handles(frame, m, art, e) {
                    dashed_rect(h.rect);
                    painter.line_segment([h.rect.center_top(), h.turn], dash);
                    for c in h.corners() {
                        let r = egui::Rect::from_center_size(c, vec2(9.0, 9.0));
                        painter.rect_filled(r, 2, Color32::WHITE);
                        painter.rect_stroke(r, 2, egui::Stroke::new(1.5, accent), egui::StrokeKind::Middle);
                    }
                    painter.circle(h.turn, 5.5, Color32::WHITE, egui::Stroke::new(1.5, accent));
                }
            } else if count <= 800 {
                for q in e.lines(art).iter().flat_map(|l| l.0.clone()) {
                    let r = egui::Rect::from_center_size(frame.to_screen_pt(m.to_world(q)), vec2(7.0, 7.0));
                    painter.rect_filled(r, 1, Color32::WHITE);
                    painter.rect_stroke(r, 1, egui::Stroke::new(1.5, accent), egui::StrokeKind::Middle);
                }
            } else {
                painter.text(
                    frame.rect.center_top() + vec2(0.0, 24.0),
                    egui::Align2::CENTER_CENTER,
                    t!("Too many points to show. Press Simplify first."),
                    egui::FontId::proportional(13.0),
                    Color32::WHITE,
                );
            }
        } else if self.sel.len() > 1 {
            let b = self.sel.iter().filter_map(|&i| screen_box(frame, m, art, s.shapes.get(i)?)).reduce(|a, b| a.union(b));
            if let Some(b) = b {
                dashed_rect(b.expand(4.0));
            }
        }
        let guide = egui::Stroke::new(1.0, Color32::from_rgb(0xff, 0x40, 0xa0));
        for &(vertical, t) in &self.guides {
            let (a, b) = if vertical { ((t, -1e5), (t, 1e5)) } else { ((-1e5, t), (1e5, t)) };
            painter.line_segment([frame.to_screen_pt(m.to_world(a)), frame.to_screen_pt(m.to_world(b))], guide);
        }
        if let (Some(Drag::Band { start, .. }), Some(p)) = (&self.drag, painter.ctx().pointer_latest_pos()) {
            let r = egui::Rect::from_two_pos(*start, p);
            painter.rect_filled(r, 0, accent.gamma_multiply(0.12));
            painter.rect_stroke(r, 0, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
        }
    }

    /// Keys for the selected artwork shapes; they take precedence over the
    /// same keys for the whole design.
    pub(super) fn shape_keys(&mut self, ctx: &egui::Context) {
        let Some(m) = self.shape_map() else { return };
        let press = |m: Modifiers, k: Key| ctx.input_mut(|inp| inp.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if press(Modifiers::COMMAND, Key::A) {
            self.arrange(Arrange::SelectAll);
        }
        if self.primary().is_none() {
            return;
        }
        let keys = [
            (Modifiers::NONE, Key::Escape, Arrange::Deselect),
            (Modifiers::NONE, Key::Delete, Arrange::Delete),
            (Modifiers::NONE, Key::Backspace, Arrange::Delete),
            (Modifiers::COMMAND, Key::D, Arrange::Duplicate),
            (Modifiers::NONE, Key::CloseBracket, Arrange::Forward),
            (Modifiers::NONE, Key::OpenBracket, Arrange::Backward),
            (Modifiers::SHIFT, Key::CloseBracket, Arrange::Front),
            (Modifiers::SHIFT, Key::OpenBracket, Arrange::Back),
            (Modifiers::COMMAND, Key::H, Arrange::Hide),
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::G, Arrange::Ungroup),
            (Modifiers::COMMAND, Key::G, Arrange::Group),
        ];
        for (mods, key, op) in keys {
            if press(mods, key) {
                self.arrange(op);
                return;
            }
        }
        let Some(s) = &mut self.art else { return };
        for (key, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            for (mods, step) in [(Modifiers::NONE, 10.0), (Modifiers::SHIFT, 100.0)] {
                if press(mods, key) {
                    // Arrows move on the hoop, so undo the design's own turn first.
                    let (ux, uy) = m.t.invert_delta((dx * step, dy * step));
                    for &i in &self.sel {
                        if let Some(e) = s.shapes.get_mut(i) {
                            e.dx += ux / m.scale;
                            e.dy += uy / m.scale;
                        }
                    }
                }
            }
        }
    }

    /// The numbers of a shape drawn with a shape tool, to change it later.
    fn form_card(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent();
        let Some(i) = (self.sel.len() == 1).then(|| self.sel[0]) else { return };
        let Some(map) = self.shape_map().or_else(|| self.draw_map()) else { return };
        let Some(e) = self.art.as_ref().and_then(|s| s.shapes.get(i)) else { return };
        let Some(made) = e.made else { return };
        // Sizes show in mm, as the shape is scaled on the hoop.
        let mm = map.scale / 10.0 * e.scale;
        let mut m = made;
        card(ui, t!("Form"), accent, |ui| {
            let size = |ui: &mut egui::Ui, w: &mut f32, h: &mut f32| {
                ui.horizontal(|ui| {
                    let (mut a, mut b) = (*w * mm, *h * mm);
                    ui.label(t!("Width"));
                    let c1 = ui.add(egui::DragValue::new(&mut a).range(1.0..=500.0).speed(0.5).suffix(" mm").max_decimals(1)).changed();
                    ui.label(t!("Height"));
                    let c2 = ui.add(egui::DragValue::new(&mut b).range(1.0..=500.0).speed(0.5).suffix(" mm").max_decimals(1)).changed();
                    if c1 || c2 {
                        (*w, *h) = (a / mm, b / mm);
                    }
                });
            };
            match &mut m {
                artwork::Made::Rect { w, h, round } => {
                    size(ui, w, h);
                    let mut pct = *round * 200.0;
                    if slider_row(ui, t!("Round corners"), &mut pct, 0.0..=100.0, " %", 0).changed() {
                        *round = pct / 200.0;
                    }
                }
                artwork::Made::Ellipse { w, h } => size(ui, w, h),
                artwork::Made::Polygon { w, h, sides } => {
                    size(ui, w, h);
                    ui.horizontal(|ui| {
                        ui.label(t!("Sides"));
                        ui.add(egui::DragValue::new(sides).range(3..=24));
                    });
                }
                artwork::Made::Star { w, h, points, inner } => {
                    size(ui, w, h);
                    ui.horizontal(|ui| {
                        ui.label(t!("Star points"));
                        ui.add(egui::DragValue::new(points).range(3..=24));
                    });
                    let mut pct = *inner * 100.0;
                    if slider_row(ui, t!("Inner size"), &mut pct, 10.0..=90.0, " %", 0).changed() {
                        *inner = pct / 100.0;
                    }
                }
                artwork::Made::Line { bend, from, to, .. } => {
                    let mut pct = *bend * 100.0;
                    if slider_row(ui, t!("Bend"), &mut pct, -50.0..=50.0, " %", 0).changed() {
                        *bend = pct / 100.0;
                    }
                    if from.is_some() || to.is_some() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(t!("Its green ends follow the shapes they are on.")).size(12.0));
                            if ui.button(t!("Detach")).clicked() {
                                (*from, *to) = (None, None);
                            }
                        });
                    }
                }
            }
        });
        if m != made
            && let Some(e) = self.art.as_mut().and_then(|s| s.shapes.get_mut(i))
        {
            e.made = Some(m);
            e.remake();
        }
    }

    /// The Shape tab: what can be done to the picked shapes.
    pub(super) fn shape_tab(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        let accent = self.accent();
        card(ui, t!("Add"), accent, |ui| {
            let w = (ui.available_width() - 12.0) / 4.0;
            for row in artwork::BUILT_IN.chunks(4) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for name in row {
                        if ui.add(egui::Button::new(crate::i18n::tr(name)).min_size(vec2(w, 24.0))).clicked() {
                            self.list_add = Some(name);
                        }
                    }
                });
            }
        });
        if let Some(name) = self.list_add.take() {
            self.add_built_in(name);
        }
        if self.sel.is_empty() {
            ui.label(
                RichText::new(t!("Click a shape on the hoop or in Objects. Drag on empty fabric to pick several; Ctrl+click adds one."))
                    .size(12.0)
                    .color(muted),
            );
            return;
        }
        self.text_card(ui);
        self.form_card(ui);
        self.tool_card(ui);
        let mut op = None;
        card_folded(ui, t!("Arrange"), accent, |ui| {
            let w = (ui.available_width() - 12.0) / 4.0;
            let rows: [[(&str, &str, Arrange); 4]; 3] = [
                [
                    (t!("Front"), t!("Sew last, on top · Shift+]"), Arrange::Front),
                    (t!("Forward"), t!("One step up · ]"), Arrange::Forward),
                    (t!("Backward"), t!("One step down · ["), Arrange::Backward),
                    (t!("Back"), t!("Sew first, underneath · Shift+["), Arrange::Back),
                ],
                [
                    (t!("Left"), t!("Align left edges"), Arrange::Left),
                    (t!("Center"), t!("Align middles across"), Arrange::CenterX),
                    (t!("Right"), t!("Align right edges"), Arrange::Right),
                    (t!("Top"), t!("Align top edges"), Arrange::Top),
                ],
                [
                    (t!("Middle"), t!("Align middles up and down"), Arrange::Middle),
                    (t!("Bottom"), t!("Align bottom edges"), Arrange::Bottom),
                    (t!("Copy"), t!("Duplicate · Ctrl+D"), Arrange::Duplicate),
                    (t!("Delete"), "Del", Arrange::Delete),
                ],
            ];
            for row in rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (label, hover, a) in row {
                        if ui.add(egui::Button::new(label).min_size(vec2(w, 24.0))).on_hover_text(hover).clicked() {
                            op = Some(a);
                        }
                    }
                });
            }
            if self.sel.len() == 1 {
                ui.label(RichText::new(t!("One shape aligns to the drawing's frame; several align to each other.")).size(11.5).color(muted));
            }
        });
        if let Some(op) = op {
            self.arrange(op);
            return;
        }
        let (Some(art), Some(s)) = (&self.artwork, &mut self.art) else { return };
        let sel = self.sel.clone();
        card(ui, t!("Thread color"), accent, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (l, layer) in art.layers.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    ui.painter().rect_filled(r.shrink(2.0), 4, rgb(layer.color));
                    if sel.iter().all(|&i| s.shapes[i].layer == l) {
                        ui.painter().rect_stroke(r, 4, egui::Stroke::new(2.0, accent), egui::StrokeKind::Inside);
                    }
                    if resp.on_hover_text(t!("Stitch with this color's thread (set in Threads)")).clicked() {
                        for &i in &sel {
                            s.shapes[i].layer = l;
                        }
                        if let Some(c) = s.layers.get_mut(l) {
                            c.enabled = true;
                        }
                    }
                }
            });
        });
        card(ui, t!("Stitch as"), accent, |ui| {
            // One look for all picked shapes, or the thread's look (Auto).
            let first = s.shapes[sel[0]].look;
            let same = sel.iter().all(|&i| s.shapes[i].look == first);
            let name = |l: Option<Look>| match l {
                None => t!("Thread's look").to_string(),
                Some(Look::Default) => t!("Fill style").to_string(),
                Some(Look::Fill(k)) => crate::i18n::tr(crate::lettering::fill_name(k)).to_string(),
                Some(Look::Outline) => t!("Outline").into(),
                Some(Look::Applique) => t!("Appliqué").into(),
                Some(Look::Hole) => t!("Leave fabric").into(),
            };
            let mut look = first;
            egui::ComboBox::from_id_salt("shape-look").selected_text(if same { name(first) } else { t!("Mixed").into() }).width(ui.available_width()).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(&mut look, None, t!("Thread's look")).on_hover_text(t!("Whatever the Threads tab sets for this color"));
                    for k in [FillKind::Auto, FillKind::Column, FillKind::Tatami, FillKind::Satin, FillKind::Contour, FillKind::Wave, FillKind::Open] {
                        ui.selectable_value(&mut look, Some(Look::Fill(k)), crate::i18n::tr(crate::lettering::fill_name(k)).to_string());
                    }
                    ui.selectable_value(&mut look, Some(Look::Outline), t!("Outline")).on_hover_text(t!("Only the outline: for thin shapes"));
                    ui.selectable_value(&mut look, Some(Look::Applique), t!("Appliqué"));
                    ui.selectable_value(&mut look, Some(Look::Hole), t!("Leave fabric"))
                        .on_hover_text(t!("Not sewn; the fabric shows here, and nothing below is sewn through it"));
                },
            );
            if look != first {
                for &i in &sel {
                    s.shapes[i].look = look;
                }
            }
            ui.label(RichText::new(t!("Satin suits parts under about 7 mm wide, tatami wider ones, outline anything under 1 mm.")).size(11.5).color(muted));
        });
        let [i] = sel[..] else {
            ui.label(RichText::new(t!("{v0} shapes picked. Drag them on the hoop or nudge with the arrow keys.", v0 = sel.len())).size(12.0).color(muted));
            return;
        };
        let (scale, _) = artwork::frame(art, s);
        let e = &mut s.shapes[i];
        card(ui, t!("Transform"), accent, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t!("Name")).color(muted));
                ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(ui.available_width()));
            });
            // Offsets are user units; show them as mm on the fabric.
            let mm = scale / 10.0;
            let (mut x, mut y) = (e.dx * mm, e.dy * mm);
            slider_row(ui, t!("Move X"), &mut x, -150.0..=150.0, " mm", 1).on_hover_text(t!("Or drag the shape on the hoop, or nudge with the arrow keys"));
            slider_row(ui, t!("Move Y"), &mut y, -150.0..=150.0, " mm", 1);
            if (x - e.dx * mm).abs() > 1e-4 || (y - e.dy * mm).abs() > 1e-4 {
                (e.dx, e.dy) = (x / mm, y / mm);
            }
            let mut size = e.scale * 100.0;
            if slider_row(ui, t!("Size"), &mut size, 10.0..=400.0, " %", 0).on_hover_text(t!("Or drag a corner handle")).changed() {
                e.scale = (size / 100.0).max(0.05);
            }
            slider_row(ui, t!("Turn"), &mut e.rotate, -180.0..=180.0, "°", 0).on_hover_text(t!("Or drag the round knob; Shift snaps to 15°"));
            let b = artwork::extent(&e.lines(art));
            if b.0 <= b.2 {
                ui.label(
                    RichText::new(t!("{v0:.1} × {v1:.1} mm on the fabric", v0 = (b.2 - b.0) * mm, v1 = (b.3 - b.1) * mm)).monospace().size(12.0).color(muted),
                );
            }
        });
        let e = &mut s.shapes[i];
        card(ui, t!("Points"), accent, |ui| {
            let points: usize = e.base(art).lines.iter().map(|l| l.0.len()).sum();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(t!("Edit points")).selected(self.edit_points))
                    .on_hover_text(t!("Drag points on the hoop. Right-click a point to remove it, double-click the outline to add one."))
                    .clicked()
                {
                    self.edit_points = !self.edit_points;
                    if self.edit_points {
                        e.bake(art);
                    }
                }
                if ui.button(t!("Simplify")).on_hover_text(t!("Fewer points, smoother to edit; keeps the shape within 0.3 mm")).clicked() {
                    e.bake(art);
                    if let Some(own) = &mut e.own {
                        let eps = 3.0 / scale;
                        for (c, closed) in &mut own.lines {
                            let fewer = if *closed { trace::simplify_closed(c, eps) } else { trace::simplify(c, eps) };
                            if fewer.len() >= if *closed { 3 } else { 2 } {
                                *c = fewer;
                            }
                        }
                    }
                }
                let can_reset = e.moved() || (e.own.is_some() && e.source < art.shapes.len());
                if ui.add_enabled(can_reset, egui::Button::new("Reset")).on_hover_text(t!("Back to the shape as drawn")).clicked() {
                    (e.dx, e.dy, e.scale, e.rotate) = (0.0, 0.0, 1.0, 0.0);
                    if e.source < art.shapes.len() {
                        e.own = None;
                    }
                }
            });
            ui.label(RichText::new(tn!(points, "{n} point", "{n} points")).size(12.0).color(muted));
        });
    }
}
