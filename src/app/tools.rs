//! The drawing tools of Artwork: the tool palette on the hoop's left edge,
//! the options strip beside it, and what each tool does with the pointer.
//! Select, Edit points and the Pen live elsewhere (shapes.rs, draw.rs); the
//! palette only switches them. The tools here draw rectangles, ellipses,
//! polygons and stars that keep their numbers, lines that snap to shape
//! outlines and bend, a bucket that gives shapes the current thread, and
//! the picked shape's stitch angle lines and start point.

use super::*;
use crate::artwork::{self, Made};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ShapeTool {
    /// Drag a line across the shape; the fill's rows follow it.
    Angle,
    /// Click where the needle should start the shape.
    Entry,
    /// Drag out a rectangle, ellipse, polygon or star.
    Shape,
    /// Drag a line; its ends snap to shape outlines.
    Line,
    /// Click a shape to give it the current thread.
    Fill,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Form {
    #[default]
    Rect,
    Ellipse,
    Polygon,
    Star,
}

impl Form {
    fn name(self) -> &'static str {
        match self {
            Form::Rect => t!("Rectangle"),
            Form::Ellipse => t!("Ellipse"),
            Form::Polygon => t!("Polygon"),
            Form::Star => t!("Star"),
        }
    }
}

pub(super) struct Tools {
    pub tool: Option<ShapeTool>,
    /// The thread picked in the tool palette for new shapes and the bucket.
    pub paint: Option<usize>,
    /// Where a drag began and where it is now, on the hoop.
    drag: Option<((f32, f32), (f32, f32))>,
    /// Where a line end would snap to under the pointer, on the hoop.
    snap: Option<(f32, f32)>,
    /// The shape the line being drawn starts on.
    from: Option<artwork::Anchor>,
    pub form: Form,
    /// Rectangle corner rounding, a share of the shorter side.
    pub round: f32,
    pub sides: u32,
    pub points: u32,
    /// Star inner radius, a share of the outer.
    pub inner: f32,
    /// How far new lines bow, a share of their length.
    pub bend: f32,
}

impl Default for Tools {
    fn default() -> Self {
        Tools { tool: None, paint: None, drag: None, snap: None, from: None, form: Form::Rect, round: 0.0, sides: 6, points: 5, inner: 0.45, bend: 0.0 }
    }
}

/// What the palette shows as on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Pick {
    Select,
    Points,
    Pen,
    Shape(Form),
    Line,
    Fill,
    Angle,
    Start,
}

/// Snapping reach for line ends, in screen points.
const SNAP: f32 = 10.0;

/// The point on a polyline nearest `q`, and its distance.
fn nearest_on(c: &[egui::Pos2], closed: bool, q: egui::Pos2) -> Option<(egui::Pos2, f32)> {
    let n = c.len();
    if n == 0 {
        return None;
    }
    let segs = if closed { n } else { n - 1 };
    let mut best = (c[0], (c[0] - q).length());
    for k in 0..segs {
        let (a, b) = (c[k], c[(k + 1) % n]);
        let ab = b - a;
        let t = if ab.length_sq() > 0.0 { ((q - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
        let p = a + ab * t;
        let d = (p - q).length();
        if d < best.1 {
            best = (p, d);
        }
    }
    Some(best)
}

impl HoopApp {
    /// The picked shape's index, when artwork is open.
    fn tool_target(&self) -> Option<usize> {
        (self.mode == Mode::Artwork && self.art.is_some()).then(|| self.primary()).flatten()
    }

    pub(super) fn pick(&self) -> Pick {
        if self.pen.on {
            return Pick::Pen;
        }
        match self.tools.tool {
            Some(ShapeTool::Shape) => Pick::Shape(self.tools.form),
            Some(ShapeTool::Line) => Pick::Line,
            Some(ShapeTool::Fill) => Pick::Fill,
            Some(ShapeTool::Angle) => Pick::Angle,
            Some(ShapeTool::Entry) => Pick::Start,
            None if self.edit_points => Pick::Points,
            None => Pick::Select,
        }
    }

    /// Switches to a tool, turning the others off. Drawing tools start a
    /// blank drawing when there is none.
    pub(super) fn set_pick(&mut self, pick: Pick) {
        self.pen.on = false;
        self.pen.points.clear();
        self.tools.tool = None;
        self.tools.drag = None;
        self.edit_points = false;
        let draws = matches!(pick, Pick::Pen | Pick::Shape(_) | Pick::Line);
        if draws && self.art.is_none() {
            self.picture = None;
            self.use_artwork(Ok(artwork::blank()));
        }
        match pick {
            Pick::Select => {}
            Pick::Points => self.edit_points = self.tool_target().is_some(),
            Pick::Pen => self.pen.on = true,
            Pick::Shape(f) => {
                self.tools.form = f;
                self.tools.tool = Some(ShapeTool::Shape);
            }
            Pick::Line => self.tools.tool = Some(ShapeTool::Line),
            Pick::Fill => self.tools.tool = Some(ShapeTool::Fill),
            Pick::Angle => self.tools.tool = self.tool_target().map(|_| ShapeTool::Angle),
            Pick::Start => self.tools.tool = self.tool_target().map(|_| ShapeTool::Entry),
        }
    }

    /// Pointer input while a tool is on. Returns true when it used the
    /// pointer, so nothing else moves.
    pub(super) fn tool_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        let Some(tool) = self.tools.tool else { return false };
        if self.mode != Mode::Artwork || self.art.is_none() {
            self.tools.tool = None;
            return false;
        }
        if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            self.tools.tool = None;
            self.tools.drag = None;
            return true;
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        match tool {
            ShapeTool::Angle | ShapeTool::Entry => self.mark_input(tool, response, frame),
            ShapeTool::Shape => self.shape_tool_input(ui, response, frame),
            ShapeTool::Line => self.line_tool_input(response, frame),
            ShapeTool::Fill => self.fill_tool_input(ui, response, frame),
        }
    }

    /// Stitch angle lines and the start point, on the picked shape.
    fn mark_input(&mut self, tool: ShapeTool, response: &egui::Response, frame: &Frame) -> bool {
        let Some(i) = self.tool_target() else {
            self.tools.tool = None;
            return false;
        };
        let Some(m) = self.shape_map() else { return false };
        let at = response.interact_pointer_pos().map(|p| frame.to_world(p));
        if tool == ShapeTool::Entry {
            if response.clicked()
                && let (Some(w), Some(art)) = (at, &self.artwork)
            {
                let s = self.art.as_mut().unwrap();
                let e = &mut s.shapes[i];
                e.entry = Some(e.unplace(art, m.to_user(w)));
                self.tools.tool = None;
            }
            return true;
        }
        if response.drag_started()
            && let Some(w) = at
        {
            self.tools.drag = Some((w, w));
        }
        if let (Some(d), Some(w)) = (&mut self.tools.drag, at) {
            d.1 = w;
        }
        if response.drag_stopped()
            && let (Some((a, b)), Some(art)) = (self.tools.drag.take(), &self.artwork)
        {
            // A click or a tiny drag isn't a direction.
            if (a.0 - b.0).hypot(a.1 - b.1) > 20.0 {
                let s = self.art.as_mut().unwrap();
                let e = &mut s.shapes[i];
                let (ua, ub) = (e.unplace(art, m.to_user(a)), e.unplace(art, m.to_user(b)));
                e.angles.push((ua, ub));
            }
        }
        true
    }

    /// The made shape for the shape tool's options at a size.
    fn tool_made(&self, w: f32, h: f32) -> Made {
        let t = &self.tools;
        match t.form {
            Form::Rect => Made::Rect { w, h, round: t.round },
            Form::Ellipse => Made::Ellipse { w, h },
            Form::Polygon => Made::Polygon { w, h, sides: t.sides },
            Form::Star => Made::Star { w, h, points: t.points, inner: t.inner },
        }
    }

    /// The box a shape drag covers, in user units: its middle and size.
    /// Shift keeps it square.
    fn drag_box(&self, ui: &egui::Ui, m: shapes::ShapeMap, a: (f32, f32), b: (f32, f32)) -> ((f32, f32), (f32, f32)) {
        let (ua, ub) = (m.to_user(a), m.to_user(b));
        let (mut w, mut h) = ((ub.0 - ua.0).abs(), (ub.1 - ua.1).abs());
        if ui.input(|i| i.modifiers.shift) {
            w = w.max(h);
            h = w;
        }
        let (sx, sy) = ((ub.0 - ua.0).signum(), (ub.1 - ua.1).signum());
        ((ua.0 + sx * w / 2.0, ua.1 + sy * h / 2.0), (w, h))
    }

    fn shape_tool_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        let Some(m) = self.draw_map() else { return false };
        let at = response.interact_pointer_pos().map(|p| frame.to_world(p));
        if response.drag_started()
            && let Some(w) = at
        {
            self.tools.drag = Some((w, w));
        }
        if let (Some(d), Some(w)) = (&mut self.tools.drag, at) {
            d.1 = w;
        }
        let made = if response.drag_stopped()
            && let Some((a, b)) = self.tools.drag.take()
        {
            let (mid, (w, h)) = self.drag_box(ui, m, a, b);
            // Under 2 mm is a slip, not a shape.
            (w.max(h) * m.scale >= 20.0).then(|| (mid, self.tool_made(w.max(1.0), h.max(1.0))))
        } else if response.clicked()
            && let Some(w) = at
        {
            // A click makes one 30 mm across.
            let size = 300.0 / m.scale;
            Some((m.to_user(w), self.tool_made(size, size)))
        } else {
            None
        };
        if let Some((mid, made)) = made {
            let mut lines = artwork::made_lines(&made);
            for p in lines.iter_mut().flat_map(|l| l.0.iter_mut()) {
                *p = (p.0 + mid.0, p.1 + mid.1);
            }
            let own = artwork::Shape { kind: artwork::Kind::Fill { even_odd: false }, lines, layer: self.paint_layer() };
            self.push_shape(self.tools.form.name(), own, Some(made));
        }
        true
    }

    /// Where a line end lands: on the nearest shape outline within reach,
    /// else under the pointer. On the hoop.
    /// Also gives the anchor that keeps the end on that shape.
    fn snapped(&self, frame: &Frame, p: egui::Pos2) -> ((f32, f32), Option<artwork::Anchor>) {
        let (Some(m), Some(art), Some(s)) = (self.draw_map(), &self.artwork, &self.art) else { return (frame.to_world(p), None) };
        let mut best: Option<(egui::Pos2, f32, usize)> = None;
        for (i, e) in s.shapes.iter().enumerate().filter(|(_, e)| !e.hidden && s.layers.get(e.layer).is_none_or(|l| l.enabled)) {
            for (c, closed) in e.lines(art) {
                let pts: Vec<egui::Pos2> = c.iter().map(|&u| frame.to_screen_pt(m.to_world(u))).collect();
                if let Some(hit) = nearest_on(&pts, closed, p)
                    && hit.1 <= SNAP
                    && best.is_none_or(|b| hit.1 < b.1)
                {
                    best = Some((hit.0, hit.1, i));
                }
            }
        }
        match best {
            Some((q, _, i)) => {
                let w = frame.to_world(q);
                let e = &s.shapes[i];
                (w, Some(artwork::Anchor { shape: e.id, at: e.unplace(art, m.to_user(w)) }))
            }
            None => (frame.to_world(p), None),
        }
    }

    fn line_tool_input(&mut self, response: &egui::Response, frame: &Frame) -> bool {
        let Some(m) = self.draw_map() else { return false };
        let pos = response.interact_pointer_pos().or(response.hover_pos());
        let hit = pos.map(|p| self.snapped(frame, p));
        self.tools.snap = hit.filter(|h| h.1.is_some()).map(|h| h.0);
        if response.drag_started()
            && let Some((w, anchor)) = hit
        {
            self.tools.drag = Some((w, w));
            self.tools.from = anchor;
        }
        if let (Some(d), Some((w, _))) = (&mut self.tools.drag, hit) {
            d.1 = w;
        }
        if response.drag_stopped()
            && let Some((a, b)) = self.tools.drag.take()
            && (a.0 - b.0).hypot(a.1 - b.1) >= 20.0
        {
            // The end let go over a shape stays on it too.
            let to = hit.and_then(|h| h.1);
            let made = Made::Line { a: m.to_user(a), b: m.to_user(b), bend: self.tools.bend, from: self.tools.from.take(), to };
            let own = artwork::Shape { kind: artwork::Kind::Stroke { width: 12.0 / m.scale }, lines: artwork::made_lines(&made), layer: self.paint_layer() };
            self.push_shape(t!("Line"), own, Some(made));
        }
        true
    }

    fn fill_tool_input(&mut self, ui: &egui::Ui, response: &egui::Response, frame: &Frame) -> bool {
        if !response.clicked() {
            return true;
        }
        let (Some(m), Some(art), Some(p)) = (self.draw_map(), &self.artwork, response.interact_pointer_pos()) else { return true };
        let s = self.art.as_ref().unwrap();
        let Some(i) = artwork::shape_at(art, s, m.to_stitch(m.to_user(frame.to_world(p))), 6.0 / self.view.scale) else { return true };
        if ui.input(|inp| inp.modifiers.alt) {
            // Alt+click picks the shape's thread up instead.
            self.tools.paint = Some(s.shapes[i].layer);
            return true;
        }
        let layer = self.paint_layer();
        let s = self.art.as_mut().unwrap();
        s.shapes[i].layer = layer;
        if let Some(l) = s.layers.get_mut(layer) {
            l.enabled = true;
        }
        self.sel = vec![i];
        true
    }

    /// The picked shape's angle lines and start point, the shape or line
    /// being drawn, and the tool hint.
    pub(super) fn tool_overlay(&self, painter: &egui::Painter, frame: &Frame) {
        let accent = self.accent();
        if let (Some(i), Some(m), Some(art)) = (self.tool_target(), self.shape_map(), &self.artwork) {
            let e = &self.art.as_ref().unwrap().shapes[i];
            let screen = |p: (f32, f32)| frame.to_screen_pt(m.to_world(e.place(art, p)));
            for &(a, b) in &e.angles {
                arrow(painter, screen(a), screen(b), accent);
            }
            if let Some(p) = e.entry {
                let c = screen(p);
                painter.circle(c, 6.0, Color32::from_rgb(0x55, 0xc2, 0x8c), egui::Stroke::new(1.5, Color32::WHITE));
                painter.text(c + vec2(9.0, -9.0), egui::Align2::LEFT_BOTTOM, t!("Start"), egui::FontId::proportional(12.0), Color32::WHITE);
            }
        }
        // Attached ends of picked lines, as small rings.
        if let (Some(m), Some(art), Some(s)) = (self.shape_map(), &self.artwork, &self.art) {
            for &i in &self.sel {
                let Some(e) = s.shapes.get(i) else { continue };
                if let Some(Made::Line { a, b, from, to, .. }) = e.made {
                    for (p, on) in [(a, from.is_some()), (b, to.is_some())] {
                        if on {
                            let c = frame.to_screen_pt(m.to_world(e.place(art, p)));
                            painter.circle(c, 5.0, Color32::from_rgb(0x55, 0xc2, 0x8c), egui::Stroke::new(1.5, Color32::WHITE));
                        }
                    }
                }
            }
        }
        let Some(tool) = self.tools.tool else { return };
        let stroke = egui::Stroke::new(1.5, accent);
        match (tool, self.tools.drag) {
            (ShapeTool::Angle, Some((a, b))) => arrow(painter, frame.to_screen_pt(a), frame.to_screen_pt(b), accent),
            (ShapeTool::Shape, Some((a, b))) => {
                if let Some(m) = self.draw_map() {
                    let ctx = painter.ctx().clone();
                    // The pointer's own Ui isn't at hand here; Shift is read from the context.
                    let shift = ctx.input(|i| i.modifiers.shift);
                    let (ua, ub) = (m.to_user(a), m.to_user(b));
                    let (mut w, mut h) = ((ub.0 - ua.0).abs(), (ub.1 - ua.1).abs());
                    if shift {
                        w = w.max(h);
                        h = w;
                    }
                    let mid = (ua.0 + (ub.0 - ua.0).signum() * w / 2.0, ua.1 + (ub.1 - ua.1).signum() * h / 2.0);
                    for (c, _) in artwork::made_lines(&self.tool_made(w.max(0.1), h.max(0.1))) {
                        let pts: Vec<egui::Pos2> = c.iter().map(|&p| frame.to_screen_pt(m.to_world((p.0 + mid.0, p.1 + mid.1)))).collect();
                        painter.add(egui::Shape::closed_line(pts, stroke));
                    }
                    let mm = |u: f32| u * m.scale / 10.0;
                    painter.text(
                        frame.to_screen_pt(b) + vec2(12.0, 12.0),
                        egui::Align2::LEFT_TOP,
                        format!("{:.1} × {:.1} mm", mm(w), mm(h)),
                        egui::FontId::monospace(11.0),
                        accent,
                    );
                }
            }
            (ShapeTool::Line, drag) => {
                if let (Some((a, b)), Some(m)) = (drag, self.draw_map()) {
                    let made = Made::Line { a: m.to_user(a), b: m.to_user(b), bend: self.tools.bend, from: None, to: None };
                    let pts: Vec<egui::Pos2> = artwork::made_lines(&made)[0].0.iter().map(|&p| frame.to_screen_pt(m.to_world(p))).collect();
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, accent)));
                    painter.circle_stroke(frame.to_screen_pt(a), 4.0, stroke);
                }
                if let Some(q) = self.tools.snap {
                    painter.circle(frame.to_screen_pt(q), 5.0, accent.gamma_multiply(0.4), egui::Stroke::new(1.5, Color32::WHITE));
                }
            }
            _ => {}
        }
        let tip = match tool {
            ShapeTool::Angle => t!("Drag across the shape in the direction the stitches should run · Esc to stop"),
            ShapeTool::Entry => t!("Click where the needle should start this shape · Esc to stop"),
            ShapeTool::Shape => t!("Drag to draw, Shift for even sides, or click for one 30 mm across · Esc to stop"),
            ShapeTool::Line => t!("Drag a line; its ends snap to shape outlines · Esc to stop"),
            ShapeTool::Fill => t!("Click a shape to give it the current thread · Alt+click takes a shape's thread · Esc to stop"),
        };
        let galley = painter.layout(tip.into(), egui::FontId::proportional(14.0), Color32::WHITE, frame.rect.width() - 80.0);
        let pos = frame.rect.center_bottom() + vec2(-galley.size().x / 2.0, -galley.size().y - 24.0);
        painter.rect_filled(egui::Rect::from_min_size(pos - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0)), 8, Color32::from_black_alpha(190));
        painter.galley(pos, galley, Color32::WHITE);
    }

    /// The color of a thread as chosen for the design.
    fn layer_color(&self, l: usize) -> Color32 {
        let c = self.art.as_ref().and_then(|s| s.layers.get(l)).map_or([128, 128, 128], |l| crate::formats::jef_thread(l.thread, 0).color);
        Color32::from_rgb(c[0], c[1], c[2])
    }

    /// The tool palette down the hoop's left edge in Artwork, and the
    /// options of the tool in use beside it.
    pub(super) fn tool_palette(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        if self.mode != Mode::Artwork || self.playing {
            return;
        }
        let picked = self.tool_target().is_some();
        let has_art = self.art.is_some();
        let now = self.pick();
        let panel = ctx.global_style().visuals.panel_fill;
        let frame = egui::Frame::new()
            .fill(panel)
            .corner_radius(8)
            .inner_margin(egui::Margin::same(3))
            .stroke(egui::Stroke::new(1.0, ctx.global_style().visuals.widgets.noninteractive.bg_stroke.color))
            .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(60) });
        let mut choose: Option<Pick> = None;
        let mut paint: Option<usize> = None;
        let groups: [&[(Pick, &str, &str, bool)]; 4] = [
            &[(Pick::Select, "cursor", t!("Select and move · Esc"), true), (Pick::Points, "bezier", t!("Edit points of the picked shape"), picked)],
            &[
                (Pick::Pen, "pen", t!("Pen: click for corners, drag for curves · B"), true),
                (Pick::Shape(Form::Rect), "rect", t!("Rectangle · U"), true),
                (Pick::Shape(Form::Ellipse), "ellipse", t!("Ellipse · U again"), true),
                (Pick::Shape(Form::Polygon), "polygon", t!("Polygon"), true),
                (Pick::Shape(Form::Star), "star", t!("Star"), true),
                (Pick::Line, "line", t!("Line that snaps to shapes · W"), true),
            ],
            &[(Pick::Fill, "bucket", t!("Fill bucket: give a shape the current thread · K"), has_art)],
            &[(Pick::Angle, "angle", t!("Stitch angle of the picked shape"), picked), (Pick::Start, "start", t!("Start point of the picked shape"), picked)],
        ];
        let threads: Vec<usize> = self.art.as_ref().map_or(Vec::new(), |s| (0..s.layers.len()).collect());
        let current = self.paint_layer();
        let swatch = self.layer_color(current);
        let colors: Vec<Color32> = threads.iter().map(|&l| self.layer_color(l)).collect();
        let names: Vec<String> = self.art.as_ref().map_or(Vec::new(), |s| s.layers.iter().map(|l| crate::formats::jef_thread(l.thread, 0).name).collect());
        let palette =
            egui::Area::new(egui::Id::new("tool-palette")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + vec2(8.0, 8.0)).show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                    for (g, group) in groups.iter().enumerate() {
                        if g > 0 {
                            ui.add_space(2.0);
                            let y = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range().shrink(4.0),
                                y,
                                egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                            );
                            ui.add_space(3.0);
                        }
                        for &(pick, icon, tip, enabled) in group.iter() {
                            if icons::button(ui, enabled, now == pick, icon, tip) {
                                choose = Some(pick);
                            }
                        }
                    }
                    if !threads.is_empty() {
                        ui.add_space(4.0);
                        let (r, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), egui::Sense::click());
                        let sq = egui::Rect::from_center_size(r.center(), vec2(20.0, 20.0));
                        ui.painter().rect(sq, 4, swatch, egui::Stroke::new(1.5, ui.visuals().strong_text_color()), egui::StrokeKind::Outside);
                        let resp = resp.on_hover_text(t!("Current thread for new shapes and the bucket; picking one also gives it to the picked shapes"));
                        egui::Popup::menu(&resp).show(|ui| {
                            for (k, &l) in threads.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                                    ui.painter().rect_filled(r, 3, colors[k]);
                                    if ui.add(egui::Button::selectable(l == current, &names[k])).clicked() {
                                        paint = Some(l);
                                        ui.close();
                                    }
                                });
                            }
                        });
                    }
                });
            });
        // The options of the tool in use, beside the palette.
        let at = palette.response.rect.right_top() + vec2(6.0, 0.0);
        let show_options = matches!(now, Pick::Shape(Form::Rect | Form::Polygon | Form::Star) | Pick::Line);
        if show_options {
            egui::Area::new(egui::Id::new("tool-options")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
                frame.inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let t = &mut self.tools;
                        match now {
                            Pick::Shape(Form::Rect) => {
                                ui.label(t!("Round corners"));
                                let mut pct = t.round * 200.0;
                                if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").integer()).changed() {
                                    t.round = pct / 200.0;
                                }
                            }
                            Pick::Shape(Form::Polygon) => {
                                ui.label(t!("Sides"));
                                ui.add(egui::DragValue::new(&mut t.sides).range(3..=24));
                            }
                            Pick::Shape(Form::Star) => {
                                ui.label(t!("Star points"));
                                ui.add(egui::DragValue::new(&mut t.points).range(3..=24));
                                ui.label(t!("Inner size"));
                                let mut pct = t.inner * 100.0;
                                if ui.add(egui::Slider::new(&mut pct, 10.0..=90.0).suffix(" %").integer()).changed() {
                                    t.inner = pct / 100.0;
                                }
                            }
                            Pick::Line => {
                                ui.label(t!("Bend"));
                                let mut pct = t.bend * 100.0;
                                if ui
                                    .add(egui::Slider::new(&mut pct, -50.0..=50.0).suffix(" %").integer())
                                    .on_hover_text(t!("0 is straight; the sign says which way it bows"))
                                    .changed()
                                {
                                    t.bend = pct / 100.0;
                                }
                            }
                            _ => {}
                        }
                    });
                });
            });
        }
        if let Some(p) = choose {
            self.set_pick(if p == now && p != Pick::Select { Pick::Select } else { p });
        }
        if let Some(l) = paint {
            self.tools.paint = Some(l);
            if let Some(s) = &mut self.art {
                for &i in &self.sel {
                    if let Some(e) = s.shapes.get_mut(i) {
                        e.layer = l;
                    }
                }
                if let Some(c) = s.layers.get_mut(l) {
                    c.enabled = true;
                }
            }
        }
    }

    /// Tool keys in Artwork: B pen, U shapes (again for the next form), W
    /// line, K bucket.
    pub(super) fn tool_keys(&mut self, ctx: &egui::Context) {
        if self.mode != Mode::Artwork || ctx.egui_wants_keyboard_input() {
            return;
        }
        let press = |k: egui::Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
        // Esc leaves a drawing tool before it deselects the shape just drawn.
        if (self.tools.tool.is_some() || self.pen.on || self.edit_points) && self.pen.points.is_empty() && press(egui::Key::Escape) {
            self.set_pick(Pick::Select);
            return;
        }
        if press(egui::Key::B) {
            self.set_pick(Pick::Pen);
        }
        if press(egui::Key::U) {
            let next = match self.pick() {
                Pick::Shape(Form::Rect) => Form::Ellipse,
                Pick::Shape(Form::Ellipse) => Form::Polygon,
                Pick::Shape(Form::Polygon) => Form::Star,
                Pick::Shape(Form::Star) => Form::Rect,
                _ => self.tools.form,
            };
            self.set_pick(Pick::Shape(next));
        }
        if press(egui::Key::W) {
            self.set_pick(Pick::Line);
        }
        if press(egui::Key::K) && self.art.is_some() {
            self.set_pick(Pick::Fill);
        }
    }

    /// The Stitching card of the Shape tab: angle lines and start point.
    pub(super) fn tool_card(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.tool_target() else { return };
        let muted = self.muted();
        let accent = self.accent();
        let (count, has_entry) = {
            let e = &self.art.as_ref().unwrap().shapes[i];
            (e.angles.len(), e.entry.is_some())
        };
        let mut clear_angles = false;
        let mut clear_entry = false;
        card(ui, t!("Stitching"), accent, |ui| {
            ui.horizontal(|ui| {
                let on = self.tools.tool == Some(ShapeTool::Angle);
                if ui
                    .add(egui::Button::selectable(on, t!("Add stitch angle")))
                    .on_hover_text(t!("Drag a line across the shape; its fill rows follow it. Several lines split the fill into parts."))
                    .clicked()
                {
                    self.tools.tool = if on { None } else { Some(ShapeTool::Angle) };
                }
                if ui.add_enabled(count > 0, egui::Button::new(t!("Clear"))).clicked() {
                    clear_angles = true;
                }
            });
            if count > 0 {
                ui.label(RichText::new(tn!(count, "{n} angle line", "{n} angle lines")).size(12.0).color(muted));
            }
            ui.horizontal(|ui| {
                let on = self.tools.tool == Some(ShapeTool::Entry);
                if ui
                    .add(egui::Button::selectable(on, t!("Set start point")))
                    .on_hover_text(t!("Where the needle comes in, to save a jump or hide the tie-in"))
                    .clicked()
                {
                    self.tools.tool = if on { None } else { Some(ShapeTool::Entry) };
                }
                if ui.add_enabled(has_entry, egui::Button::new(t!("Clear"))).clicked() {
                    clear_entry = true;
                }
            });
        });
        let s = self.art.as_mut().unwrap();
        if clear_angles {
            s.shapes[i].angles.clear();
        }
        if clear_entry {
            s.shapes[i].entry = None;
        }
    }
}

/// A line with an arrow head at `b`.
fn arrow(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, color: Color32) {
    let stroke = egui::Stroke::new(2.0, color);
    painter.line_segment([a, b], egui::Stroke::new(4.0, Color32::from_black_alpha(90)));
    painter.line_segment([a, b], stroke);
    let d = b - a;
    if d.length() < 4.0 {
        return;
    }
    let u = d.normalized();
    let side = vec2(-u.y, u.x);
    painter.line_segment([b, b - u * 10.0 + side * 5.0], stroke);
    painter.line_segment([b, b - u * 10.0 - side * 5.0], stroke);
}
