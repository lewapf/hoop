//! Text in Artwork: the Text tool places a word on the drawing as glyph
//! outlines from a font, which stitch like any other shape. The text keeps
//! what it says, its font, size and shape, so the Text card can change it
//! and the outlines are drawn again.

use super::*;
use crate::artwork::{ArtText, Kind, Shape};
use crate::lettering::{Contour, TextShape};

impl HoopApp {
    /// The text's outlines in user units, centered on 0, 0.
    fn text_lines(&self, t: &ArtText, scale: f32) -> Option<Vec<(Contour, bool)>> {
        let font = self.fonts.iter().find(|f| f.name == t.font).or(self.fonts.first())?;
        let mm = |u: f32| u * scale / 10.0;
        let s = Settings {
            text: t.text.clone(),
            height_mm: mm(t.height).max(1.0),
            spacing_mm: mm(t.spacing),
            shape: t.shape,
            arc_deg: t.arc_deg,
            envelope_mm: mm(t.envelope),
            circle_mm: mm(t.circle).max(10.0),
            ..Settings::default()
        };
        let glyphs = crate::lettering::outlines(font, &s).ok()?;
        let mut lines: Vec<(Contour, bool)> =
            glyphs.into_iter().flatten().map(|c| (c.into_iter().map(|p| (p.0 / scale, p.1 / scale)).collect(), true)).collect();
        if lines.is_empty() {
            return None;
        }
        let (cx, cy) = artwork::middle(&lines);
        for p in lines.iter_mut().flat_map(|l| l.0.iter_mut()) {
            *p = (p.0 - cx, p.1 - cy);
        }
        Some(lines)
    }

    /// A short name for a text object: its first line.
    fn text_name(t: &ArtText) -> String {
        let first: String = t.text.lines().next().unwrap_or("").chars().take(24).collect();
        if first.trim().is_empty() { t!("Text").into() } else { first }
    }

    /// Places new text, 15 mm high, centered on `at` (user units), picks it
    /// and opens the Shape page to type it.
    pub(super) fn add_text(&mut self, at: (f32, f32)) {
        let Some(m) = self.draw_map() else { return };
        let font = self.fonts.get(self.letter.font).or(self.fonts.first()).map_or(String::new(), |f| f.name.clone());
        let t = ArtText {
            text: t!("Text").into(),
            font,
            height: 150.0 / m.scale,
            spacing: 0.0,
            shape: TextShape::Straight,
            arc_deg: 120.0,
            envelope: 80.0 / m.scale,
            circle: 600.0 / m.scale,
        };
        let Some(mut lines) = self.text_lines(&t, m.scale) else { return };
        for p in lines.iter_mut().flat_map(|l| l.0.iter_mut()) {
            *p = (p.0 + at.0, p.1 + at.1);
        }
        let own = Shape { kind: Kind::Fill { even_odd: false }, lines, layer: self.paint_layer() };
        self.push_shape(&Self::text_name(&t), own, None);
        if let Some(e) = self.art.as_mut().and_then(|s| s.shapes.last_mut()) {
            e.text = Some(t);
        }
        self.focus_text = true;
        self.show_right = true;
        self.tab[Mode::Artwork as usize] = shell::Tab::Shape;
    }

    /// Draws a text object again from its settings, keeping its middle.
    fn retext(&mut self, i: usize) {
        let Some(m) = self.shape_map().or_else(|| self.draw_map()) else { return };
        let Some(t) = self.art.as_ref().and_then(|s| s.shapes.get(i)).and_then(|e| e.text.clone()) else { return };
        let Some(mut lines) = self.text_lines(&t, m.scale) else { return };
        let Some(e) = self.art.as_mut().and_then(|s| s.shapes.get_mut(i)) else { return };
        let Some(own) = &mut e.own else { return };
        let (cx, cy) = artwork::middle(&own.lines);
        for p in lines.iter_mut().flat_map(|l| l.0.iter_mut()) {
            *p = (p.0 + cx, p.1 + cy);
        }
        own.lines = lines;
        e.name = Self::text_name(&t);
    }

    /// The Text card of the Shape tab, for a picked text object.
    pub(super) fn text_card(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent();
        let Some(i) = (self.sel.len() == 1).then(|| self.sel[0]) else { return };
        let Some(map) = self.shape_map().or_else(|| self.draw_map()) else { return };
        let Some(e) = self.art.as_ref().and_then(|s| s.shapes.get(i)) else { return };
        let Some(old) = e.text.clone() else { return };
        // Sizes show in mm, as the text is scaled on the hoop.
        let mm = map.scale / 10.0 * e.scale;
        let mut t = old.clone();
        let fonts: Vec<String> = self.fonts.iter().map(|f| f.name.clone()).collect();
        let focus = std::mem::take(&mut self.focus_text);
        card(ui, t!("Text"), accent, |ui| {
            let edit = ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(2).desired_width(ui.available_width()));
            if focus {
                edit.request_focus();
                // Typing replaces the placeholder.
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit.id) {
                    let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(t.text.chars().count()));
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), edit.id);
                }
            }
            ui.horizontal(|ui| {
                ui.label(t!("Font"));
                egui::ComboBox::from_id_salt("art-text-font").selected_text(&t.font).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                    for f in &fonts {
                        ui.selectable_value(&mut t.font, f.clone(), f);
                    }
                });
            });
            let mut h = t.height * mm;
            if slider_row(ui, t!("Height"), &mut h, 3.0..=120.0, " mm", 1).changed() {
                t.height = h / mm;
            }
            let mut sp = t.spacing * mm;
            if slider_row(ui, t!("Spacing"), &mut sp, -3.0..=10.0, " mm", 1).changed() {
                t.spacing = sp / mm;
            }
            ui.horizontal(|ui| {
                ui.label(t!("Shape"));
                egui::ComboBox::from_id_salt("art-text-shape").selected_text(crate::i18n::tr(t.shape.name())).show_ui(ui, |ui| {
                    for k in TextShape::ALL {
                        ui.selectable_value(&mut t.shape, k, crate::i18n::tr(k.name()));
                    }
                });
            });
            match t.shape {
                TextShape::Arc => {
                    slider_row(ui, t!("Arc"), &mut t.arc_deg, -300.0..=300.0, "°", 0);
                }
                TextShape::Circle => {
                    let mut c = t.circle * mm;
                    if slider_row(ui, t!("Circle"), &mut c, 20.0..=200.0, " mm", 0).changed() {
                        t.circle = c / mm;
                    }
                }
                k if k.bends() => {
                    let mut v = t.envelope * mm;
                    if slider_row(ui, t!("Bend"), &mut v, 0.0..=60.0, " mm", 1).changed() {
                        t.envelope = v / mm;
                    }
                }
                _ => {}
            }
        });
        if t != old
            && let Some(e) = self.art.as_mut().and_then(|s| s.shapes.get_mut(i))
        {
            e.text = Some(t);
            self.retext(i);
        }
    }
}
